//! The server menu and the server settings: invites, nickname, new
//! channels and categories, leaving; and for those allowed, the overview,
//! roles, emoji, members, invites, bans, audit log and deleting the server.
//! Requests go through the backend's generic API call, tagged so their
//! answers find their way back here.

use reqwest::Method;
use serde_json::{Value, json};

use crate::app::App;
use crate::backend::Command;
use crate::model::Id;

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    Invite,
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
    /// The featured servers for "Descubra".
    Discover,
    /// Joined a server from "Descubra".
    JoinedDiscoverable {
        guild: Id,
    },
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
    },
    DeleteGuild {
        guild: Id,
        typed: String,
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

#[derive(Default)]
pub struct Admin {
    pub menu: bool,
    pub modal: Option<Modal>,
    pub busy: bool,
    pub error: Option<String>,
    pub settings: Option<GuildSettings>,
    /// The channel the open invite leads to.
    pub invite_channel: Option<Id>,
    /// A server just created or joined, to open when it arrives.
    pub open_when_ready: Option<Id>,
    /// Discord's "Descubra" page, while open.
    pub discover: Option<Discover>,
}

/// The servers Discord features on its "Descubra" page.
#[derive(Default)]
pub struct Discover {
    pub guilds: Vec<DiscoverGuild>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DiscoverGuild {
    pub id: Id,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub splash: Option<String>,
    pub online: u64,
    pub members: u64,
    pub verified: bool,
}

fn discover_guild(value: &Value) -> Option<DiscoverGuild> {
    use crate::discord::CDN;
    let id = crate::discord::parse::id(&value["id"])?;
    let features = value["features"].as_array();
    let has = |f: &str| features.is_some_and(|l| l.iter().any(|x| x == f));
    Some(DiscoverGuild {
        id,
        name: value["name"].as_str().unwrap_or_default().to_owned(),
        description: value["description"].as_str().unwrap_or_default().to_owned(),
        icon: value["icon"]
            .as_str()
            .map(|h| format!("{CDN}/icons/{id}/{h}.png?size=96")),
        splash: value["discovery_splash"]
            .as_str()
            .map(|h| format!("{CDN}/discovery-splashes/{id}/{h}.jpg?size=600"))
            .or_else(|| {
                value["splash"]
                    .as_str()
                    .map(|h| format!("{CDN}/splashes/{id}/{h}.jpg?size=600"))
            }),
        online: value["approximate_presence_count"].as_u64().unwrap_or(0),
        members: value["approximate_member_count"].as_u64().unwrap_or(0),
        verified: has("VERIFIED") || has("PARTNERED"),
    })
}

#[derive(Clone, Debug)]
pub enum AdminAction {
    ToggleMenu,
    CloseMenu,
    Open(Modal),
    /// An invite to a voice channel, from its row.
    InviteTo(Id),
    /// Choosing the new server's icon.
    PickIcon,
    /// Discord's "Descubra" page.
    OpenDiscover,
    /// Joining a server from "Descubra".
    JoinDiscoverable(Id),
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
            AdminAction::Open(modal) => {
                self.admin.menu = false;
                self.admin.error = None;
                self.admin.busy = false;
                if let Modal::Invite { .. } = modal {
                    self.create_invite(self.channel);
                }
                self.admin.modal = Some(modal);
            }
            AdminAction::PickIcon => self.backend().send(Command::PickIcon),
            AdminAction::OpenDiscover => {
                let fresh = self.admin.discover.is_none();
                let discover = self.admin.discover.get_or_insert_with(Discover::default);
                if fresh && discover.guilds.is_empty() {
                    discover.loading = true;
                    self.api(
                        Tag::Discover,
                        Method::GET,
                        "/discoverable-guilds?offset=0&limit=48".into(),
                        None,
                    );
                }
            }
            AdminAction::JoinDiscoverable(guild) => {
                if self.model.guild(guild).is_some() {
                    self.admin.discover = None;
                    self.actions.push(crate::app::Action::OpenServer(guild));
                    return;
                }
                self.api(
                    Tag::JoinedDiscoverable { guild },
                    Method::PUT,
                    format!("/guilds/{guild}/members/@me?lurker=false"),
                    Some(json!({})),
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
                let body = json!({ "name": settings.role_name.trim(), "color": color });
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
        use base64::Engine;
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
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
        let uri = format!(
            "data:image/{kind};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        );
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
            Modal::Ban { guild, user } => {
                self.api(
                    Tag::Removed { user },
                    Method::PUT,
                    format!("/guilds/{guild}/bans/{user}"),
                    Some(json!({ "delete_message_seconds": 0 })),
                );
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
                // Discord asks for a captcha to join from outside its own
                // apps: the invite opens in the browser instead.
                if let Tag::Discover = tag {
                    if let Some(discover) = &mut self.admin.discover {
                        discover.loading = false;
                        discover.error = Some(error);
                    }
                    return;
                }
                if let Tag::JoinedDiscoverable { guild } = tag {
                    self.apply_chat(crate::chat::ChatAction::OpenUrl(format!(
                        "https://discord.com/channels/{guild}"
                    )));
                    return;
                }
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
            Tag::Discover => {
                if let Some(discover) = &mut self.admin.discover {
                    discover.loading = false;
                    let list = value["guilds"].as_array().or(value.as_array());
                    discover.guilds = list
                        .into_iter()
                        .flatten()
                        .filter_map(discover_guild)
                        .collect();
                }
            }
            Tag::JoinedDiscoverable { guild } => {
                self.admin.discover = None;
                self.open_guild_when_ready(guild);
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
