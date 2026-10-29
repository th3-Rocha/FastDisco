//! The server menu and the server settings: invites, nickname, new
//! channels and categories, leaving; and for those allowed, the overview,
//! roles, emoji, members, invites, bans, audit log and deleting the server.
//! Requests go through the backend's generic API call, tagged so their
//! answers find their way back here.

use std::collections::HashMap;

use reqwest::Method;
use serde_json::{Value, json};

use crate::app::App;
use crate::backend::Command;
use crate::model::Id;

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    Invite,
    /// What an invite in a message leads to, for its card.
    InvitePreview {
        code: String,
    },
    Nick {
        guild: Id,
        nick: String,
    },
    VoiceStatus {
        channel: Id,
        status: String,
    },
    /// The user's status or custom status saved; nothing more to do.
    Presence,
    /// A new server, opened once the gateway brings it.
    GuildCreated,
    /// The devices signed in to the account.
    Sessions,
    /// An account setting saved; nothing more to do.
    Saved,
    /// What Discord may do with the account's data.
    Consents,
    /// Which e-mails Discord sends.
    EmailSettings,
    /// The copy of the account's data, asked for.
    DataRequested,
    /// A device signed out.
    SessionOut {
        hash: String,
    },
    /// The account disabled or deleted: signing out follows.
    AccountGone,
    /// Joined a server from an invite.
    Joined {
        code: String,
    },
    Created,
    Left {
        guild: Id,
    },
    GuildSaved,
    RoleSaved,
    Invites {
        guild: Id,
    },
    Revoked {
        code: String,
    },
    Bans {
        guild: Id,
    },
    Unbanned {
        user: Id,
    },
    Removed {
        user: Id,
    },
    Audit {
        guild: Id,
    },
    Deleted {
        guild: Id,
    },
    /// A server mute, deafen, disconnect or move; the gateway shows it.
    VoiceMember,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuildPage {
    Overview,
    Roles,
    Emoji,
    Members,
    Invites,
    Bans,
    Audit,
    Stickers,
    AutoMod,
    /// The channel in `Manage::channel`.
    Channel,
}

/// A dialog over the window.
#[derive(Clone, Debug)]
pub enum Modal {
    Invite {
        link: Option<String>,
    },
    Nick {
        guild: Id,
        text: String,
    },
    /// The line under a voice channel's name.
    VoiceStatus {
        channel: Id,
        text: String,
    },
    /// "Criar seu servidor", step by step as Discord's.
    CreateGuild {
        step: CreateStep,
        /// Index into `TEMPLATES`.
        template: usize,
        name: String,
        /// The icon as a data URI, once picked.
        icon: Option<String>,
    },
    /// The devices signed in to the account, each with "Sair".
    Sessions,
    /// Disabling ("Desativar conta") or deleting ("Excluir conta") the
    /// account; Discord asks for the password.
    CloseAccount {
        delete: bool,
        password: String,
    },
    /// "Entrar em um servidor" with an invite.
    JoinGuild {
        code: String,
    },
    /// "No que você está pensando?": the user's custom status.
    CustomStatus {
        text: String,
    },
    CreateChannel {
        guild: Id,
        voice: bool,
        name: String,
        category: Option<Id>,
    },
    CreateCategory {
        guild: Id,
        name: String,
    },
    Leave {
        guild: Id,
    },
    Kick {
        guild: Id,
        user: Id,
    },
    Ban {
        guild: Id,
        user: Id,
        /// How much of their recent history goes too, in seconds.
        history: u64,
    },
    /// "Castigo": how long they cannot talk.
    Timeout {
        guild: Id,
        user: Id,
        minutes: u32,
    },
    DeleteGuild {
        guild: Id,
        typed: String,
    },
    /// "Iniciar palco": the stage's topic.
    StartStage {
        channel: Id,
        topic: String,
    },
}

#[derive(Default)]
pub struct GuildSettings {
    pub guild: Id,
    pub page: Option<GuildPage>,
    pub name: String,
    pub role: Option<Id>,
    pub role_name: String,
    pub role_color: [u8; 3],
    pub invites: Option<Vec<Value>>,
    pub bans: Option<Vec<Value>>,
    pub audit: Option<Value>,
    pub message: Option<String>,
}

/// Where "Criar seu servidor" is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateStep {
    /// Make your own, or start from a template.
    Start,
    /// For friends or for a community.
    Audience,
    /// The name and the icon.
    Customize,
}

/// Discord's server templates: the label, its emoji, and the categories
/// with their text and voice channels.
pub struct Template {
    pub label: &'static str,
    pub emoji: &'static str,
    pub categories: &'static [(&'static str, &'static [&'static str], &'static [&'static str])],
}

