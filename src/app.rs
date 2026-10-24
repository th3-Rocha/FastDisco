//! Application state. Views draw from it and push `Action`s; `App::apply`
//! runs them after the frame is drawn, so no view mutates what another is
//! still reading. Backend events arrive through `App::poll`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::backend::{Backend, Command, Connection, Event};
use crate::chat::{Chat, ChatAction};
use crate::discord::parse;
use crate::login::{Login, Qr, View};
use crate::model::{ChannelKind, Id, Message, Model, Status};
use crate::theme::Palette;

/// What fills the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Checking the saved token, or waiting for READY.
    Connecting,
    Login,
    Main,
}

/// Where the user is: the home screen with direct messages, or a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Home,
    Server(Id),
}

#[derive(Clone, Debug)]
pub enum Action {
    GoHome,
    OpenServer(Id),
    OpenChannel(Id),
    ToggleCategory(Id),
    ToggleMembers,
    Send,
    ToggleMute,
    ToggleDeafen,
    LoginSubmit,
    MfaSubmit,
    TokenSubmit,
    Forgot,
    Register,
    ShowToken,
    LoginBack,
    RetryQr,
    /// Asks before logging out.
    AskLogout,
    CancelLogout,
    Logout,
    Chat(ChatAction),
    MarkRead(Id),
    MarkGuildRead(Id),
    /// Alt+Up and Alt+Down: the channel above or below the open one.
    StepChannel(i32),
}

#[derive(Clone, PartialEq, Eq)]
enum Picture {
    Requested,
    /// The URI egui's loaders know the bytes by.
    Loaded(String),
    Failed,
}

pub struct App {
    pub screen: Screen,
    pub login: Login,
    pub confirm_logout: bool,
    pub connection: Connection,
    backend: Backend,
    pub model: Model,
    pub palette: Palette,
    pub place: Place,
    pub channel: Option<Id>,
    /// The channel last open on each server.
    last_channel: HashMap<Id, Id>,
    pub collapsed: HashSet<Id>,
    pub show_members: bool,
    pub muted: bool,
    pub deafened: bool,
    /// What the user is typing, per channel.
    pub drafts: HashMap<Id, String>,
    pub actions: Vec<Action>,
    pub chat: Chat,
    pub ctx: egui::Context,
    pictures: RefCell<HashMap<String, Picture>>,
    animations: RefCell<crate::animation::Animations>,
    /// Numbers each loaded picture's URI.
    loaded: u64,
}

impl App {
    pub fn new(ctx: &egui::Context) -> Self {
        let backend = Backend::start(ctx.clone());
        backend.send(Command::Resume);
        Self {
            screen: Screen::Connecting,
            login: Login::default(),
            confirm_logout: false,
            connection: Connection::Connecting,
            backend,
            model: Model::default(),
            palette: Palette::dark(),
            place: Place::Home,
            channel: None,
            last_channel: HashMap::new(),
            collapsed: HashSet::new(),
            show_members: true,
            muted: false,
            deafened: false,
            drafts: HashMap::new(),
            actions: Vec::new(),
            chat: Chat::new(),
            ctx: ctx.clone(),
            pictures: RefCell::new(HashMap::new()),
            animations: RefCell::new(crate::animation::Animations::default()),
            loaded: 0,
        }
    }

    pub fn backend(&self) -> &Backend {
        &self.backend
    }

    pub fn server(&self) -> Option<Id> {
        match self.place {
            Place::Server(id) => Some(id),
            Place::Home => None,
        }
    }

    /// The picture at `url`, as a URI for `egui::Image`, once it has
    /// loaded; the first ask downloads it.
    pub fn picture(&self, url: Option<&str>) -> Option<String> {
        let url = url?;
        let mut pictures = self.pictures.borrow_mut();
        match pictures.get(url) {
            Some(Picture::Loaded(uri)) => Some(uri.clone()),
            Some(Picture::Requested | Picture::Failed) => None,
            None => {
                pictures.insert(url.to_owned(), Picture::Requested);
                self.backend.send(Command::FetchImage(url.to_owned()));
                None
            }
        }
    }

