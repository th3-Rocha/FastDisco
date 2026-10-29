//! The heavier part of running a server, beside `admin`: role
//! permissions, a channel's settings and permission overwrites, the
//! server's icon and banner, emoji and stickers, AutoMod rules, timeouts
//! and bans that clear history. Also the gateway events that keep a
//! server's roles, emoji, stickers and details current.

use reqwest::Method;
use serde_json::{Value, json};

use crate::admin::GuildPage;
use crate::app::App;
use crate::backend::Command;
use crate::discord::parse;
use crate::model::{Id, Overwrite};

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    /// A change the gateway shows; only a failure needs telling.
    Done,
    /// Saved: says so on the page.
    Saved,
    RoleCreated,
    AutoMod { guild: Id },
    /// A rule created, changed or deleted: the list is asked for again.
    AutoModChanged,
}

/// What a picked picture is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picking {
    NewGuild,
    GuildIcon,
    GuildBanner,
    Emoji,
    Sticker,
    Avatar,
    ProfileBanner,
}

/// The settings pages' fields while they are edited.
#[derive(Default)]
pub struct Manage {
    pub picking: Option<Picking>,
    /// The role being edited: its permission bits.
    pub role_permissions: u64,
    pub emoji_name: String,
    pub sticker_name: String,
    /// The emoji a sticker stands for, which Discord asks for.
    pub sticker_tag: String,
    /// The channel whose settings are open.
    pub channel: Option<Id>,
    pub channel_name: String,
    pub channel_topic: String,
    pub slowmode: u32,
    pub nsfw: bool,
    /// The overwrite being edited: a role or member, and its bits.
    pub overwrite: Option<Overwrite>,
    pub automod: Option<Vec<Value>>,
    pub rule_name: String,
    pub rule_words: String,
}

#[derive(Clone, Debug)]
pub enum ManageAction {
    PickImage(Picking),
    /// Takes the server's icon or banner away.
    RemoveGuildImage { banner: bool },
    CreateRole,
    DeleteRole(Id),
    /// Flips a permission of the role being edited.
    ToggleRolePermission(u64),
    DeleteEmoji(Id),
    DeleteSticker(Id),
    /// Opens the server settings on a channel's page.
    OpenChannel(Id),
    SaveChannel,
    DeleteChannel,
    /// Edits the overwrite for a role (kind 0) or member (kind 1).
    SelectOverwrite { id: Id, kind: i64 },
    /// A permission of the overwrite: allowed, neutral or denied.
    SetOverwrite { bit: u64, state: i8 },
    SaveOverwrite,
    RemoveOverwrite,
    LoadAutoMod,
    ToggleRule(Id, bool),
    DeleteRule(Id),
    CreateRule,
    /// Keeps someone from talking for `minutes`, or lifts it with 0.
    Timeout { guild: Id, user: Id, minutes: u32 },
    /// Changes how a server (none: direct messages) or one of its
    /// channels notifies.
    Notify(NotifyChange),
    /// The user's own picture taken away.
    RemoveAvatar,
    /// The colour of the user's banner when there is no picture.
    SetAccent(Option<u32>),
}

/// One change to the notification settings.
#[derive(Clone, Debug, Default)]
pub struct NotifyChange {
    pub guild: Option<Id>,
    pub channel: Option<Id>,
    /// 0 every message, 1 mentions, 2 nothing, 3 the server's default.
    pub level: Option<u8>,
    /// Muted for so many minutes (none: until turned back on), or not.
    pub mute: Option<Option<Option<u32>>>,
    pub suppress_everyone: Option<bool>,
    pub suppress_roles: Option<bool>,
}

impl App {
    fn manage_api(&self, tag: Tag, method: Method, path: String, body: Option<Value>) {
        self.backend().send(Command::Api {
            tag: crate::backend::Tag::Manage(tag),
            method,
            path,
            body,
        });
    }

    fn settings_guild(&self) -> Option<Id> {
        self.admin.settings.as_ref().map(|s| s.guild)
    }

    fn page_message(&mut self, text: impl Into<String>) {
        if let Some(settings) = &mut self.admin.settings {
            settings.message = Some(text.into());
        }
    }

