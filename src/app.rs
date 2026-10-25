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

/// A page of the user settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsPage {
    Account,
    Profiles,
    Appearance,
    Accessibility,
    Notifications,
    Keybinds,
    Linux,
    Advanced,
}

/// A switch in the settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    Animate,
    Embeds,
    Notifications,
    NotificationSound,
    CloseToTray,
    StartAtLogin,
    Stats,
    SpellCheck,
}

/// The profile page's fields while they are edited.
pub struct ProfileForm {
    pub name: String,
    pub bio: String,
    pub pronouns: String,
    pub saving: bool,
    pub message: Option<String>,
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
    OpenSettings(SettingsPage),
    CloseSettings,
    Toggle(Flag),
    SetTheme(crate::settings::Theme),
    SetZoom(f32),
    SaveProfile,
    AskAutostart,
    MarkRead(Id),
    MarkGuildRead(Id),
    /// Alt+Up and Alt+Down: the channel above or below the open one.
    StepChannel(i32),
    /// The server menu, its dialogs and the server settings.
    Admin(crate::admin::AdminAction),
    /// The switcher, the inbox, pins, search, friends and jumping.
    Nav(crate::nav::NavAction),
    /// Forwarding, pins, polls, bots' components and commands.
    Interact(crate::interact::InteractAction),
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
    /// What the user is typing, per channel.
    pub drafts: HashMap<Id, String>,
    pub actions: Vec<Action>,
    pub chat: Chat,
    pub ctx: egui::Context,
    pub settings: crate::settings::Settings,
    notifier: crate::notify::Notifier,
    pub waker: fastframe_shell::Waker,
    pub tray: Option<fastframe_tray::Tray>,
    /// Closing the window keeps the app in the tray.
    pub hide_intent: bool,
    /// The tray asked for the window while there was none.
    pub wants_show: bool,
    pub quit_requested: bool,
    pub window_open: bool,
    pub stats: crate::stats::Stats,
    /// What later launches asked of this copy: "show", "reload-themes".
    pub requests: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    /// The unread count last put in the window title.
    title_count: u32,
    pub settings_page: Option<SettingsPage>,
    pub admin: crate::admin::Admin,
    pub nav: crate::nav::Nav,
    pub interact: crate::interact::Interact,
    /// The spell checker, once its dictionaries are loaded.
    pub speller: Option<std::sync::Arc<crate::spell::Speller>>,
    pub profile_form: Option<ProfileForm>,
    /// Whether FastDisco starts at login; `None` where it cannot.
    pub autostart: Option<bool>,
    pub autostart_asked: bool,
    /// The F12 performance overlay.
    pub show_stats: bool,
    files: fastframe_file::Writer,
    pub themes: fastframe_theme::Catalog<Palette>,
    pictures: RefCell<HashMap<String, Picture>>,
    animations: RefCell<crate::animation::Animations>,
    /// egui's own widgets have been given the palette.
    styled: bool,
    /// Numbers each loaded picture's URI.
    loaded: u64,
}