    /// The frame to show now of a looping silent video (a GIF sent as
    /// MP4); the first ask starts decoding it.
    pub fn animation(&self, url: &str) -> Option<egui::TextureHandle> {
        self.animations.borrow_mut().frame(&self.ctx, url)
    }

    pub fn animation_failed(&self, url: &str) -> bool {
        self.animations.borrow().failed(url)
    }

    pub fn update_animations(&self) {
        self.animations.borrow_mut().update(&self.ctx);
    }

    /// Takes what the backend has sent since the last frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.backend.events.try_recv() {
            match event {
                Event::SignedOut => {
                    self.screen = Screen::Login;
                    self.login = Login::default();
                    self.model = Model::default();
                    self.backend.send(Command::StartQr);
                }
                Event::SignedIn => {
                    self.login = Login::default();
                    self.screen = Screen::Connecting;
                }
                Event::Connection(state) => self.connection = state,
                Event::Ready(model) => self.ready(*model),
                Event::Dispatch { kind, data } => self.dispatch(&kind, data),
                Event::Messages {
                    channel,
                    list,
                    older: false,
                } => self.history(channel, &list),
                Event::Messages {
                    channel,
                    list,
                    older: true,
                } => self.older(channel, &list),
                Event::SendFailed {
                    channel,
                    nonce,
                    reason,
                } => {
                    if let Some(message) = self.model.channels.get_mut(&channel).and_then(|c| {
                        c.messages
                            .iter_mut()
                            .find(|m| m.nonce.as_deref() == Some(&nonce))
                    }) {
                        message.pending = false;
                        message.failed = true;
                    }
                    self.chat.dialog = Some(crate::chat::Dialog::Failed(reason));
                }
                Event::GifHome(home) => self.chat.gif_home = Some(home),
                Event::Profile { user, profile } => {
                    self.chat.profiles.insert(
                        user,
                        (
                            crate::chat::ProfileState::Ready(profile),
                            std::time::Instant::now(),
                        ),
                    );
                }
                Event::ProfileFailed(user) => {
                    self.chat.profiles.insert(
                        user,
                        (crate::chat::ProfileState::Failed, std::time::Instant::now()),
                    );
                }
                Event::DirectOpened(value) => self.direct_opened(&value),
                Event::ApplicationIcon { application, url } => {
                    self.chat.app_icons.insert(application, url);
                }
                Event::Favorites(list) => self.chat.favorites = list,
                Event::Gifs { query, list } => {
                    let current = self.chat.picker.as_ref().and_then(|p| p.searched.clone());
                    if current.as_deref().unwrap_or_default()
                        == query.as_deref().unwrap_or_default()
                    {
                        self.chat.gifs = list;
                    }
                }
                Event::FilesPicked(paths) => self.apply_chat(ChatAction::AddFiles(paths)),
                Event::Image { url, bytes, ext } => {
                    let mut pictures = self.pictures.borrow_mut();
                    self.loaded += 1;
                    let uri = format!("bytes://fastdisco/{}.{ext}", self.loaded);
                    pictures.insert(url, Picture::Loaded(uri.clone()));
                    ctx.include_bytes(uri, bytes);
                }
                Event::ImageFailed(url) => {
                    self.pictures.borrow_mut().insert(url, Picture::Failed);
                }
                event => {
                    if self.login.handle(ctx, event) {
                        self.backend.send(Command::StartQr);
                    }
                }
            }
        }
    }

    fn ready(&mut self, model: Model) {
        let first = self.screen != Screen::Main;
        self.model = model;
        self.screen = Screen::Main;
        // After a reconnect, stay where the user was if it still exists.
        let still_there = self
            .channel
            .is_some_and(|c| self.model.channels.contains_key(&c));
        if first || !still_there {
            match self.model.guilds.first().map(|g| g.id) {
                Some(guild) => self.open_server(guild),
                None => self.go_home(),
            }
            self.open_named();
        } else if let Some(channel) = self.channel {
            self.open_channel(channel);
        }
    }

    /// Development aid: `FASTDISCO_OPEN=name` opens the first text channel
    /// whose name contains it, to check a screen without clicking there.
    fn open_named(&mut self) {
        if let Ok(wanted) = std::env::var("FASTDISCO_OPEN") {
            let wanted = crate::text::hyphens(&wanted).into_owned();
            let found = self.model.guilds.iter().find_map(|g| {
                g.channels
                    .iter()
                    .copied()
                    .find(|c| {
                        self.model.channel(*c).is_some_and(|c| {
                            c.kind.is_textual()
                                && c.name.contains(&wanted)
                                && self.model.can_view(c)
                        })
                    })
                    .map(|c| (g.id, c))
            });
            if let Some((_, channel)) = found {
                self.open_channel(channel);
            }
        }
        let tab = match std::env::var("FASTDISCO_PICKER").as_deref() {
            Ok("emoji") => Some(crate::chat::PickerTab::Emoji),
            Ok("gif") => Some(crate::chat::PickerTab::Gif),
            Ok("sticker") => Some(crate::chat::PickerTab::Sticker),
            _ => None,
        };
        // FASTDISCO_DM opens the newest direct message; FASTDISCO_PROFILE
        // shows the user's own card.
        if std::env::var("FASTDISCO_DM").is_ok()
            && let Some(first) = self.model.sorted_direct().into_iter().find(|c| {
                self.model
                    .channel(*c)
                    .is_some_and(|c| c.kind == crate::model::ChannelKind::Direct)
            })
        {
            self.go_direct(first);
        }
        if std::env::var("FASTDISCO_PROFILE").is_ok() {
            let anchor =
                egui::Rect::from_min_size(egui::pos2(80.0, 900.0), egui::vec2(160.0, 40.0));
            let me = self.model.me;
            self.apply_chat(ChatAction::ShowProfile {
                user: me,
                anchor,
                side: crate::chat::Side::Above,
            });
        }
        if let Some(tab) = tab {
            self.apply_chat(ChatAction::OpenPicker {
                tab,
                target: crate::chat::PickerTarget::Composer,
            });
        }
    }

    pub fn apply(&mut self) {
        for action in std::mem::take(&mut self.actions) {
            match action {
                Action::LoginSubmit => {
                    let login = &mut self.login;
                    login.clear_errors();
                    let required = "Este campo é obrigatório";
                    if login.email.trim().is_empty() {
                        login.login_error = Some(required.into());
                    }
                    if login.password.is_empty() {
                        login.password_error = Some(required.into());
                    }
                    if login.login_error.is_none() && login.password_error.is_none() {
                        login.busy = true;
                        self.backend.send(Command::Login {
                            login: login.email.trim().to_owned(),
                            password: login.password.clone(),
                        });
                    }
                }
                Action::MfaSubmit => {
                    let login = &mut self.login;
                    login.clear_errors();
                    if let View::Mfa { ticket } = &login.view {
                        if login.code.trim().is_empty() {
                            login.code_error = Some("Este campo é obrigatório".into());
                        } else {
                            login.busy = true;
                            self.backend.send(Command::Mfa {
                                ticket: ticket.clone(),
                                code: login.code.clone(),
                            });
                        }
                    }
                }
                Action::TokenSubmit => {
                    let login = &mut self.login;
                    login.clear_errors();
                    if login.token.trim().is_empty() {
                        login.token_error = Some("Este campo é obrigatório".into());
                    } else {
                        login.busy = true;
                        self.backend
                            .send(Command::Token(std::mem::take(&mut login.token)));
                    }
                }
                Action::Forgot => {
                    let login = &mut self.login;
                    login.clear_errors();
                    if login.email.trim().is_empty() {
                        login.login_error = Some("Este campo é obrigatório".into());
                    } else {
                        self.backend.send(Command::Forgot {
                            login: login.email.trim().to_owned(),
                        });
                    }
                }
                Action::Register => {
                    let _ = open::that_detached("https://discord.com/register");
                }
                Action::ShowToken => self.login.go(View::Token),
                Action::LoginBack => self.login.go(View::Form),
                Action::RetryQr => {
                    self.login.qr = Qr::Waiting;
                    self.backend.send(Command::StartQr);
                }
                Action::AskLogout => self.confirm_logout = true,
                Action::CancelLogout => self.confirm_logout = false,
                Action::Logout => {
                    self.confirm_logout = false;
                    self.screen = Screen::Connecting;
                    self.backend.send(Command::Logout);
                }
                Action::GoHome => self.go_home(),
                Action::OpenServer(id) => self.open_server(id),
                Action::OpenChannel(id) => self.open_channel(id),
                Action::ToggleCategory(id) => {
                    if !self.collapsed.remove(&id) {
                        self.collapsed.insert(id);
                    }
                }
                Action::ToggleMembers => self.show_members = !self.show_members,
                Action::Send => self.send_draft(),
                Action::Chat(action) => self.apply_chat(action),
                Action::MarkRead(id) => self.mark_read(id),
                Action::MarkGuildRead(guild) => {
                    let channels = self
                        .model
                        .guild(guild)
                        .map(|g| g.channels.clone())
                        .unwrap_or_default();
                    for id in channels {
                        if self
                            .model
                            .channel(id)
                            .is_some_and(|c| c.unread() || c.mentions > 0)
                        {
                            self.mark_read(id);
                        }
                    }
                }
                Action::StepChannel(step) => self.step_channel(step),
                Action::ToggleMute => {
                    self.muted = !self.muted;
                    if !self.muted {
                        self.deafened = false;
                    }
                }
                Action::ToggleDeafen => {
                    self.deafened = !self.deafened;
                    self.muted = self.deafened;
                }
            }
        }
    }

    /// The open place's text channels in their shown order, and moves
    /// `step` along them.
    fn step_channel(&mut self, step: i32) {
        let list: Vec<Id> = match self.place {
            Place::Server(g) => {
                let Some(guild) = self.model.guild(g) else {
                    return;
                };
                let (loose, categories) = self.model.layout(guild);
                loose
                    .into_iter()
                    .chain(categories.into_iter().flat_map(|(_, l)| l))
                    .collect()
            }
            Place::Home => self.model.sorted_direct(),
        };
        let list: Vec<Id> = list
            .into_iter()
            .filter(|c| self.model.channel(*c).is_some_and(|c| c.kind.is_textual()))
            .collect();
        let Some(at) = self.channel.and_then(|c| list.iter().position(|x| *x == c)) else {
            return;
        };
        let next = at as i64 + i64::from(step);
        if let Some(id) = usize::try_from(next).ok().and_then(|n| list.get(n)) {
            self.open_channel(*id);
        }
    }

    /// Opens a direct message from anywhere.
    pub fn go_direct(&mut self, channel: Id) {
        self.place = Place::Home;
        self.open_channel(channel);
    }

    fn go_home(&mut self) {
        self.place = Place::Home;
        self.channel = None;
        if let Some(first) = self.model.sorted_direct().first().copied() {
            self.open_channel(first);
        }
    }

    fn open_server(&mut self, id: Id) {
        self.place = Place::Server(id);
        self.channel = None;
        let channel = self
            .last_channel
            .get(&id)
            .copied()
            .filter(|c| self.model.channels.contains_key(c))
            .or_else(|| self.model.first_text_channel(id));
        if let Some(channel) = channel {
            self.open_channel(channel);
        }
    }

    /// Shows a channel: loads its history the first time, marks it read
    /// and asks for the server's member list.
    fn open_channel(&mut self, id: Id) {
        let Some(channel) = self.model.channels.get_mut(&id) else {
            return;
        };
        if !channel.kind.is_textual() {
            return;
        }
        // A channel opened from a mention may be on another server.
        self.place = match channel.guild {
            Some(guild) => Place::Server(guild),
            None => Place::Home,
        };
        if self.channel != Some(id) {
            self.chat.player = None;
            self.chat.popout = None;
            self.chat.picker = None;
            self.chat.editing = None;
        }
        if self.channel != Some(id) {
            match channel.read.filter(|_| channel.unread()) {
                Some(read) => self.chat.unread_from.insert(id, read),
                None => self.chat.unread_from.remove(&id),
            };
        }
        self.channel = Some(id);
        self.chat.focus_composer = true;
        if !channel.requested {
            channel.requested = true;
            self.backend.send(Command::LoadMessages {
                channel: id,
                before: None,
            });
        }
        let guild = channel.guild;
        if let Some(guild) = guild {
            self.last_channel.insert(guild, id);
            if let Some(g) = self.model.guild_mut(guild)
                && !g.subscribed
            {
                g.subscribed = true;
                self.backend.send(Command::Subscribe { guild, channel: id });
            }
        }
        self.mark_read(id);
    }

    fn mark_read(&mut self, id: Id) {
        let Some(channel) = self.model.channels.get_mut(&id) else {
            return;
        };
        let was_unread = channel.unread() || channel.mentions > 0;
        channel.mentions = 0;
        if let Some(last) = channel.last_message {
            channel.read = Some(last);
            if was_unread {
                self.backend.send(Command::Ack {
                    channel: id,
                    message: last,
                });
            }
        }
    }

    fn history(&mut self, id: Id, list: &[Value]) {
        let me = self.model.me;
        let guild = self.model.channel(id).and_then(|c| c.guild);
        let mut messages: Vec<Message> = Vec::new();
        for value in list.iter().rev() {
            self.learn(guild, value);
            if let Some(m) = parse::message(value, me) {
                messages.push(m);
            }
        }
        if let Some(channel) = self.model.channels.get_mut(&id) {
            channel.more = list.len() >= 50;
            // Keep anything that arrived live while the history loaded.
            let newest = messages.last().map_or(0, |m| m.id);
            let live: Vec<Message> = channel
                .messages
                .drain(..)
                .filter(|m| m.id > newest || m.pending)
                .collect();
            messages.extend(live);
            channel.messages = messages;
        }
    }

    fn dispatch(&mut self, kind: &str, data: Value) {
        let me = self.model.me;
        match kind {
            "READY_SUPPLEMENTAL" => parse::supplemental(&data, &mut self.model),
            "MESSAGE_CREATE" => {
                let Some(channel_id) = parse::id(&data["channel_id"]) else {
                    return;
                };
                let guild = parse::id(&data["guild_id"]);
                self.learn(guild, &data);
                let Some(message) = parse::message(&data, me) else {
                    return;
                };
                self.stopped_typing(channel_id, message.author);
                let open = self.channel == Some(channel_id);
                let Some(channel) = self.model.channels.get_mut(&channel_id) else {
                    return;
                };
                channel.last_message = Some(message.id);
                if message.author == me || open {
                    channel.read = Some(message.id);
                } else if message.mentions_me || channel.kind != ChannelKind::Text {
                    channel.mentions += 1;
                }
                if open && message.author != me {
                    self.backend.send(Command::Ack {
                        channel: channel_id,
                        message: message.id,
                    });
                }
                if !channel.requested {
                    return;
                }
                match channel
                    .messages
                    .iter_mut()
                    .find(|m| m.pending && m.nonce.is_some() && m.nonce == message.nonce)
                {
                    Some(pending) => *pending = message,
                    None => channel.messages.push(message),
                }
            }
            "MESSAGE_UPDATE" => {
                let (Some(channel), Some(id)) =
                    (parse::id(&data["channel_id"]), parse::id(&data["id"]))
                else {
                    return;
                };
                let guild = parse::id(&data["guild_id"]);
                // A full message replaces ours; a partial one (embeds that
                // just resolved) only adds what it carries.
                let full = data["author"]
                    .is_object()
                    .then(|| parse::message(&data, me))
                    .flatten();
                if full.is_some() {
                    self.learn(guild, &data);
                }
                let partial = parse::message(
                    &serde_json::json!({
                        "id": data["id"], "author": { "id": "0" }, "timestamp": "2015-01-01T00:00:00Z",
                        "embeds": data["embeds"],
                    }),
                    me,
                );
                if let Some(m) = self
                    .model
                    .channels
                    .get_mut(&channel)
                    .and_then(|c| c.messages.iter_mut().find(|m| m.id == id))
                {
                    match full {
                        Some(mut new) => {
                            // Reactions come by their own events.
                            new.reactions = std::mem::take(&mut m.reactions);
                            *m = new;
                        }
                        None => {
                            if let Some(content) = data["content"].as_str() {
                                m.text = content.to_owned();
                                m.edited = true;
                            }
                            if data["embeds"].is_array()
                                && let Some(partial) = partial
                            {
                                m.embeds = partial.embeds;
                            }
                        }
                    }
                }
            }
            "MESSAGE_DELETE" => {
                let (Some(channel), Some(id)) =
                    (parse::id(&data["channel_id"]), parse::id(&data["id"]))
                else {
                    return;
                };
                if let Some(c) = self.model.channels.get_mut(&channel) {
                    c.messages.retain(|m| m.id != id);
                }
            }
            "MESSAGE_ACK" => {
                let (Some(channel), Some(id)) = (
                    parse::id(&data["channel_id"]),
                    parse::id(&data["message_id"]),
                ) else {
                    return;
                };
                if let Some(c) = self.model.channels.get_mut(&channel) {
                    c.read = Some(id);
                    c.mentions = data["mention_count"].as_u64().unwrap_or(0) as u32;
                }
            }
            "CHANNEL_CREATE" | "CHANNEL_UPDATE" => {
                for u in data["recipients"].as_array().into_iter().flatten() {
                    if let Some(u) = parse::user(u) {
                        self.model.users.entry(u.id).or_insert(u);
                    }
                }
                let Some(mut channel) = parse::channel(&data, None) else {
                    return;
                };
                let id = channel.id;
                if let Some(old) = self.model.channels.remove(&id) {
                    channel.messages = old.messages;
                    channel.requested = old.requested;
                    channel.read = old.read;
                    channel.mentions = old.mentions;
                } else {
                    channel.read = channel.last_message;
                    match channel.guild {
                        Some(guild) => {
                            if let Some(g) = self.model.guild_mut(guild) {
                                g.channels.push(id);
                            }
                        }
                        None => self.model.direct.push(id),
                    }
                }
                self.model.channels.insert(id, channel);
            }
            "CHANNEL_DELETE" => {
                let Some(id) = parse::id(&data["id"]) else {
                    return;
                };
                self.model.channels.remove(&id);
                self.model.direct.retain(|c| *c != id);
                for g in &mut self.model.guilds {
                    g.channels.retain(|c| *c != id);
                }
                if self.channel == Some(id) {
                    self.channel = None;
                }
            }
            "GUILD_CREATE" => {
                let Some(guild) = parse::guild(&data, &mut self.model) else {
                    return;
                };
                match self.model.guilds.iter_mut().find(|g| g.id == guild.id) {
                    Some(old) => *old = guild,
                    None => self.model.guilds.push(guild),
                }
            }
            "GUILD_DELETE" => {
                let Some(id) = parse::id(&data["id"]) else {
                    return;
                };
                if data["unavailable"].as_bool() == Some(true) {
                    return;
                }
                self.model.guilds.retain(|g| g.id != id);
                if self.place == Place::Server(id) {
                    self.go_home();
                }
            }
            "PRESENCE_UPDATE" => {
                let Some(id) = parse::id(&data["user"]["id"]) else {
                    return;
                };
                if let Some(user) = self.model.users.get_mut(&id) {
                    user.set_presence(parse::presence(&data));
                }
            }
            "GUILD_MEMBER_LIST_UPDATE" => parse::member_list(&data, &mut self.model),
            "MESSAGE_REACTION_ADD"
            | "MESSAGE_REACTION_REMOVE"
            | "MESSAGE_REACTION_REMOVE_ALL"
            | "TYPING_START" => self.chat_dispatch(kind, &data),
            "SESSIONS_REPLACE" => {
                if let Some(me) = self.model.users.get_mut(&me)
                    && let Some(first) = data.as_array().and_then(|l| l.first())
                {
                    me.status = Status::parse(first["status"].as_str().unwrap_or("online"));
                }
            }
            _ => {}
        }
    }
}

/// A snowflake for now, used as a message nonce as Discord's clients do.
pub fn snowflake(time: jiff::Timestamp) -> u64 {
    const DISCORD_EPOCH: i64 = 1_420_070_400_000;
    ((time.as_millisecond() - DISCORD_EPOCH) as u64) << 22
}