pub const TEMPLATES: &[Template] = &[
    Template {
        label: "Criar o meu",
        emoji: "🧑‍💻",
        categories: &[("Canais de texto", &["geral"], &[]), ("Canais de voz", &[], &["Geral"])],
    },
    Template {
        label: "Jogos",
        emoji: "🎮",
        categories: &[
            ("Canais de texto", &["geral", "clipes-e-destaques"], &[]),
            ("Canais de voz", &[], &["Saguão", "Jogando"]),
        ],
    },
    Template {
        label: "Amigos",
        emoji: "💗",
        categories: &[
            ("Canais de texto", &["geral", "jogos", "música"], &[]),
            ("Canais de voz", &[], &["Saguão", "Sala de transmissão"]),
        ],
    },
    Template {
        label: "Grupo de estudos",
        emoji: "🍎",
        categories: &[
            ("Informações", &["boas-vindas-e-regras", "notas-e-recursos"], &[]),
            (
                "Canais de texto",
                &["geral", "ajuda-com-tarefas", "planejamento-de-sessões", "fora-do-tópico"],
                &[],
            ),
            ("Canais de voz", &[], &["Saguão", "Sala de estudos 1", "Sala de estudos 2"]),
        ],
    },
    Template {
        label: "Clube escolar",
        emoji: "📘",
        categories: &[
            ("Informações", &["boas-vindas-e-regras", "anúncios", "recursos"], &[]),
            ("Canais de texto", &["geral", "planejamento-de-reuniões", "fora-do-tópico"], &[]),
            ("Canais de voz", &[], &["Saguão", "Sala de reuniões"]),
        ],
    },
    Template {
        label: "Comunidade local",
        emoji: "🌳",
        categories: &[
            ("Informações", &["boas-vindas-e-regras", "anúncios", "recursos"], &[]),
            ("Canais de texto", &["geral", "planejamento-de-encontros", "fora-do-tópico"], &[]),
            ("Canais de voz", &[], &["Saguão", "Sala de reuniões"]),
        ],
    },
    Template {
        label: "Artistas e criadores",
        emoji: "🎨",
        categories: &[
            ("Informações", &["boas-vindas-e-regras", "anúncios"], &[]),
            ("Canais de texto", &["geral", "eventos", "ideias-e-feedback"], &[]),
            ("Canais de voz", &[], &["Saguão", "Comunidade", "Sala de transmissão"]),
        ],
    },
];

/// The channels of a new server from a template, with placeholder ids
/// tying channels to their categories, as Discord's create call takes.
fn template_channels(template: &Template) -> Value {
    let mut list = Vec::new();
    let mut next = 1u64;
    for (category, text, voice) in template.categories {
        let parent = next;
        next += 1;
        list.push(json!({ "id": parent.to_string(), "name": category, "type": 4 }));
        for name in *text {
            list.push(json!({ "id": next.to_string(), "name": name, "type": 0, "parent_id": parent.to_string() }));
            next += 1;
        }
        for name in *voice {
            list.push(json!({ "id": next.to_string(), "name": name, "type": 2, "parent_id": parent.to_string() }));
            next += 1;
        }
    }
    Value::Array(list)
}

/// The code in an invite link: "https://discord.gg/abc" and "abc" are
/// both "abc".
pub fn invite_code(text: &str) -> String {
    let text = text.trim().trim_end_matches('/');
    text.rsplit('/').next().unwrap_or(text).to_owned()
}

/// An invite link's card in a message: being looked up, gone, or what
/// it leads to.
#[derive(Clone, Debug)]
pub enum Preview {
    Loading,
    Failed,
    Ready(InviteInfo),
}

/// What Discord says an invite leads to.
#[derive(Clone, Debug, Default)]
pub struct InviteInfo {
    pub guild: Option<Id>,
    pub guild_name: String,
    pub guild_icon: Option<String>,
    pub channel: Option<Id>,
    /// A voice or stage channel: the card is the call's.
    pub voice: bool,
    pub members: Option<u64>,
    pub online: Option<u64>,
}

impl InviteInfo {
    fn parse(value: &Value) -> Self {
        let guild = crate::discord::parse::id(&value["guild"]["id"]);
        Self {
            guild,
            guild_name: value["guild"]["name"].as_str().unwrap_or_default().to_owned(),
            guild_icon: guild.zip(value["guild"]["icon"].as_str()).map(|(id, hash)| {
                format!("{}/icons/{id}/{hash}.png?size=96", crate::discord::CDN)
            }),
            channel: crate::discord::parse::id(&value["channel"]["id"]),
            voice: matches!(value["channel"]["type"].as_u64(), Some(2 | 13)),
            members: value["approximate_member_count"].as_u64(),
            online: value["approximate_presence_count"].as_u64(),
        }
    }
}