    pub fn apply_manage(&mut self, action: ManageAction) {
        match action {
            ManageAction::PickImage(purpose) => {
                self.manage.picking = Some(purpose);
                self.backend().send(Command::PickIcon);
            }
            ManageAction::RemoveGuildImage { banner } => {
                let Some(guild) = self.settings_guild() else { return };
                let key = if banner { "banner" } else { "icon" };
                self.manage_api(
                    Tag::Saved,
                    Method::PATCH,
                    format!("/guilds/{guild}"),
                    Some(json!({ key: Value::Null })),
                );
            }
            ManageAction::CreateRole => {
                let Some(guild) = self.settings_guild() else { return };
                self.manage_api(
                    Tag::RoleCreated,
                    Method::POST,
                    format!("/guilds/{guild}/roles"),
                    Some(json!({ "name": "novo cargo" })),
                );
            }
            ManageAction::DeleteRole(role) => {
                let Some(guild) = self.settings_guild() else { return };
                if let Some(settings) = &mut self.admin.settings {
                    settings.role = None;
                }
                self.manage_api(
                    Tag::Done,
                    Method::DELETE,
                    format!("/guilds/{guild}/roles/{role}"),
                    None,
                );
            }
            ManageAction::ToggleRolePermission(bit) => self.manage.role_permissions ^= bit,
            ManageAction::DeleteEmoji(emoji) => {
                let Some(guild) = self.settings_guild() else { return };
                self.manage_api(
                    Tag::Done,
                    Method::DELETE,
                    format!("/guilds/{guild}/emojis/{emoji}"),
                    None,
                );
            }
            ManageAction::DeleteSticker(sticker) => {
                let Some(guild) = self.settings_guild() else { return };
                self.manage_api(
                    Tag::Done,
                    Method::DELETE,
                    format!("/guilds/{guild}/stickers/{sticker}"),
                    None,
                );
            }
            ManageAction::OpenChannel(channel) => {
                let Some(c) = self.model.channel(channel).cloned() else {
                    return;
                };
                let Some(guild) = c.guild else { return };
                if self.settings_guild() != Some(guild) {
                    self.apply_admin(crate::admin::AdminAction::OpenSettings(guild));
                }
                self.manage.channel = Some(channel);
                self.manage.channel_name = c.name.clone();
                self.manage.channel_topic = c.topic.clone().unwrap_or_default();
                self.manage.slowmode = c.slowmode;
                self.manage.nsfw = c.nsfw;
                self.manage.overwrite = c.overwrites.iter().find(|o| o.id == guild).cloned();
                if let Some(settings) = &mut self.admin.settings {
                    settings.page = Some(GuildPage::Channel);
                    settings.message = None;
                }
            }
            ManageAction::SaveChannel => {
                let Some(channel) = self.manage.channel else { return };
                let voice = self.model.channel(channel).is_some_and(|c| c.kind.is_voice());
                let mut body = json!({
                    "name": self.manage.channel_name.trim(),
                    "nsfw": self.manage.nsfw,
                });
                if !voice {
                    body["topic"] = json!(self.manage.channel_topic.trim());
                    body["rate_limit_per_user"] = json!(self.manage.slowmode);
                }
                self.manage_api(Tag::Saved, Method::PATCH, format!("/channels/{channel}"), Some(body));
            }
            ManageAction::DeleteChannel => {
                let Some(channel) = self.manage.channel.take() else { return };
                if let Some(settings) = &mut self.admin.settings {
                    settings.page = Some(GuildPage::Overview);
                }
                self.manage_api(Tag::Done, Method::DELETE, format!("/channels/{channel}"), None);
            }
            ManageAction::SelectOverwrite { id, kind } => {
                let existing = self
                    .manage
                    .channel
                    .and_then(|c| self.model.channel(c))
                    .and_then(|c| c.overwrites.iter().find(|o| o.id == id).cloned());
                self.manage.overwrite = Some(existing.unwrap_or(Overwrite {
                    id,
                    kind,
                    allow: 0,
                    deny: 0,
                }));
            }
            ManageAction::SetOverwrite { bit, state } => {
                if let Some(o) = &mut self.manage.overwrite {
                    o.allow &= !bit;
                    o.deny &= !bit;
                    match state {
                        1 => o.allow |= bit,
                        -1 => o.deny |= bit,
                        _ => {}
                    }
                }
            }
            ManageAction::SaveOverwrite => {
                let (Some(channel), Some(o)) = (self.manage.channel, self.manage.overwrite.clone())
                else {
                    return;
                };
                // Shown at once; CHANNEL_UPDATE confirms it.
                if let Some(c) = self.model.channels.get_mut(&channel) {
                    c.overwrites.retain(|x| x.id != o.id);
                    c.overwrites.push(o.clone());
                }
                self.manage_api(
                    Tag::Saved,
                    Method::PUT,
                    format!("/channels/{channel}/permissions/{}", o.id),
                    Some(json!({
                        "type": o.kind,
                        "allow": o.allow.to_string(),
                        "deny": o.deny.to_string(),
                    })),
                );
            }
            ManageAction::RemoveOverwrite => {
                let (Some(channel), Some(o)) = (self.manage.channel, self.manage.overwrite.take())
                else {
                    return;
                };
                if let Some(c) = self.model.channels.get_mut(&channel) {
                    c.overwrites.retain(|x| x.id != o.id);
                }
                self.manage_api(
                    Tag::Done,
                    Method::DELETE,
                    format!("/channels/{channel}/permissions/{}", o.id),
                    None,
                );
            }
            ManageAction::LoadAutoMod => {
                let Some(guild) = self.settings_guild() else { return };
                self.manage.automod = None;
                self.manage_api(
                    Tag::AutoMod { guild },
                    Method::GET,
                    format!("/guilds/{guild}/auto-moderation/rules"),
                    None,
                );
            }
            ManageAction::ToggleRule(rule, enabled) => {
                let Some(guild) = self.settings_guild() else { return };
                self.manage_api(
                    Tag::AutoModChanged,
                    Method::PATCH,
                    format!("/guilds/{guild}/auto-moderation/rules/{rule}"),
                    Some(json!({ "enabled": enabled })),
                );
            }
            ManageAction::DeleteRule(rule) => {
                let Some(guild) = self.settings_guild() else { return };
                self.manage_api(
                    Tag::AutoModChanged,
                    Method::DELETE,
                    format!("/guilds/{guild}/auto-moderation/rules/{rule}"),
                    None,
                );
            }
            ManageAction::CreateRule => {
                let Some(guild) = self.settings_guild() else { return };
                let words: Vec<String> = self
                    .manage
                    .rule_words
                    .split([',', '\n'])
                    .map(|w| w.trim().to_owned())
                    .filter(|w| !w.is_empty())
                    .collect();
                if words.is_empty() {
                    self.page_message("Liste as palavras a bloquear, separadas por vírgula.");
                    return;
                }
                let name = match self.manage.rule_name.trim() {
                    "" => "Palavras bloqueadas".to_owned(),
                    name => name.to_owned(),
                };
                self.manage_api(
                    Tag::AutoModChanged,
                    Method::POST,
                    format!("/guilds/{guild}/auto-moderation/rules"),
                    Some(json!({
                        "name": name,
                        "event_type": 1,
                        "trigger_type": 1,
                        "trigger_metadata": { "keyword_filter": words },
                        "actions": [{ "type": 1, "metadata": {} }],
                        "enabled": true,
                    })),
                );
                self.manage.rule_name.clear();
                self.manage.rule_words.clear();
            }
            ManageAction::Notify(change) => self.change_notify(change),
            ManageAction::RemoveAvatar => self.manage_api(
                Tag::Saved,
                Method::PATCH,
                "/users/@me".into(),
                Some(json!({ "avatar": Value::Null })),
            ),
            ManageAction::SetAccent(color) => self.manage_api(
                Tag::Saved,
                Method::PATCH,
                "/users/@me/profile".into(),
                Some(json!({ "accent_color": color })),
            ),
            ManageAction::Timeout {
                guild,
                user,
                minutes,
            } => {
                let until = (minutes > 0).then(|| {
                    (jiff::Timestamp::now() + jiff::SignedDuration::from_mins(i64::from(minutes)))
                        .to_string()
                });
                self.admin.modal = None;
                self.manage_api(
                    Tag::Done,
                    Method::PATCH,
                    format!("/guilds/{guild}/members/{user}"),
                    Some(json!({ "communication_disabled_until": until })),
                );
            }
        }
    }

