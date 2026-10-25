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
    Nick { guild: Id, nick: String },
    Created,
    Left { guild: Id },
    GuildSaved,
    RoleSaved,
    Invites { guild: Id },
    Revoked { code: String },
    Bans { guild: Id },
    Unbanned { user: Id },
    Removed { user: Id },
    Audit { guild: Id },
    Deleted { guild: Id },
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

#[derive(Default)]
pub struct Admin {
    pub menu: bool,
    pub modal: Option<Modal>,
    pub busy: bool,
    pub error: Option<String>,
    pub settings: Option<GuildSettings>,
}

#[derive(Clone, Debug)]
pub enum AdminAction {
    ToggleMenu,
    CloseMenu,
    Open(Modal),
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
}

impl App {
    fn api(&self, tag: Tag, method: Method, path: String, body: Option<Value>) {
        self.backend().send(Command::Api {
            tag,
            method,
            path,
            body,
        });
    }

    pub fn apply_admin(&mut self, action: AdminAction) {
        match action {
            AdminAction::ToggleMenu => self.admin.menu = !self.admin.menu,
            AdminAction::CloseMenu => self.admin.menu = false,
            AdminAction::Open(modal) => {
                self.admin.menu = false;
                self.admin.error = None;
                self.admin.busy = false;
                if let Modal::Invite { .. } = modal {
                    self.create_invite();
                }
                self.admin.modal = Some(modal);
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

    /// A week-long invite to the open channel, as Discord's default.
    fn create_invite(&mut self) {
        let Some(channel) = self.channel else { return };
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
                if self.admin.modal.is_some() {
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