#[derive(Default)]
pub struct Admin {
    /// Invite cards in messages, by code.
    pub previews: HashMap<String, Preview>,
    pub menu: bool,
    pub modal: Option<Modal>,
    pub busy: bool,
    pub error: Option<String>,
    pub settings: Option<GuildSettings>,
    /// The channel the open invite leads to.
    pub invite_channel: Option<Id>,
    /// A server just created or joined, to open when it arrives.
    pub open_when_ready: Option<Id>,
    /// The devices signed in, once asked for.
    pub sessions: Option<Vec<Session>>,
    pub sessions_asked: bool,
    /// "usage_statistics" and "personalization", once asked for.
    pub consents: Option<serde_json::Map<String, Value>>,
    /// The e-mail categories and whether each is on, once asked for.
    pub emails: Option<serde_json::Map<String, Value>>,
    pub privacy_asked: bool,
    /// What came of asking for a copy of the data.
    pub data_request: Option<String>,
}

/// A device signed in to the account.
#[derive(Clone, Debug)]
pub struct Session {
    /// What signing it out names it by.
    pub hash: String,
    /// "Linux · Discord Client", as Discord lists it.
    pub name: String,
    pub location: String,
    /// "2026-10-28T20:58:23Z".
    pub last_used: String,
}

#[derive(Clone, Debug)]
pub enum AdminAction {
    /// Looks up an invite seen in a message, once.
    ResolveInvite(String),
    /// "Entrar" on an invite card.
    AcceptInvite(String),
    ToggleMenu,
    CloseMenu,
    Open(Modal),
    /// An invite to a voice channel, from its row.
    InviteTo(Id),
    /// Choosing the new server's icon.
    PickIcon,
    /// The signed-in devices, for the account page.
    LoadSessions,
    /// Changes account settings, as `{"key": value}`.
    SetUserSettings(Value),
    /// The data consents and e-mail settings, for the privacy and
    /// notification pages.
    LoadPrivacy,
    /// Grants or revokes a data consent.
    SetConsent(String, bool),
    /// Turns an e-mail category on or off.
    SetEmail(String, bool),
    /// Asks Discord for a copy of the account's data.
    RequestData,
    /// Signs one device out.
    SignOutSession(String),
    /// Online, idle, do not disturb or invisible, from the profile card.
    SetStatus(crate::model::Status),
    /// The open dialog's main button.
    Submit,
    CloseModal,
    OpenSettings(Id),
    CloseSettings,
    Page(GuildPage),
    SaveGuild,
    SelectRole(Id),
    SaveRole,
    Revoke(String),
    Unban(Id),
    /// Raises or lowers the user's hand in a stage's audience.
    StageHand(bool),
    /// Brings someone (the user when none) up to a stage or down to the
    /// audience.
    StageSpeaker {
        user: Option<Id>,
        speak: bool,
    },
    /// Ends the stage for everyone.
    EndStage(Id),
    /// Changes someone's voice state in the server: `mute`, `deaf` or
    /// `channel_id` (null disconnects).
    VoiceMember {
        guild: Id,
        user: Id,
        change: Value,
    },
}

impl App {
    fn api(&self, tag: Tag, method: Method, path: String, body: Option<Value>) {
        self.backend().send(Command::Api {
            tag: crate::backend::Tag::Admin(tag),
            method,
            path,
            body,
        });
    }