    /// Shown at once, then sent as Discord's user guild settings.
    fn change_notify(&mut self, change: NotifyChange) {
        let mute_body = |mute: Option<u32>| {
            let window = mute.map_or(-1, |m| i64::from(m) * 60);
            let end = mute.map(|m| {
                (jiff::Timestamp::now() + jiff::SignedDuration::from_mins(i64::from(m))).to_string()
            });
            json!({ "selected_time_window": window, "end_time": end })
        };
        let notify = self.model.notify.entry(change.guild).or_insert_with(|| {
            crate::model::Notify {
                level: 3,
                ..Default::default()
            }
        });
        let mut body = json!({});
        match change.channel {
            Some(channel) => {
                let entry = notify.channels.entry(channel).or_default();
                let mut over = json!({});
                if let Some(level) = change.level {
                    entry.level = level;
                    over["message_notifications"] = json!(level);
                }
                if let Some(mute) = change.mute {
                    entry.muted = mute.is_some();
                    over["muted"] = json!(mute.is_some());
                    if let Some(m) = mute {
                        over["mute_config"] = mute_body(m);
                    }
                }
                body["channel_overrides"] = json!({ channel.to_string(): over });
            }
            None => {
                if let Some(level) = change.level {
                    notify.level = level;
                    body["message_notifications"] = json!(level);
                }
                if let Some(mute) = change.mute {
                    notify.muted = mute.is_some();
                    body["muted"] = json!(mute.is_some());
                    if let Some(m) = mute {
                        body["mute_config"] = mute_body(m);
                    }
                }
                if let Some(on) = change.suppress_everyone {
                    notify.suppress_everyone = on;
                    body["suppress_everyone"] = json!(on);
                }
                if let Some(on) = change.suppress_roles {
                    notify.suppress_roles = on;
                    body["suppress_roles"] = json!(on);
                }
            }
        }
        let place = change.guild.map_or_else(|| "@me".to_owned(), |g| g.to_string());
        self.manage_api(
            Tag::Done,
            Method::PATCH,
            format!("/users/@me/guilds/{place}/settings"),
            Some(body),
        );
    }

