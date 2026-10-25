//! Getting around, as Discord does: the quick switcher (Ctrl+K), the
//! keyboard shortcuts, pinned messages, the inbox, search, the friends
//! page, jumping to a message and marking one unread. Requests go through
//! the backend's generic API call, tagged so their answers come back here.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use reqwest::Method;
use serde_json::{Value, json};

use crate::app::{Action, App, Place};
use crate::backend::Command;
use crate::discord::parse;
use crate::model::{ChannelKind, Id, Message};

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    Pins {
        channel: Id,
    },
    Mentions,
    Search {
        query: String,
    },
    Around {
        channel: Id,
    },
    AddFriend,
    /// Only a failure needs telling.
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Popout {
    Pins,
    Inbox,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InboxTab {
    #[default]
    Mentions,
    Unreads,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FriendsTab {
    #[default]
    Online,
    All,
    Pending,
    Blocked,
    Add,
}

/// A message shown away from its channel: a pin, a mention, a search hit.
#[derive(Clone, Debug)]
pub struct Found {
    pub channel: Id,
    pub guild: Option<Id>,
    pub message: Message,
}

pub struct Search {
    pub query: String,
    /// `None` while Discord is still looking.
    pub results: Option<Vec<Found>>,
    pub total: u64,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Switcher {
    pub query: String,
    pub selected: usize,
}

#[derive(Default)]
pub struct Nav {
    pub switcher: Option<Switcher>,
    /// The Ctrl+/ list of shortcuts.
    pub shortcuts: bool,
    pub popout: Option<Popout>,
    /// Each channel's pins once loaded, newest first.
    pub pins: HashMap<Id, Vec<Found>>,
    pub inbox_tab: InboxTab,
    pub mentions: Option<Vec<Found>>,
    /// The header's search field.
    pub search_text: String,
    pub focus_search: bool,
    pub search: Option<Search>,
    pub friends_tab: FriendsTab,
    pub friend_filter: String,
    pub add_friend: String,
    /// What Discord said to the last friend request.
    pub add_result: Option<Result<String, String>>,
    /// The message to bring into view and light up, and when.
    pub jump: Option<Jump>,
    /// Channels showing older messages after a jump: the newest are not
    /// loaded until "Ir para o presente".
    pub detached: HashSet<Id>,
    /// The direct message last open at home; none is the friends page.
    pub home_channel: Option<Id>,
    /// The rail's folders shown open.
    pub open_folders: HashSet<u64>,
}

#[derive(Clone, Copy, Debug)]
pub struct Jump {
    pub channel: Id,
    pub message: Id,
    pub at: Instant,
    pub scrolled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Channel(Id),
    Guild(Id),
    User(Id),
}

#[derive(Clone, Debug)]
pub enum NavAction {
    ToggleSwitcher,
    CloseSwitcher,
    SwitchTo(Target),
    ToggleShortcuts,
    TogglePopout(Popout),
    ClosePopout,
    InboxTab(InboxTab),
    Search(String),
    CloseSearch,
    Jump {
        channel: Id,
        message: Id,
    },
    JumpToPresent(Id),
    Friends,
    FriendsTab(FriendsTab),
    AddFriend,
    Accept(Id),
    /// Unfriends, declines, cancels or unblocks: Discord's one call.
    RemoveRelationship(Id),
    Block(Id),
    MarkUnread {
        channel: Id,
        message: Id,
    },
    StepServer(i32),
    StepUnread(i32),
    ToggleFolder(u64),
    /// A channel dragged to sit before another in a category, or last.
    MoveChannel {
        channel: Id,
        parent: Option<Id>,
        before: Option<Id>,
    },
    MoveGuild {
        guild: Id,
        drop: crate::folders::Drop,
    },
}

/// Discord's relationship types.
pub mod relationship {
    pub const FRIEND: u8 = 1;
    pub const BLOCKED: u8 = 2;
    pub const INCOMING: u8 = 3;
    pub const OUTGOING: u8 = 4;
}

/// Every shortcut, for the Ctrl+/ list and the settings page.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl + K", "Encontrar ou começar uma conversa"),
    ("Ctrl + /", "Mostrar os atalhos de teclado"),
    ("Alt + ↑ / ↓", "Canal anterior / próximo"),
    ("Alt + Shift + ↑ / ↓", "Canal não lido anterior / próximo"),
    ("Ctrl + Alt + ↑ / ↓", "Servidor anterior / próximo"),
    ("Esc", "Marcar o canal como lido"),
    ("Shift + Esc", "Marcar o servidor como lido"),
    ("Ctrl + I", "Abrir a caixa de entrada"),
    ("Ctrl + P", "Mostrar as mensagens fixadas"),
    ("Ctrl + U", "Mostrar ou ocultar a lista de membros"),
    ("Ctrl + F", "Buscar"),
    ("Ctrl + E", "Seletor de emoji"),
    ("Ctrl + G", "Seletor de GIFs"),
    ("Ctrl + S", "Seletor de figurinhas"),
    ("Ctrl + Shift + U", "Enviar um arquivo"),
    ("Ctrl + Shift + M", "Ativar ou desativar o microfone"),
    ("Ctrl + Shift + D", "Ativar ou desativar o áudio"),
    ("Ctrl + ,", "Configurações de usuário"),
    ("Alt + clique em uma mensagem", "Marcar como não lida"),
    ("Enter", "Enviar mensagem"),
    ("Shift + Enter", "Nova linha"),
    ("↑ com o campo vazio", "Editar a última mensagem"),
    ("Tab / Enter", "Escolher uma sugestão (@, :, #)"),
    ("Shift + clique em excluir", "Excluir sem confirmar"),
    ("Ctrl + + / Ctrl + −", "Zoom"),
    ("F12", "Desempenho (FPS, memória, CPU)"),
];

/// One row of the quick switcher.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub target: Target,
    pub title: String,
    /// The server a channel is on.
    pub detail: Option<String>,
    pub unread: bool,
    pub mentions: u32,
}

/// The switcher's rows for what was typed. `@` keeps people, `#` text
/// channels, `!` voice channels and `*` servers, as in Discord. With
/// nothing typed: what is unread, then the recent direct messages.
pub fn candidates(app: &App, query: &str) -> Vec<Candidate> {
    let model = &app.model;
    let (filter, text) = match query.chars().next() {
        Some(c @ ('@' | '#' | '!' | '*')) => (Some(c), query[1..].trim()),
        _ => (None, query.trim()),
    };
    let text = text.to_lowercase();
    let mut out: Vec<(u8, Candidate)> = Vec::new();
    let mut add = |name: &str, candidate: Candidate| {
        if let Some(rank) = rank(&name.to_lowercase(), &text) {
            out.push((rank, candidate));
        }
    };
    if matches!(filter, None | Some('#') | Some('!')) {
        for guild in &model.guilds {
            for id in &guild.channels {
                let Some(c) = model.channel(*id) else {
                    continue;
                };
                let wanted = match filter {
                    Some('!') => c.kind == ChannelKind::Voice,
                    _ => c.kind == ChannelKind::Text,
                };
                if !wanted || !model.can_view(c) {
                    continue;
                }
                add(
                    &c.name,
                    Candidate {
                        target: Target::Channel(c.id),
                        title: c.name.clone(),
                        detail: Some(guild.name.clone()),
                        unread: c.unread(),
                        mentions: c.mentions,
                    },
                );
            }
        }
    }
    if matches!(filter, None | Some('@')) {
        let mut seen = HashSet::new();
        for id in model.sorted_direct() {
            let Some(c) = model.channel(id) else { continue };
            if c.kind == ChannelKind::Direct
                && let Some(user) = c.recipients.first()
            {
                seen.insert(*user);
            }
            let name = model.direct_name(c);
            add(
                &name,
                Candidate {
                    target: Target::Channel(id),
                    title: name.clone(),
                    detail: None,
                    unread: c.unread(),
                    mentions: c.mentions,
                },
            );
        }
        // Friends without a conversation yet.
        if !text.is_empty() {
            for (user, kind) in &model.relationships {
                if *kind != relationship::FRIEND || seen.contains(user) {
                    continue;
                }
                let Some(u) = model.user(*user) else { continue };
                let name = format!("{} {}", u.name, u.username);
                add(
                    &name,
                    Candidate {
                        target: Target::User(*user),
                        title: u.name.clone(),
                        detail: Some(u.username.clone()),
                        unread: false,
                        mentions: 0,
                    },
                );
            }
        }
    }
    if matches!(filter, None | Some('*')) && (!text.is_empty() || filter.is_some()) {
        for guild in &model.guilds {
            let (unread, mentions) = model.guild_unread(guild);
            add(
                &guild.name,
                Candidate {
                    target: Target::Guild(guild.id),
                    title: guild.name.clone(),
                    detail: None,
                    unread,
                    mentions,
                },
            );
        }
    }
    if text.is_empty() && filter.is_none() {
        // Nothing typed: what waits for the user first, then the recent
        // conversations, as Discord's "previous channels".
        let current = app.channel.map(Target::Channel);
        out.retain(|(_, c)| Some(c.target) != current);
        out.sort_by_key(|(_, c)| (c.mentions == 0, !c.unread));
        out.retain(|(_, c)| c.unread || c.mentions > 0 || c.detail.is_none());
    } else {
        out.sort_by_key(|(rank, c)| (*rank, c.mentions == 0, !c.unread, c.title.len()));
    }
    out.into_iter().map(|(_, c)| c).take(30).collect()
}

/// How well `name` matches: from the start, a word's start, anywhere,
/// or its letters in order. `None` when it does not.
fn rank(name: &str, text: &str) -> Option<u8> {
    if text.is_empty() {
        return Some(0);
    }
    if name.starts_with(text) {
        return Some(0);
    }
    if name
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(text))
    {
        return Some(1);
    }
    if name.contains(text) {
        return Some(2);
    }
    let mut letters = name.chars();
    text.chars()
        .all(|wanted| letters.any(|c| c == wanted))
        .then_some(3)
}