    pub fn apply_admin(&mut self, action: AdminAction) {
        match action {
            AdminAction::ToggleMenu => self.admin.menu = !self.admin.menu,
            AdminAction::VoiceMember {
                guild,
                user,
                change,
            } => self.api(
                Tag::VoiceMember,
                Method::PATCH,
                format!("/guilds/{guild}/members/{user}"),
                Some(change),
            ),
            AdminAction::CloseMenu => self.admin.menu = false,
            AdminAction::StageHand(raise) => {
                let Some((guild, channel)) = self.call.as_ref().and_then(|c| Some((c.guild?, c.channel))) else {
                    return;
                };
                let when = raise.then(|| jiff::Timestamp::now().to_string());
                self.api(
                    Tag::VoiceMember,
                    Method::PATCH,
                    format!("/guilds/{guild}/voice-states/@me"),
                    Some(json!({ "channel_id": channel.to_string(), "request_to_speak_timestamp": when })),
                );
            }
            AdminAction::StageSpeaker { user, speak } => {
                let Some((guild, channel)) = self.call.as_ref().and_then(|c| Some((c.guild?, c.channel))) else {
                    return;
                };
                let who = user.map_or("@me".to_owned(), |u| u.to_string());
                let mut body = json!({ "channel_id": channel.to_string(), "suppress": !speak });
                if user.is_none() {
                    body["request_to_speak_timestamp"] = Value::Null;
                }
                self.api(
                    Tag::VoiceMember,
                    Method::PATCH,
                    format!("/guilds/{guild}/voice-states/{who}"),
                    Some(body),
                );
            }
            AdminAction::EndStage(channel) => self.api(
                Tag::VoiceMember,
                Method::DELETE,
                format!("/stage-instances/{channel}"),
                None,
            ),
            AdminAction::ResolveInvite(code) => {
                if self.admin.previews.contains_key(&code) {
                    return;
                }
                self.admin.previews.insert(code.clone(), Preview::Loading);
                self.api(
                    Tag::InvitePreview { code: code.clone() },
                    Method::GET,
                    format!("/invites/{code}?with_counts=true&with_expiration=true"),
                    None,
                );
            }
            AdminAction::AcceptInvite(code) => self.api(
                Tag::Joined { code: code.clone() },
                Method::POST,
                format!("/invites/{code}"),
                Some(json!({})),
            ),
            AdminAction::Open(modal) => {
                self.admin.menu = false;
                self.admin.error = None;
                self.admin.busy = false;
                if let Modal::Invite { .. } = modal {
                    self.create_invite(self.channel);
                }
                self.admin.modal = Some(modal);
            }
            AdminAction::PickIcon => self.apply_manage(crate::manage::ManageAction::PickImage(
                crate::manage::Picking::NewGuild,
            )),
            AdminAction::SetUserSettings(patch) => {
                if let (Some(store), Some(changes)) =
                    (self.model.account.settings.as_object_mut(), patch.as_object())
                {
                    for (key, value) in changes {
                        store.insert(key.clone(), value.clone());
                    }
                }
                self.api(Tag::Saved, Method::PATCH, "/users/@me/settings".into(), Some(patch));
            }
            AdminAction::LoadPrivacy => {
                self.admin.privacy_asked = true;
                self.api(Tag::Consents, Method::GET, "/users/@me/consent".into(), None);
                self.api(Tag::EmailSettings, Method::GET, "/users/@me/email-settings".into(), None);
            }
            AdminAction::SetConsent(kind, on) => {
                if let Some(consents) = &mut self.admin.consents {
                    consents.insert(kind.clone(), json!({ "consented": on }));
                }
                let body = if on {
                    json!({ "grant": [kind], "revoke": [] })
                } else {
                    json!({ "grant": [], "revoke": [kind] })
                };
                self.api(Tag::Saved, Method::POST, "/users/@me/consent".into(), Some(body));
            }
            AdminAction::SetEmail(category, on) => {
                let Some(emails) = &mut self.admin.emails else {
                    return;
                };
                emails.insert(category, json!(on));
                let body = json!({ "settings": { "categories": emails.clone() } });
                self.api(Tag::Saved, Method::PUT, "/users/@me/email-settings".into(), Some(body));
            }
            AdminAction::RequestData => {
                self.admin.data_request = Some("Pedindo…".into());
                self.api(Tag::DataRequested, Method::POST, "/users/@me/harvest".into(), Some(json!({})));
            }
            AdminAction::LoadSessions => {
                self.admin.sessions_asked = true;
                self.api(Tag::Sessions, Method::GET, "/auth/sessions".into(), None);
            }
            AdminAction::SignOutSession(hash) => {
                self.api(
                    Tag::SessionOut { hash: hash.clone() },
                    Method::POST,
                    "/auth/sessions/logout".into(),
                    Some(json!({ "session_id_hashes": [hash] })),
                );
            }
            AdminAction::SetStatus(status) => {
                let me = self.model.me;
                if let Some(user) = self.model.users.get_mut(&me) {
                    user.status = status;
                }
                self.send_presence();
                self.api(
                    Tag::Presence,
                    Method::PATCH,
                    "/users/@me/settings".into(),
                    Some(json!({ "status": status.name() })),
                );
            }
            AdminAction::InviteTo(channel) => {
                self.admin.menu = false;
                self.admin.error = None;
                self.create_invite(Some(channel));
                self.admin.modal = Some(Modal::Invite { link: None });
            }
            AdminAction::CloseModal => {
                self.admin.modal = None;
                self.admin.busy = false;
                self.admin.error = None;
            }
            AdminAction::Submit => self.submit_modal(),
            AdminAction::OpenSettings(guild) => {
                self.admin.menu = false;
                let name = self
                    .model
                    .guild(guild)
                    .map(|g| g.name.clone())
                    .unwrap_or_default();
                self.admin.settings = Some(GuildSettings {
                    guild,
                    page: Some(GuildPage::Overview),
                    name,
                    ..Default::default()
                });
            }
            AdminAction::CloseSettings => self.admin.settings = None,
            AdminAction::Page(page) => {
                let Some(settings) = &mut self.admin.settings else {
                    return;
                };
                settings.page = Some(page);
                settings.message = None;
                let guild = settings.guild;
                match page {
                    GuildPage::Invites => self.api(
                        Tag::Invites { guild },
                        Method::GET,
                        format!("/guilds/{guild}/invites"),
                        None,
                    ),
                    GuildPage::Bans => self.api(
                        Tag::Bans { guild },
                        Method::GET,
                        format!("/guilds/{guild}/bans?limit=1000"),
                        None,
                    ),
                    GuildPage::AutoMod => {
                        self.apply_manage(crate::manage::ManageAction::LoadAutoMod)
                    }
                    GuildPage::Audit => self.api(
                        Tag::Audit { guild },
                        Method::GET,
                        format!("/guilds/{guild}/audit-logs?limit=50"),
                        None,
                    ),
                    _ => {}
                }
            }
            AdminAction::SaveGuild => {
                let Some(settings) = &self.admin.settings else {
                    return;
                };
                let (guild, name) = (settings.guild, settings.name.trim().to_owned());
                if !name.is_empty() {
                    self.api(
                        Tag::GuildSaved,
                        Method::PATCH,
                        format!("/guilds/{guild}"),
                        Some(json!({ "name": name })),
                    );
                }
            }
            AdminAction::SelectRole(role) => {
                let Some(settings) = &mut self.admin.settings else {
                    return;
                };
                let found = self
                    .model
                    .guild(settings.guild)
                    .and_then(|g| g.roles.iter().find(|r| r.id == role))
                    .cloned();
                if let Some(r) = found {
                    settings.role = Some(role);
                    settings.role_name = r.name.clone();
                    settings.role_color = r
                        .color
                        .map_or([0x99, 0xaa, 0xb5], |c| [c.r(), c.g(), c.b()]);
                    self.manage.role_permissions = r.permissions;
                    settings.message = None;
                }
            }
            AdminAction::SaveRole => {
                let Some(settings) = &self.admin.settings else {
                    return;
                };
                let Some(role) = settings.role else { return };
                let [r, g, b] = settings.role_color;
                let color = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
                let body = json!({
                    "name": settings.role_name.trim(),
                    "color": color,
                    "permissions": self.manage.role_permissions.to_string(),
                });
                self.api(
                    Tag::RoleSaved,
                    Method::PATCH,
                    format!("/guilds/{}/roles/{role}", settings.guild),
                    Some(body),
                );
            }
            AdminAction::Revoke(code) => {
                self.api(
                    Tag::Revoked { code: code.clone() },
                    Method::DELETE,
                    format!("/invites/{code}"),
                    None,
                );
            }
            AdminAction::Unban(user) => {
                let Some(settings) = &self.admin.settings else {
                    return;
                };
                self.api(
                    Tag::Unbanned { user },
                    Method::DELETE,
                    format!("/guilds/{}/bans/{user}", settings.guild),
                    None,
                );
            }
        }
    }

