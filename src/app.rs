//! Application state. Views draw from it and push `Action`s; `App::apply`
//! runs them after the frame is drawn, so no view mutates what another is
//! still reading.

use std::collections::HashSet;

use crate::backend::{Backend, Command, Event};
use crate::login::{Login, Qr, View};
use crate::model::{ChannelId, Message, Model, ServerId};
use crate::theme::Palette;

/// What fills the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Checking the saved token.
    Connecting,
    Login,
    Main,
}

/// Where the user is: the home screen with direct messages, or a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Home,
    Server(ServerId),
}

#[derive(Clone, Debug)]
pub enum Action {
    GoHome,
    OpenServer(ServerId),
    OpenChannel(ChannelId),
    ToggleCategory(ServerId, usize),
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
}

pub struct App {
    pub screen: Screen,
    pub login: Login,
    pub confirm_logout: bool,
    backend: Backend,
    pub model: Model,
    pub palette: Palette,
    pub place: Place,
    /// The open channel, remembered per server.
    pub channel: Option<ChannelId>,
    last_channel: Vec<Option<ChannelId>>,
    pub collapsed: HashSet<(ServerId, usize)>,
    pub show_members: bool,
    pub muted: bool,
    pub deafened: bool,
    /// What the user is typing, per channel.
    pub drafts: Vec<String>,
    pub actions: Vec<Action>,
}

impl App {
    pub fn new(ctx: &egui::Context, model: Model) -> Self {
        let channels = model.channels.len();
        let servers = model.servers.len();
        let backend = Backend::start(ctx.clone());
        backend.send(Command::Resume);
        let mut app = Self {
            screen: Screen::Connecting,
            login: Login::default(),
            confirm_logout: false,
            backend,
            model,
            palette: Palette::dark(),
            place: Place::Home,
            channel: None,
            last_channel: vec![None; servers],
            collapsed: HashSet::new(),
            show_members: true,
            muted: false,
            deafened: false,
            drafts: vec![String::new(); channels],
            actions: Vec::new(),
        };
        let (server, channel) = app.model.start;
        app.last_channel[server] = Some(channel);
        app.open_server(server);
        app
    }

    pub fn server(&self) -> Option<ServerId> {
        match self.place {
            Place::Server(id) => Some(id),
            Place::Home => None,
        }
    }

    /// Takes what the backend has sent since the last frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.backend.events.try_recv() {
            match event {
                Event::SignedOut => {
                    self.screen = Screen::Login;
                    self.login = Login::default();
                    self.backend.send(Command::StartQr);
                }
                Event::SignedIn(me) => {
                    let user = &mut self.model.users[self.model.me];
                    user.name = me.display_name().to_owned();
                    user.username = me.username.clone();
                    user.picture = me.avatar_url();
                    self.login = Login::default();
                    self.screen = Screen::Main;
                }
                Event::Image { uri, bytes } => ctx.include_bytes(uri, bytes),
                event => {
                    if self.login.handle(ctx, event) {
                        self.backend.send(Command::StartQr);
                    }
                }
            }
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
                Action::GoHome => {
                    self.place = Place::Home;
                    self.channel = self.model.direct.first().map(|dm| dm.channel);
                    self.mark_read();
                }
                Action::OpenServer(id) => self.open_server(id),
                Action::OpenChannel(id) => {
                    self.channel = Some(id);
                    if let Some(server) = self.server() {
                        self.last_channel[server] = Some(id);
                    }
                    self.mark_read();
                }
                Action::ToggleCategory(server, category) => {
                    if !self.collapsed.remove(&(server, category)) {
                        self.collapsed.insert((server, category));
                    }
                }
                Action::ToggleMembers => self.show_members = !self.show_members,
                Action::Send => self.send(),
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

    fn open_server(&mut self, id: ServerId) {
        self.place = Place::Server(id);
        self.channel = self.last_channel[id].or_else(|| self.model.first_text_channel(id));
        self.last_channel[id] = self.channel;
        self.mark_read();
    }

    /// Opening a channel reads it, and a server with nothing left unread
    /// loses its dot.
    fn mark_read(&mut self) {
        let Some(id) = self.channel else { return };
        let channel = &mut self.model.channels[id];
        channel.unread = false;
        channel.mentions = 0;
        if let Some(dm) = self.model.direct.iter_mut().find(|dm| dm.channel == id) {
            dm.unread = 0;
        }
        if let Some(server) = self.server() {
            let channels = &self.model.channels;
            let server = &mut self.model.servers[server];
            let all = server
                .loose
                .iter()
                .chain(server.categories.iter().flat_map(|c| c.channels.iter()));
            let (unread, mentions) = all.fold((false, 0), |(u, m), &c| {
                (u || channels[c].unread, m + channels[c].mentions)
            });
            server.unread = unread;
            server.mentions = mentions;
        }
    }

    /// The message shows at once; the gateway will confirm it later.
    fn send(&mut self) {
        let Some(id) = self.channel else { return };
        let text = self.drafts[id].trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.drafts[id].clear();
        let messages = &mut self.model.channels[id].messages;
        let minute = messages.last().map_or(0, |m| m.minute.max(0)).max(616);
        messages.push(Message {
            author: self.model.me,
            minute,
            text,
            mentions_me: false,
        });
    }
}