/// Letters and digits stay; everything else is %-escaped for a query.
fn escape(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

impl App {
    fn nav_api(&self, tag: Tag, method: Method, path: String, body: Option<Value>) {
        self.backend().send(Command::Api {
            tag: crate::backend::Tag::Nav(tag),
            method,
            path,
            body,
        });
    }

    pub fn apply_nav(&mut self, action: NavAction) {
        match action {
            NavAction::ToggleSwitcher => {
                self.nav.switcher = match self.nav.switcher {
                    Some(_) => None,
                    None => Some(Switcher::default()),
                };
                self.nav.popout = None;
            }
            NavAction::CloseSwitcher => {
                self.nav.switcher = None;
                self.chat.focus_composer = true;
            }
            NavAction::SwitchTo(target) => {
                self.nav.switcher = None;
                match target {
                    Target::Channel(id) => self.open_channel(id),
                    Target::Guild(id) => self.actions.push(Action::OpenServer(id)),
                    Target::User(user) => {
                        self.actions
                            .push(Action::Chat(crate::chat::ChatAction::MessageUser {
                                user,
                                text: String::new(),
                            }))
                    }
                }
            }
            NavAction::ToggleShortcuts => self.nav.shortcuts = !self.nav.shortcuts,
            NavAction::TogglePopout(popout) => {
                if self.nav.popout == Some(popout) {
                    self.nav.popout = None;
                    return;
                }
                self.nav.popout = Some(popout);
                match popout {
                    Popout::Pins => {
                        if let Some(channel) = self.channel {
                            self.nav_api(
                                Tag::Pins { channel },
                                Method::GET,
                                format!("/channels/{channel}/pins"),
                                None,
                            );
                        }
                    }
                    Popout::Inbox => {
                        self.nav.mentions = None;
                        self.nav_api(
                            Tag::Mentions,
                            Method::GET,
                            "/users/@me/mentions?limit=25&roles=true&everyone=true".into(),
                            None,
                        );
                    }
                }
            }
            NavAction::ClosePopout => self.nav.popout = None,
            NavAction::InboxTab(tab) => self.nav.inbox_tab = tab,
            NavAction::Search(query) => {
                let query = query.trim().to_owned();
                if query.is_empty() {
                    self.nav.search = None;
                    return;
                }
                let path = match (self.place, self.channel) {
                    (Place::Server(guild), _) => {
                        format!("/guilds/{guild}/messages/search?content={}", escape(&query))
                    }
                    (Place::Home, Some(channel)) => format!(
                        "/channels/{channel}/messages/search?content={}",
                        escape(&query)
                    ),
                    (Place::Home, None) => return,
                };
                self.nav.search = Some(Search {
                    query: query.clone(),
                    results: None,
                    total: 0,
                    error: None,
                });
                self.nav_api(Tag::Search { query }, Method::GET, path, None);
            }
            NavAction::CloseSearch => {
                self.nav.search = None;
                self.nav.search_text.clear();
            }
            NavAction::Jump { channel, message } => self.jump(channel, message),
            NavAction::JumpToPresent(channel) => {
                self.nav.detached.remove(&channel);
                if let Some(c) = self.model.channels.get_mut(&channel) {
                    c.loading = true;
                }
                self.backend().send(Command::LoadMessages {
                    channel,
                    before: None,
                });
            }
            NavAction::Friends => {
                self.place = Place::Home;
                self.channel = None;
                self.nav.home_channel = None;
                self.chat.popout = None;
            }
            NavAction::FriendsTab(tab) => {
                self.nav.friends_tab = tab;
                self.nav.add_result = None;
            }
            NavAction::AddFriend => {
                let name = self
                    .nav
                    .add_friend
                    .trim()
                    .trim_start_matches('@')
                    .to_owned();
                if name.is_empty() {
                    return;
                }
                self.nav.add_result = None;
                self.nav_api(
                    Tag::AddFriend,
                    Method::POST,
                    "/users/@me/relationships".into(),
                    Some(json!({ "username": name, "discriminator": null })),
                );
            }
            NavAction::Accept(user) => self.nav_api(
                Tag::Done,
                Method::PUT,
                format!("/users/@me/relationships/{user}"),
                Some(json!({})),
            ),
            NavAction::RemoveRelationship(user) => self.nav_api(
                Tag::Done,
                Method::DELETE,
                format!("/users/@me/relationships/{user}"),
                None,
            ),
            NavAction::Block(user) => self.nav_api(
                Tag::Done,
                Method::PUT,
                format!("/users/@me/relationships/{user}"),
                Some(json!({ "type": relationship::BLOCKED })),
            ),
            NavAction::MarkUnread { channel, message } => self.mark_unread(channel, message),
            NavAction::StepServer(step) => self.step_server(step),
            NavAction::StepUnread(step) => self.step_unread(step),
            NavAction::ToggleFolder(id) => {
                if !self.nav.open_folders.remove(&id) {
                    self.nav.open_folders.insert(id);
                }
            }
            NavAction::MoveGuild { guild, drop } => self.move_guild(guild, drop),
            NavAction::MoveChannel {
                channel,
                parent,
                before,
            } => self.move_channel(channel, parent, before),
        }
    }

    /// Opens the channel and brings the message into view, loading the
    /// messages around it when they are not there.
    fn jump(&mut self, channel: Id, message: Id) {
        self.nav.popout = None;
        self.open_channel(channel);
        let loaded = self
            .model
            .channel(channel)
            .is_some_and(|c| c.messages.iter().any(|m| m.id == message));
        if !loaded {
            self.nav.detached.insert(channel);
            if let Some(c) = self.model.channels.get_mut(&channel) {
                c.loading = true;
            }
            self.nav_api(
                Tag::Around { channel },
                Method::GET,
                format!("/channels/{channel}/messages?around={message}&limit=50"),
                None,
            );
        }
        self.nav.jump = Some(Jump {
            channel,
            message,
            at: Instant::now(),
            scrolled: false,
        });
    }

    /// The channel reads up to the message before this one, as Discord's
    /// Alt+click: the message and what follows count as new again.
    fn mark_unread(&mut self, channel: Id, message: Id) {
        let Some(c) = self.model.channels.get_mut(&channel) else {
            return;
        };
        let before = c
            .messages
            .iter()
            .rev()
            .find(|m| m.id < message && !m.pending)
            .map_or(message - 1, |m| m.id);
        c.read = Some(before);
        self.chat.unread_from.insert(channel, before);
        self.nav_api(
            Tag::Done,
            Method::POST,
            format!("/channels/{channel}/messages/{before}/ack"),
            Some(json!({ "manual": true, "mention_count": 0 })),
        );
    }

    /// Puts a channel before another in a category, or at its end, and
    /// tells Discord every position in that category.
    fn move_channel(&mut self, channel: Id, parent: Option<Id>, before: Option<Id>) {
        let Some(guild) = self.model.channel(channel).and_then(|c| c.guild) else {
            return;
        };
        let Some(g) = self.model.guild(guild) else {
            return;
        };
        let (loose, categories) = self.model.layout(g);
        let mut list: Vec<Id> = match parent {
            None => loose,
            Some(p) => categories
                .into_iter()
                .find(|(c, _)| *c == p)
                .map(|(_, l)| l)
                .unwrap_or_default(),
        };
        list.retain(|c| *c != channel);
        let at = before
            .and_then(|b| list.iter().position(|c| *c == b))
            .unwrap_or(list.len());
        list.insert(at, channel);
        let mut body = Vec::new();
        for (position, id) in list.iter().enumerate() {
            if let Some(c) = self.model.channels.get_mut(id) {
                c.position = position as i64;
                c.parent = parent;
            }
            body.push(json!({
                "id": id.to_string(),
                "position": position,
                "parent_id": parent.map(|p| p.to_string()),
            }));
        }
        self.nav_api(
            Tag::Done,
            Method::PATCH,
            format!("/guilds/{guild}/channels"),
            Some(Value::Array(body)),
        );
    }

    /// Ctrl+Alt+↑/↓: the server above or below, home at the top.
    fn step_server(&mut self, step: i32) {
        let mut places = vec![Place::Home];
        places.extend(self.model.guilds.iter().map(|g| Place::Server(g.id)));
        let at = places.iter().position(|p| *p == self.place).unwrap_or(0) as i64;
        let next = (at + i64::from(step)).rem_euclid(places.len() as i64) as usize;
        match places[next] {
            Place::Home => self.actions.push(Action::GoHome),
            Place::Server(id) => self.actions.push(Action::OpenServer(id)),
        }
    }

    /// Alt+Shift+↑/↓: the next unread channel, across servers, in the
    /// order the rail and the channel lists show them.
    fn step_unread(&mut self, step: i32) {
        let mut order: Vec<Id> = self.model.sorted_direct();
        for guild in &self.model.guilds {
            let (loose, categories) = self.model.layout(guild);
            order.extend(loose);
            order.extend(categories.into_iter().flat_map(|(_, l)| l));
        }
        let unread = |id: &Id| {
            self.model
                .channel(*id)
                .is_some_and(|c| c.kind.is_textual() && (c.unread() || c.mentions > 0))
        };
        let at = self
            .channel
            .and_then(|c| order.iter().position(|x| *x == c))
            .map_or(-1, |p| p as i64);
        let len = order.len() as i64;
        for offset in 1..=len {
            let i = (at + offset * i64::from(step.signum())).rem_euclid(len) as usize;
            if unread(&order[i]) {
                let id = order[i];
                self.open_channel(id);
                return;
            }
        }
    }

    /// A message from an API answer, with the people in it learnt.
    fn found(&mut self, value: &Value) -> Option<Found> {
        let channel = parse::id(&value["channel_id"])?;
        let guild = parse::id(&value["guild_id"])
            .or_else(|| self.model.channel(channel).and_then(|c| c.guild));
        self.learn(guild, value);
        let message = parse::message(value, self.model.me)?;
        Some(Found {
            channel,
            guild,
            message,
        })
    }

    pub fn nav_result(&mut self, tag: Tag, result: Result<Value, String>) {
        match (tag, result) {
            (Tag::Pins { channel }, Ok(value)) => {
                // The old list, or the newer {items: [{message}]}.
                let list: Vec<Value> = match value["items"].as_array() {
                    Some(items) => items.iter().map(|i| i["message"].clone()).collect(),
                    None => value.as_array().cloned().unwrap_or_default(),
                };
                let found = list.iter().filter_map(|v| self.found(v)).collect();
                self.nav.pins.insert(channel, found);
            }
            (Tag::Mentions, Ok(value)) => {
                let list = value.as_array().cloned().unwrap_or_default();
                let found = list.iter().filter_map(|v| self.found(v)).collect();
                self.nav.mentions = Some(found);
            }
            (Tag::Mentions, Err(_)) => self.nav.mentions = Some(Vec::new()),
            (Tag::Search { query }, result) => {
                let Some(search) = self.nav.search.as_ref().filter(|s| s.query == query) else {
                    return;
                };
                let _ = search;
                match result {
                    Ok(value) => {
                        let total = value["total_results"].as_u64().unwrap_or(0);
                        let hits: Vec<Value> = value["messages"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|group| group.as_array().and_then(|g| g.first()).cloned())
                            .collect();
                        let found = hits.iter().filter_map(|v| self.found(v)).collect();
                        if let Some(search) = &mut self.nav.search {
                            search.total = total;
                            search.results = Some(found);
                        }
                    }
                    Err(error) => {
                        if let Some(search) = &mut self.nav.search {
                            search.results = Some(Vec::new());
                            search.error = Some(error);
                        }
                    }
                }
            }
            (Tag::Around { channel }, result) => {
                let list = result
                    .ok()
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default();
                let guild = self.model.channel(channel).and_then(|c| c.guild);
                let me = self.model.me;
                let mut messages = Vec::new();
                for value in list.iter().rev() {
                    self.learn(guild, value);
                    if let Some(m) = parse::message(value, me) {
                        messages.push(m);
                    }
                }
                if let Some(c) = self.model.channels.get_mut(&channel) {
                    c.loading = false;
                    if !messages.is_empty() {
                        c.messages = messages;
                        c.more = true;
                    }
                }
            }
            (Tag::AddFriend, result) => {
                self.nav.add_result = Some(match result {
                    Ok(_) => {
                        let name = std::mem::take(&mut self.nav.add_friend);
                        Ok(format!(
                            "Pronto! Seu pedido de amizade para {name} foi enviado."
                        ))
                    }
                    Err(error) => Err(if error.contains("80004") || error.contains("Unknown") {
                        "Hum, não funcionou. Confira se o nome de usuário está certo.".into()
                    } else {
                        error
                    }),
                });
            }
            (Tag::Done, Err(error)) => {
                self.chat.dialog = Some(crate::chat::Dialog::Failed(error));
            }
            (Tag::Pins { .. } | Tag::Done, _) => {}
        }
    }
}