    /// The new server's icon, as the data URI Discord takes.
    pub fn icon_picked(&mut self, path: &std::path::Path) {
        let Some(uri) = crate::manage::data_uri(path) else {
            return;
        };
        if let Some(Modal::CreateGuild { icon, .. }) = &mut self.admin.modal {
            *icon = Some(uri);
        }
    }

    /// Opens a server now if it is here, else once GUILD_CREATE brings it.
    fn open_guild_when_ready(&mut self, id: Id) {
        if self.model.guild(id).is_some() {
            self.actions.push(crate::app::Action::OpenServer(id));
        } else {
            self.admin.open_when_ready = Some(id);
        }
    }

    /// The user's status and custom status, to the gateway.
    fn send_presence(&self) {
        let me = self.model.me;
        let Some(user) = self.model.users.get(&me) else {
            return;
        };
        let custom = user
            .activities
            .iter()
            .find(|a| a.kind == crate::model::ActivityKind::Custom)
            .and_then(|a| a.state.clone());
        self.backend().send(Command::Presence {
            status: user.status.name().to_owned(),
            custom,
        });
    }

    /// A week-long invite to a channel, as Discord's default.
    fn create_invite(&mut self, channel: Option<Id>) {
        let Some(channel) = channel else { return };
        self.admin.invite_channel = Some(channel);
        self.admin.busy = true;
        let body = json!({ "max_age": 604800, "max_uses": 0, "temporary": false });
        self.api(
            Tag::Invite,
            Method::POST,
            format!("/channels/{channel}/invites"),
            Some(body),
        );
    }