    /// A picked picture, by what it was picked for. Emoji and server
    /// pictures go as data URIs; a sticker goes as a form with its file.
    pub fn picture_picked(&mut self, path: &std::path::Path) {
        let purpose = self.manage.picking.take().unwrap_or(Picking::NewGuild);
        let uri = || data_uri(path);
        let guild = self.settings_guild();
        match purpose {
            Picking::NewGuild => self.icon_picked(path),
            Picking::GuildIcon | Picking::GuildBanner => {
                let (Some(guild), Some(uri)) = (guild, uri()) else { return };
                let key = if purpose == Picking::GuildIcon { "icon" } else { "banner" };
                self.manage_api(
                    Tag::Saved,
                    Method::PATCH,
                    format!("/guilds/{guild}"),
                    Some(json!({ key: uri })),
                );
            }
            Picking::Emoji => {
                let (Some(guild), Some(uri)) = (guild, uri()) else { return };
                let name: String = match self.manage.emoji_name.trim() {
                    "" => path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "emoji".into()),
                    name => name.to_owned(),
                }
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .take(32)
                .collect();
                let name = if name.len() < 2 { "emoji".to_owned() } else { name };
                self.manage.emoji_name.clear();
                self.manage_api(
                    Tag::Saved,
                    Method::POST,
                    format!("/guilds/{guild}/emojis"),
                    Some(json!({ "name": name, "image": uri, "roles": [] })),
                );
            }
            Picking::Sticker => {
                let Some(guild) = guild else { return };
                let name = match self.manage.sticker_name.trim() {
                    "" => path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "figurinha".into()),
                    name => name.to_owned(),
                };
                let tag = match self.manage.sticker_tag.trim() {
                    "" => "🙂".to_owned(),
                    tag => tag.to_owned(),
                };
                self.manage.sticker_name.clear();
                self.manage.sticker_tag.clear();
                self.backend().send(Command::ApiForm {
                    tag: crate::backend::Tag::Manage(Tag::Saved),
                    path: format!("/guilds/{guild}/stickers"),
                    fields: vec![
                        ("name".into(), name),
                        ("tags".into(), tag),
                        ("description".into(), String::new()),
                    ],
                    file: path.to_owned(),
                });
            }
            Picking::Avatar | Picking::ProfileBanner => {
                let Some(uri) = uri() else { return };
                let key = if purpose == Picking::Avatar { "avatar" } else { "banner" };
                self.manage_api(
                    Tag::Saved,
                    Method::PATCH,
                    "/users/@me".into(),
                    Some(json!({ key: uri })),
                );
                if let Some(form) = &mut self.profile_form {
                    form.message = Some("Enviando…".into());
                }
            }
        }
    }

    pub fn manage_result(&mut self, tag: Tag, result: Result<Value, String>) {
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                if let Some(form) = &mut self.profile_form {
                    form.message = Some(error.clone());
                }
                if self.admin.settings.is_some() {
                    self.page_message(error);
                } else if self.profile_form.is_none() {
                    self.chat.dialog = Some(crate::chat::Dialog::Failed(error));
                }
                return;
            }
        };
        match tag {
            Tag::Done => {}
            Tag::Saved => {
                self.page_message("Alterações salvas.");
                if let Some(form) = &mut self.profile_form {
                    form.message = Some("Salvo.".into());
                    // The profile card shows the new picture or colour.
                    let me = self.model.me;
                    self.chat.profiles.remove(&me);
                }
                // Our own avatar or banner, shown at once.
                if value["discriminator"].is_string()
                    && let Some(user) = parse::user(&value)
                    && user.id == self.model.me
                {
                    let picture = user.picture.clone();
                    if let Some(me) = self.model.users.get_mut(&user.id) {
                        me.picture = picture;
                    }
                    self.chat.profiles.remove(&user.id);
                }
            }
            Tag::RoleCreated => {
                if let Some(role) = parse::id(&value["id"]) {
                    self.guild_role_event(&json!({
                        "guild_id": self.settings_guild().map(|g| g.to_string()),
                        "role": value,
                    }));
                    self.apply_admin(crate::admin::AdminAction::SelectRole(role));
                }
            }
            Tag::AutoMod { guild } => {
                if self.settings_guild() == Some(guild) {
                    self.manage.automod = value.as_array().cloned();
                }
            }
            Tag::AutoModChanged => {
                self.apply_manage(ManageAction::LoadAutoMod);
            }
        }
    }

    fn guild_role_event(&mut self, data: &Value) {
        let Some(guild) = parse::id(&data["guild_id"]).and_then(|g| self.model.guild_mut(g)) else {
            return;
        };
        if let Some(role) = parse::role(&data["role"]) {
            guild.roles.retain(|r| r.id != role.id);
            guild.roles.push(role);
            guild.roles.sort_by_key(|r| std::cmp::Reverse((r.position, r.id)));
        }
    }

    /// The gateway's changes to a server: its details, roles, emoji,
    /// stickers and the user's own membership.
    pub fn guild_dispatch(&mut self, kind: &str, data: &Value) {
        match kind {
            "GUILD_UPDATE" => {
                let Some(id) = parse::id(&data["id"]) else { return };
                let mut scratch = crate::model::Model::default();
                let Some(fresh) = parse::guild(data, &mut scratch) else {
                    return;
                };
                if let Some(guild) = self.model.guild_mut(id) {
                    guild.name = fresh.name;
                    guild.icon = fresh.icon;
                    guild.banner = fresh.banner;
                    guild.owner = fresh.owner;
                    if !fresh.roles.is_empty() {
                        guild.roles = fresh.roles;
                    }
                }
            }
            "GUILD_ROLE_CREATE" | "GUILD_ROLE_UPDATE" => self.guild_role_event(data),
            "GUILD_ROLE_DELETE" => {
                let role = parse::id(&data["role_id"]);
                if let Some(guild) = parse::id(&data["guild_id"]).and_then(|g| self.model.guild_mut(g)) {
                    guild.roles.retain(|r| Some(r.id) != role);
                }
            }
            "GUILD_EMOJIS_UPDATE" => {
                let emojis = parse::emojis(&data["emojis"]);
                if let Some(guild) = parse::id(&data["guild_id"]).and_then(|g| self.model.guild_mut(g)) {
                    guild.emojis = emojis;
                }
            }
            "GUILD_STICKERS_UPDATE" => {
                let stickers: Vec<_> = data["stickers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(parse::sticker)
                    .collect();
                if let Some(guild) = parse::id(&data["guild_id"]).and_then(|g| self.model.guild_mut(g)) {
                    guild.stickers = stickers;
                }
            }
            "GUILD_MEMBER_UPDATE" => {
                let Some(user) = parse::id(&data["user"]["id"]) else { return };
                let member = parse::member(data);
                if let Some(guild) = parse::id(&data["guild_id"]).and_then(|g| self.model.guild_mut(g)) {
                    guild.members.insert(user, member);
                }
            }
            _ => {}
        }
    }
}

/// A picture file as the data URI Discord takes.
pub fn data_uri(path: &std::path::Path) -> Option<String> {
    use base64::Engine;
    let bytes = std::fs::read(path).ok()?;
    let kind = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "jpeg",
        Some("gif") => "gif",
        Some("webp") => "webp",
        _ => "png",
    };
    Some(format!(
        "data:image/{kind};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}