impl App {
    pub fn new(waker: &fastframe_shell::Waker) -> Self {
        let backend = Backend::start(waker.clone());
        backend.send(Command::Resume);
        let settings = crate::settings::Settings::load();
        if settings.spellcheck {
            backend.send(Command::LoadSpeller);
        }
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
            drafts: HashMap::new(),
            actions: Vec::new(),
            chat: Chat::new(),
            settings,
            notifier: crate::notify::Notifier::start(waker),
            waker: waker.clone(),
            tray: crate::resident::tray(waker),
            hide_intent: false,
            wants_show: false,
            quit_requested: false,
            window_open: false,
            stats: crate::stats::Stats::start(),
            requests: Default::default(),
            title_count: 0,
            settings_page: None,
            admin: crate::admin::Admin::default(),
            nav: crate::nav::Nav::default(),
            interact: crate::interact::Interact::default(),
            speller: None,
            profile_form: None,
            autostart: None,
            autostart_asked: false,
            show_stats: false,
            files: fastframe_file::Writer::new(),
            themes: fastframe_theme::Catalog::default(),
            // Replaced by each window's context in `attach`.
            ctx: egui::Context::default(),
            pictures: RefCell::new(HashMap::new()),
            animations: RefCell::new(crate::animation::Animations::default()),
            loaded: 0,
            styled: false,
        }
    }

    /// Lists the palettes, Omarchy's among them on an Omarchy desktop, and
    /// watches them for changes.
    pub fn start_themes(&mut self) {
        self.themes
            .enable_desktop_themes(fastframe_theme::DesktopThemes {
                slug: "fastdisco",
                omarchy_template: fastframe_theme::omarchy::BASE_TEMPLATE,
                omarchy_previous_templates: &[],
                presets: true,
            });
        self.scan_themes();
    }

    /// A new window: its context, the palette and zoom on it, and the
    /// pictures and players of the last window let go.
    pub fn attach(&mut self, ctx: &egui::Context) {
        self.ctx = ctx.clone();
        self.window_open = true;
        self.pictures.borrow_mut().clear();
        *self.animations.borrow_mut() = crate::animation::Animations::default();
        self.chat.player = None;
        self.styled = false;
        self.title_count = u32::MAX;
        self.apply_palette();
        ctx.set_zoom_factor(self.settings.zoom);
        if let Some(tray) = &mut self.tray {
            tray.attach();
        }
    }

    fn scan_themes(&mut self) {
        let Some(dir) = crate::settings::config_dir() else {
            return;
        };
        let selected = match &self.settings.theme {
            crate::settings::Theme::File(name) => Some(name.clone()),
            _ => None,
        };
        // An Omarchy theme switch wakes the app, window or not.
        let waker = self.waker.clone();
        self.themes.start(
            dir.join("themes"),
            selected,
            &fastframe_theme::Waker::new(move || waker.wake()),
        );
    }

    /// The palette the settings ask for, once the themes are listed.
    pub fn chosen_palette(&self) -> Palette {
        self.palette_for(&self.settings.theme)
    }

    pub fn palette_for(&self, theme: &crate::settings::Theme) -> Palette {
        use crate::settings::Theme;
        match theme {
            Theme::System => self
                .themes
                .follows_omarchy()
                .then(|| self.themes.system_theme().map(|t| t.palette))
                .flatten()
                .unwrap_or_else(Palette::dark),
            Theme::Dark => Palette::dark(),
            Theme::Light => Palette::light(),
            Theme::File(name) => self
                .themes
                .find(name)
                .map_or_else(Palette::dark, |t| t.palette),
        }
    }

    fn apply_palette(&mut self) {
        let palette = self.chosen_palette();
        if palette != self.palette || !self.styled {
            self.palette = palette;
            self.styled = true;
            crate::theme::apply(&self.ctx, &palette);
        }
    }

    /// Follows changed palette files and Omarchy theme switches.
    fn poll_themes(&mut self) {
        if self.themes.needs_reload() {
            self.scan_themes();
        }
        if self.themes.poll() {
            self.apply_palette();
        }
    }

    /// Requests from later launches: show the window, reload the themes.
    pub fn take_requests(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut *self.requests.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn reload_themes(&mut self) {
        self.scan_themes();
    }

    /// "(3) FastDisco" while direct messages or mentions wait, as Discord.
    fn update_title(&mut self, ctx: &egui::Context) {
        let count: u32 = self.model.channels.values().map(|c| c.mentions).sum();
        if count != self.title_count {
            self.title_count = count;
            let title = if count > 0 {
                format!("({count}) FastDisco")
            } else {
                "FastDisco".to_owned()
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
            crate::resident::show_unread(self.tray.as_mut(), count);
        }
    }

    /// A direct message or a mention, away from it: tell the desktop.
    fn notify(&mut self, message: &Message, channel: Id) {
        if !self.settings.notifications || message.author == self.model.me {
            return;
        }
        let Some(c) = self.model.channel(channel) else {
            return;
        };
        let direct = matches!(c.kind, ChannelKind::Direct | ChannelKind::Group);
        if !direct && !message.mentions_me {
            return;
        }
        let focused = self.ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused && self.channel == Some(channel) {
            return;
        }
        let guild = c.guild;
        let author = self.model.name_in(guild, message.author);
        let title = match (c.kind, guild.and_then(|g| self.model.guild(g))) {
            (ChannelKind::Group, _) => format!("{author} ({})", self.model.direct_name(c)),
            (_, Some(g)) => format!("{author} (#{}, {})", c.name, g.name),
            _ => author,
        };
        let model = &self.model;
        let mut body = crate::notify::plain(
            &message.text,
            |id| model.name_in(guild, id),
            |id| {
                model
                    .channel(id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default()
            },
        );
        if body.is_empty() {
            body = if !message.attachments.is_empty() {
                "Enviou um anexo".into()
            } else if !message.stickers.is_empty() {
                "Enviou uma figurinha".into()
            } else {
                "Enviou uma mensagem".into()
            };
        }
        let sound = self.settings.notification_sound;
        self.notifier.show(&title, &body, channel, sound);
    }

    pub fn save_settings(&self) {
        self.settings.save(&self.files);
    }

    pub fn flush_files(&self) {
        self.files.flush(fastframe_file::FLUSH_TIMEOUT);
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
        self.poll_themes();
        if let Some(channel) = self.notifier.clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.settings_page = None;
            self.open_channel(channel);
        }
        self.update_title(ctx);
        // Ctrl + and Ctrl − change the zoom too; keep the setting with it.
        let zoom = ctx.zoom_factor();
        if (zoom - self.settings.zoom).abs() > 0.001 {
            self.settings.zoom = zoom;
            self.save_settings();
        }
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
                Event::Speller(result) => match result {
                    Ok(speller) => self.speller = Some(speller),
                    Err(error) => log::warn!("spell checking off: {error}"),
                },
                Event::Api { tag, result } => match tag {
                    crate::backend::Tag::Admin(tag) => self.api_result(tag, result),
                    crate::backend::Tag::Nav(tag) => self.nav_result(tag, result),
                    crate::backend::Tag::Folders(tag) => self.folders_result(tag, result),
                    crate::backend::Tag::Interact(tag) => self.interact_result(tag, result),
                },
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
                Event::ProfileSaved(result) => {
                    if let Some(form) = &mut self.profile_form {
                        form.saving = false;
                        form.message = Some(match &result {
                            Ok(()) => "Salvo! Suas alterações já aparecem para todos.".into(),
                            Err(error) => error.clone(),
                        });
                        if result.is_ok() {
                            let me = self.model.me;
                            if let Some(user) = self.model.users.get_mut(&me) {
                                user.name = if form.name.trim().is_empty() {
                                    user.username.clone()
                                } else {
                                    form.name.trim().to_owned()
                                };
                            }
                            self.chat.profiles.remove(&me);
                        }
                    }
                }
                Event::Autostart(state) => self.autostart = state,
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
        if let Ok(page) = std::env::var("FASTDISCO_SETTINGS") {
            self.settings_page = Some(match page.as_str() {
                "profiles" => SettingsPage::Profiles,
                "appearance" => SettingsPage::Appearance,
                "accessibility" => SettingsPage::Accessibility,
                "notifications" => SettingsPage::Notifications,
                "keybinds" => SettingsPage::Keybinds,
                "linux" => SettingsPage::Linux,
                "advanced" => SettingsPage::Advanced,
                _ => SettingsPage::Account,
            });
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
                Action::ToggleMembers => {
                    self.settings.show_members = !self.settings.show_members;
                    self.save_settings();
                }
                Action::Send => self.send_draft(),
                Action::Chat(action) => self.apply_chat(action),
                Action::Admin(action) => self.apply_admin(action),
                Action::Nav(action) => self.apply_nav(action),
                Action::Interact(action) => self.apply_interact(action),
                Action::OpenSettings(page) => {
                    self.chat.popout = None;
                    self.settings_page = Some(page);
                }
                Action::CloseSettings => {
                    self.settings_page = None;
                    self.profile_form = None;
                    self.chat.focus_composer = true;
                }
                Action::Toggle(flag) => {
                    let s = &mut self.settings;
                    match flag {
                        Flag::Animate => s.animate = !s.animate,
                        Flag::Embeds => s.embeds = !s.embeds,
                        Flag::Notifications => s.notifications = !s.notifications,
                        Flag::NotificationSound => s.notification_sound = !s.notification_sound,
                        Flag::CloseToTray => s.close_to_tray = !s.close_to_tray,
                        Flag::StartAtLogin => {
                            let on = !self.autostart.unwrap_or(false);
                            self.autostart = Some(on);
                            self.backend.send(Command::SetAutostart(on));
                        }
                        Flag::Stats => self.show_stats = !self.show_stats,
                        Flag::SpellCheck => {
                            s.spellcheck = !s.spellcheck;
                            if s.spellcheck && self.speller.is_none() {
                                self.backend.send(Command::LoadSpeller);
                            }
                        }
                    }
                    self.save_settings();
                }
                Action::SetTheme(theme) => {
                    self.settings.theme = theme;
                    self.save_settings();
                    self.scan_themes();
                    self.apply_palette();
                }
                Action::SetZoom(zoom) => {
                    self.settings.zoom = zoom;
                    self.ctx.set_zoom_factor(zoom);
                    self.save_settings();
                }
                Action::SaveProfile => {
                    if let Some(form) = &mut self.profile_form {
                        form.saving = true;
                        form.message = None;
                        self.backend.send(Command::EditProfile {
                            display_name: form.name.trim().to_owned(),
                            bio: form.bio.clone(),
                            pronouns: form.pronouns.trim().to_owned(),
                        });
                    }
                }
                Action::AskAutostart => {
                    self.autostart_asked = true;
                    self.backend.send(Command::AutostartState);
                }
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
                    self.settings.muted = !self.settings.muted;
                    if !self.settings.muted {
                        self.settings.deafened = false;
                    }
                    self.save_settings();
                }
                Action::ToggleDeafen => {
                    self.settings.deafened = !self.settings.deafened;
                    self.settings.muted = self.settings.deafened;
                    self.save_settings();
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

    /// Home opens where the user last was there: a direct message, or
    /// the friends page, as Discord.
    fn go_home(&mut self) {
        self.place = Place::Home;
        self.channel = None;
        if let Some(channel) = self
            .nav
            .home_channel
            .filter(|c| self.model.channels.contains_key(c))
        {
            self.open_channel(channel);
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
    pub(crate) fn open_channel(&mut self, id: Id) {
        let Some(channel) = self.model.channels.get_mut(&id) else {
            return;
        };
        if !channel.kind.is_textual() {
            return;
        }
        // A channel opened from a mention may be on another server.
        self.place = match channel.guild {
            Some(guild) => Place::Server(guild),
            None => {
                self.nav.home_channel = Some(id);
                Place::Home
            }
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
        self.want_commands(id);
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
        // A jump into the past arrived first: keep showing it.
        if self.nav.detached.contains(&id) {
            return;
        }
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
            channel.loading = false;
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
            "MESSAGE_POLL_VOTE_ADD" => self.poll_vote(&data, true),
            "MESSAGE_POLL_VOTE_REMOVE" => self.poll_vote(&data, false),
            "INTERACTION_SUCCESS" => self.interaction_done(&data, true),
            "INTERACTION_FAILURE" => self.interaction_done(&data, false),
            "INTERACTION_MODAL_CREATE" => {
                self.interaction_done(&data, true);
                self.modal_from(&data);
            }
            "CHANNEL_PINS_UPDATE" => {
                if let Some(channel) = parse::id(&data["channel_id"]) {
                    self.nav.pins.remove(&channel);
                }
            }
            "RELATIONSHIP_ADD" | "RELATIONSHIP_UPDATE" => {
                parse::relationship(&data, &mut self.model);
            }
            "RELATIONSHIP_REMOVE" => {
                if let Some(id) = parse::id(&data["id"]) {
                    self.model.relationships.remove(&id);
                }
            }
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
                self.notify(&message, channel_id);
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
                // Showing older messages after a jump: the new one waits
                // for "Ir para o presente".
                if !channel.requested || self.nav.detached.contains(&channel_id) {
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
                self.model.settle_folders();
            }
            "USER_SETTINGS_UPDATE" if data["guild_folders"].is_array() => {
                self.model.folders = parse::folders(&data["guild_folders"]);
                self.model.settle_folders();
            }
            "GUILD_DELETE" => {
                let Some(id) = parse::id(&data["id"]) else {
                    return;
                };
                if data["unavailable"].as_bool() == Some(true) {
                    return;
                }
                self.model.guilds.retain(|g| g.id != id);
                self.model.settle_folders();
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