    fn submit_modal(&mut self) {
        let Some(modal) = self.admin.modal.clone() else {
            return;
        };
        self.admin.busy = true;
        self.admin.error = None;
        match modal {
            Modal::Invite { link } => {
                if let Some(link) = link {
                    self.ctx.copy_text(link);
                }
                self.admin.busy = false;
            }
            Modal::Nick { guild, text } => {
                let nick = text.trim().to_owned();
                self.api(
                    Tag::Nick {
                        guild,
                        nick: nick.clone(),
                    },
                    Method::PATCH,
                    format!("/guilds/{guild}/members/@me"),
                    Some(json!({ "nick": nick })),
                );
            }
            Modal::Sessions => {
                self.admin.busy = false;
                self.admin.modal = None;
            }
            Modal::CloseAccount { delete, password } => {
                let path = if delete { "/users/@me/delete" } else { "/users/@me/disable" };
                self.api(
                    Tag::AccountGone,
                    Method::POST,
                    path.into(),
                    Some(json!({ "password": password })),
                );
            }
            Modal::CreateGuild {
                template,
                name,
                icon,
                ..
            } => {
                let template = &TEMPLATES[template.min(TEMPLATES.len() - 1)];
                let mut body = json!({
                    "name": name.trim(),
                    "channels": template_channels(template),
                    "system_channel_id": "2",
                });
                if let Some(icon) = icon {
                    body["icon"] = json!(icon);
                }
                self.api(Tag::GuildCreated, Method::POST, "/guilds".into(), Some(body));
            }
            Modal::JoinGuild { code } => {
                let code = invite_code(&code);
                if code.is_empty() {
                    self.admin.busy = false;
                    self.admin.error = Some("Insira um link de convite ou código válido.".into());
                    return;
                }
                self.api(
                    Tag::Joined { code: code.clone() },
                    Method::POST,
                    format!("/invites/{code}"),
                    Some(json!({})),
                );
            }
            Modal::CustomStatus { text } => {
                let text = text.trim().to_owned();
                let me = self.model.me;
                if let Some(user) = self.model.users.get_mut(&me) {
                    user.activities.retain(|a| a.kind != crate::model::ActivityKind::Custom);
                    if !text.is_empty() {
                        user.activities.insert(
                            0,
                            crate::model::Activity {
                                kind: crate::model::ActivityKind::Custom,
                                name: "Custom Status".into(),
                                state: Some(text.clone()),
                                ..crate::model::Activity::default()
                            },
                        );
                    }
                    user.activity = (!text.is_empty()).then(|| text.clone());
                }
                self.send_presence();
                let custom = (!text.is_empty()).then(|| json!({ "text": text }));
                self.api(
                    Tag::Presence,
                    Method::PATCH,
                    "/users/@me/settings".into(),
                    Some(json!({ "custom_status": custom })),
                );
            }
            Modal::StartStage { channel, topic } => {
                let topic = topic.trim().to_owned();
                if topic.is_empty() {
                    self.admin.busy = false;
                    self.admin.error = Some("Dê um tema ao palco.".into());
                    return;
                }
                self.api(
                    Tag::Presence,
                    Method::POST,
                    "/stage-instances".into(),
                    Some(json!({ "channel_id": channel.to_string(), "topic": topic, "privacy_level": 2 })),
                );
            }
            Modal::VoiceStatus { channel, text } => {
                let status = text.trim().to_owned();
                self.api(
                    Tag::VoiceStatus {
                        channel,
                        status: status.clone(),
                    },
                    Method::PUT,
                    format!("/channels/{channel}/voice-status"),
                    Some(json!({ "status": status })),
                );
            }
            Modal::CreateChannel {
                guild,
                voice,
                name,
                category,
            } => {
                // Discord's text channel names are lowercase with dashes.
                let name = if voice {
                    name.trim().to_owned()
                } else {
                    name.trim().to_lowercase().replace(' ', "-")
                };
                let mut body = json!({ "name": name, "type": if voice { 2 } else { 0 } });
                if let Some(parent) = category {
                    body["parent_id"] = json!(parent.to_string());
                }
                self.api(
                    Tag::Created,
                    Method::POST,
                    format!("/guilds/{guild}/channels"),
                    Some(body),
                );
            }
            Modal::CreateCategory { guild, name } => {
                self.api(
                    Tag::Created,
                    Method::POST,
                    format!("/guilds/{guild}/channels"),
                    Some(json!({ "name": name.trim(), "type": 4 })),
                );
            }
            Modal::Leave { guild } => {
                self.api(
                    Tag::Left { guild },
                    Method::DELETE,
                    format!("/users/@me/guilds/{guild}"),
                    Some(json!({ "lurking": false })),
                );
            }
            Modal::Kick { guild, user } => {
                self.api(
                    Tag::Removed { user },
                    Method::DELETE,
                    format!("/guilds/{guild}/members/{user}"),
                    None,
                );
            }
            Modal::Ban {
                guild,
                user,
                history,
            } => {
                self.api(
                    Tag::Removed { user },
                    Method::PUT,
                    format!("/guilds/{guild}/bans/{user}"),
                    Some(json!({ "delete_message_seconds": history })),
                );
            }
            Modal::Timeout {
                guild,
                user,
                minutes,
            } => {
                self.admin.busy = false;
                self.apply_manage(crate::manage::ManageAction::Timeout {
                    guild,
                    user,
                    minutes,
                });
            }
            Modal::DeleteGuild { guild, typed } => {
                let name = self
                    .model
                    .guild(guild)
                    .map(|g| g.name.clone())
                    .unwrap_or_default();
                if typed.trim() != crate::text::hyphens(&name) && typed.trim() != name {
                    self.admin.busy = false;
                    self.admin.error = Some("O nome não confere.".into());
                    return;
                }
                self.api(
                    Tag::Deleted { guild },
                    Method::DELETE,
                    format!("/guilds/{guild}"),
                    None,
                );
            }
        }
    }

    /// An answer from the API for the server menu or settings.
    pub fn api_result(&mut self, tag: Tag, result: Result<Value, String>) {
        self.admin.busy = false;
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                if let Tag::InvitePreview { code } = &tag {
                    log::debug!("invite {code}: {error}");
                    self.admin.previews.insert(code.clone(), Preview::Failed);
                    return;
                }
                if let Tag::DataRequested = tag {
                    self.admin.data_request = Some(format!("Não foi possível pedir: {error}"));
                    return;
                }
                if let Tag::Saved | Tag::Consents | Tag::EmailSettings = tag {
                    log::warn!("account settings: {error}");
                    return;
                }
                // Discord asks for a captcha to join from outside its own
                // apps: the invite opens in the browser instead.
                if let Tag::Joined { code } = &tag
                    && error.to_lowercase().contains("captcha")
                {
                    self.admin.modal = None;
                    self.apply_chat(crate::chat::ChatAction::OpenUrl(format!(
                        "https://discord.gg/{code}"
                    )));
                    return;
                }
                if let Tag::VoiceMember = tag {
                    self.chat.dialog = Some(crate::chat::Dialog::Failed(error));
                } else if self.admin.modal.is_some() {
                    self.admin.error = Some(error);
                } else if let Some(settings) = &mut self.admin.settings {
                    settings.message = Some(error);
                }
                return;
            }
        };
        match tag {
            Tag::Invite => {
                let code = value["code"].as_str().unwrap_or_default();
                if let Some(Modal::Invite { link }) = &mut self.admin.modal {
                    *link = Some(format!("https://discord.gg/{code}"));
                }
            }
            Tag::Presence => self.admin.modal = None,
            Tag::Saved => {}
            Tag::Consents => {
                self.admin.consents = value.as_object().cloned();
            }
            Tag::EmailSettings => {
                self.admin.emails = value["categories"].as_object().cloned();
            }
            Tag::DataRequested => {
                self.admin.data_request = Some(
                    "Pedido feito. O Discord envia o link para o seu e-mail quando a cópia estiver pronta (pode levar até 30 dias).".into(),
                );
            }
            Tag::Sessions => {
                let list = value["user_sessions"].as_array().cloned().unwrap_or_default();
                self.admin.sessions = Some(
                    list.iter()
                        .map(|s| {
                            let info = &s["client_info"];
                            let part = |k: &str| info[k].as_str().unwrap_or_default().to_owned();
                            let name = [part("os"), part("platform")]
                                .into_iter()
                                .filter(|p| !p.is_empty())
                                .collect::<Vec<_>>()
                                .join(" · ");
                            Session {
                                hash: s["id_hash"].as_str().unwrap_or_default().to_owned(),
                                name: if name.is_empty() { "Dispositivo".into() } else { name },
                                location: part("location"),
                                last_used: s["approx_last_used_time"].as_str().unwrap_or_default().to_owned(),
                            }
                        })
                        .collect(),
                );
            }
            Tag::SessionOut { hash } => {
                if let Some(list) = &mut self.admin.sessions {
                    list.retain(|s| s.hash != hash);
                }
            }
            Tag::AccountGone => {
                self.admin.modal = None;
                self.actions.push(crate::app::Action::Logout);
            }
            Tag::GuildCreated => {
                self.admin.modal = None;
                if let Some(id) = crate::discord::parse::id(&value["id"]) {
                    self.open_guild_when_ready(id);
                }
            }
            Tag::Joined { .. } => {
                self.admin.modal = None;
                if let Some(id) = crate::discord::parse::id(&value["guild"]["id"])
                    .or_else(|| crate::discord::parse::id(&value["guild_id"]))
                {
                    self.open_guild_when_ready(id);
                }
            }
            Tag::VoiceStatus { channel, status } => {
                if let Some(c) = self.model.channels.get_mut(&channel) {
                    c.voice_status = (!status.is_empty()).then_some(status);
                }
                self.admin.modal = None;
            }
            Tag::Nick { guild, nick } => {
                let me = self.model.me;
                if let Some(member) = self
                    .model
                    .guild_mut(guild)
                    .and_then(|g| g.members.get_mut(&me))
                {
                    member.nick = (!nick.is_empty()).then_some(nick);
                }
                self.admin.modal = None;
            }
            Tag::Created | Tag::Removed { .. } => {
                self.admin.modal = None;
                if let Tag::Removed { user } = tag {
                    let guild = self
                        .admin
                        .settings
                        .as_ref()
                        .map(|s| s.guild)
                        .or(self.server());
                    if let Some(g) = guild.and_then(|g| self.model.guild_mut(g)) {
                        g.members.remove(&user);
                        g.list.retain(
                            |item| !matches!(item, crate::model::ListItem::Member(u) if *u == user),
                        );
                    }
                }
            }
            Tag::Left { guild } | Tag::Deleted { guild } => {
                self.admin.modal = None;
                self.admin.settings = None;
                self.model.guilds.retain(|g| g.id != guild);
                self.model.settle_folders();
                self.actions.push(crate::app::Action::GoHome);
            }
            Tag::GuildSaved | Tag::RoleSaved => {
                if let Some(settings) = &mut self.admin.settings {
                    settings.message = Some("Alterações salvas.".into());
                }
            }
            Tag::InvitePreview { code } => {
                self.admin
                    .previews
                    .insert(code, Preview::Ready(InviteInfo::parse(&value)));
            }
            Tag::Invites { guild } => {
                if let Some(settings) = self.admin.settings.as_mut().filter(|s| s.guild == guild) {
                    settings.invites = value.as_array().cloned();
                }
            }
            Tag::Revoked { code } => {
                if let Some(list) = self
                    .admin
                    .settings
                    .as_mut()
                    .and_then(|s| s.invites.as_mut())
                {
                    list.retain(|i| i["code"].as_str() != Some(&code));
                }
            }
            Tag::Bans { guild } => {
                if let Some(settings) = self.admin.settings.as_mut().filter(|s| s.guild == guild) {
                    settings.bans = value.as_array().cloned();
                }
            }
            Tag::Unbanned { user } => {
                if let Some(list) = self.admin.settings.as_mut().and_then(|s| s.bans.as_mut()) {
                    list.retain(|b| crate::discord::parse::id(&b["user"]["id"]) != Some(user));
                }
            }
            Tag::VoiceMember => {}
            Tag::Audit { guild } => {
                if let Some(settings) = self.admin.settings.as_mut().filter(|s| s.guild == guild) {
                    settings.audit = Some(value);
                }
            }
        }
    }
}

/// What an audit log entry did, in Discord's words.
pub fn audit_action(kind: i64) -> &'static str {
    match kind {
        1 => "atualizou o servidor",
        10 => "criou um canal",
        11 => "atualizou um canal",
        12 => "excluiu um canal",
        13..=15 => "mudou permissões de um canal",
        20 => "expulsou um membro",
        21 => "removeu membros inativos",
        22 => "baniu um membro",
        23 => "revogou um banimento",
        24 => "atualizou um membro",
        25 => "mudou os cargos de um membro",
        26 => "moveu um membro de canal de voz",
        27 => "desconectou um membro da voz",
        28 => "adicionou um bot",
        30 => "criou um cargo",
        31 => "atualizou um cargo",
        32 => "excluiu um cargo",
        40 => "criou um convite",
        41 => "atualizou um convite",
        42 => "revogou um convite",
        50..=52 => "mudou um webhook",
        60..=62 => "mudou um emoji",
        72 => "excluiu uma mensagem",
        73 => "excluiu mensagens em massa",
        74 => "fixou uma mensagem",
        75 => "desafixou uma mensagem",
        80..=82 => "mudou uma integração",
        90..=92 => "mudou uma figurinha",
        100..=102 => "mudou um evento",
        110..=112 => "mudou um tópico",
        140..=145 => "mudou o AutoMod",
        _ => "fez uma alteração",
    }
}
