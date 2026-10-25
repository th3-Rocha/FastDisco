//! The network side. The interface sends `Command`s and reads `Event`s; all
//! waiting happens here on a tokio runtime, never on the interface thread.

use std::sync::mpsc as std_mpsc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use serde_json::Value;

use crate::discord::gateway::{self, Incoming, Outgoing};
use crate::discord::{self, auth, parse, remote_auth, rest};
use crate::model::Model;
use crate::session;

/// Whom an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    Admin(crate::admin::Tag),
    Nav(crate::nav::Tag),
    Folders(crate::folders::Tag),
}

#[derive(Debug)]
pub enum Command {
    /// At start: sign in with the saved token, if there is one.
    Resume,
    Login {
        login: String,
        password: String,
    },
    Mfa {
        ticket: String,
        code: String,
    },
    Token(String),
    Forgot {
        login: String,
    },
    StartQr,
    Logout,
    /// The newest messages of a channel, or those before a message.
    LoadMessages {
        channel: u64,
        before: Option<u64>,
    },
    Send {
        channel: u64,
        message: rest::Outgoing,
    },
    /// A recorded voice message: mono samples at Opus' rate, encoded here,
    /// away from the interface.
    SendVoice {
        channel: u64,
        nonce: String,
        reply: Option<u64>,
        samples: Vec<f32>,
        seconds: f32,
        waveform: Vec<u8>,
    },
    Edit {
        channel: u64,
        message: u64,
        content: String,
    },
    Delete {
        channel: u64,
        message: u64,
    },
    React {
        channel: u64,
        message: u64,
        emoji: String,
        add: bool,
    },
    Typing(u64),
    /// Trending GIFs, or a search.
    Gifs(Option<String>),
    EditProfile {
        display_name: String,
        bio: String,
        pronouns: String,
    },
    /// Starts FastDisco at login, or stops.
    SetAutostart(bool),
    AutostartState,
    /// Any API call; the reply comes back tagged with `tag`.
    Api {
        tag: Tag,
        method: reqwest::Method,
        path: String,
        body: Option<Value>,
    },
    /// A user's profile and the note about them.
    Profile {
        user: u64,
        guild: Option<u64>,
    },
    SetNote {
        user: u64,
        note: String,
    },
    /// Opens the direct message with a user.
    OpenDirect(u64),
    ApplicationIcon(u64),
    /// The trending searches for the GIF picker's first screen.
    GifHome,
    /// The favourite GIFs from the user's settings.
    LoadFavorites,
    /// Adds or removes a favourite GIF in the user's settings.
    Favorite {
        gif: discord::frecency::Favorite,
        add: bool,
    },
    /// The system's file picker, for files to send.
    PickFiles,
    Ack {
        channel: u64,
        message: u64,
    },
    /// The member list of a server, seen from a channel.
    Subscribe {
        guild: u64,
        channel: u64,
    },
    FetchImage(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Connecting,
    Connected,
}

#[derive(Debug)]
pub enum Event {
    /// No saved token, or Discord no longer accepts it.
    SignedOut,
    SignedIn,
    MfaRequired {
        ticket: String,
    },
    LoginFailed(auth::Error),
    MfaFailed(auth::Error),
    TokenFailed(String),
    ForgotSent,
    ForgotFailed(auth::Error),
    Qr(remote_auth::Event),
    Connection(Connection),
    /// READY, already turned into a model off the interface thread.
    Ready(Box<Model>),
    /// Any other gateway event, applied by the interface.
    Dispatch {
        kind: String,
        data: Value,
    },
    Messages {
        channel: u64,
        list: Vec<Value>,
        older: bool,
    },
    SendFailed {
        channel: u64,
        nonce: String,
        reason: String,
    },
    GifHome(rest::GifHome),
    Api {
        tag: Tag,
        result: Result<Value, String>,
    },
    ProfileSaved(Result<(), String>),
    Autostart(Option<bool>),
    Profile {
        user: u64,
        profile: Box<crate::model::Profile>,
    },
    ProfileFailed(u64),
    DirectOpened(Value),
    ApplicationIcon {
        application: u64,
        url: Option<String>,
    },
    Favorites(Vec<discord::frecency::Favorite>),
    Gifs {
        query: Option<String>,
        list: Vec<rest::Gif>,
    },
    FilesPicked(Vec<std::path::PathBuf>),
    /// A downloaded picture, `ext` from its content type.
    Image {
        url: String,
        bytes: Vec<u8>,
        ext: &'static str,
    },
    ImageFailed(String),
}

pub struct Backend {
    commands: mpsc::UnboundedSender<Command>,
    pub events: std_mpsc::Receiver<Event>,
}

impl Backend {
    pub fn start(ctx: fastframe_shell::Waker) -> Self {
        let (commands, receiver) = mpsc::unbounded_channel();
        let (sender, events) = std_mpsc::channel();
        std::thread::Builder::new()
            .name("backend".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("the tokio runtime starts");
                let (gateway_sender, gateway_events) = mpsc::unbounded_channel();
                runtime.block_on(
                    Worker::new(ctx, sender, gateway_sender).run(receiver, gateway_events),
                );
            })
            .expect("the backend thread starts");
        Self { commands, events }
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

struct Worker {
    ctx: fastframe_shell::Waker,
    events: std_mpsc::Sender<Event>,
    client: reqwest::Client,
    qr: Option<JoinHandle<()>>,
    token: Option<String>,
    gateway: Option<(JoinHandle<()>, mpsc::UnboundedSender<Outgoing>)>,
    gateway_events: mpsc::UnboundedSender<Incoming>,
}

impl Worker {
    fn new(
        ctx: fastframe_shell::Waker,
        events: std_mpsc::Sender<Event>,
        gateway_events: mpsc::UnboundedSender<Incoming>,
    ) -> Self {
        Self {
            ctx,
            events,
            client: discord::client(),
            qr: None,
            token: None,
            gateway: None,
            gateway_events,
        }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        self.ctx.wake();
    }

    async fn run(
        mut self,
        mut commands: mpsc::UnboundedReceiver<Command>,
        mut gateway_events: mpsc::UnboundedReceiver<Incoming>,
    ) {
        let (qr_sender, mut qr_events) = mpsc::unbounded_channel();
        loop {
            tokio::select! {
                Some(incoming) = gateway_events.recv() => self.gateway_event(incoming).await,
                command = commands.recv() => {
                    let Some(command) = command else { return };
                    self.handle(command, &qr_sender).await;
                }
                Some(event) = qr_events.recv() => {
                    match event {
                        remote_auth::Event::Token(token) => {
                            self.qr = None;
                            if let Err(message) = self.sign_in(token).await {
                                self.emit(Event::Qr(remote_auth::Event::Failed(message)));
                            }
                        }
                        remote_auth::Event::Scanned { ref avatar, .. } => {
                            if let Some(url) = avatar.clone() {
                                self.fetch_image(url);
                            }
                            self.emit(Event::Qr(event));
                        }
                        event => self.emit(Event::Qr(event)),
                    }
                }
            }
        }
    }

    async fn handle(&mut self, command: Command, qr: &mpsc::UnboundedSender<remote_auth::Event>) {
        match command {
            Command::Resume => {
                let token = tokio::task::spawn_blocking(session::load)
                    .await
                    .ok()
                    .and_then(|r| r.inspect_err(|e| log::warn!("{e:#}")).ok().flatten());
                match token {
                    Some(token) => match auth::check(&self.client, &token).await {
                        Ok(true) => self.signed_in(token),
                        Ok(false) => {
                            forget().await;
                            self.emit(Event::SignedOut);
                        }
                        // Offline: keep the token, but show the login for now.
                        Err(error) => {
                            log::warn!("could not check the saved token: {}", error.message());
                            self.emit(Event::SignedOut);
                        }
                    },
                    None => self.emit(Event::SignedOut),
                }
            }
            Command::Login { login, password } => {
                match auth::login(&self.client, &login, &password).await {
                    Ok(auth::Outcome::Token(token)) => {
                        if let Err(message) = self.sign_in(token).await {
                            self.emit(Event::LoginFailed(auth::Error::Other(message)));
                        }
                    }
                    Ok(auth::Outcome::Mfa { ticket }) => self.emit(Event::MfaRequired { ticket }),
                    Err(error) => self.emit(Event::LoginFailed(error)),
                }
            }
            Command::Mfa { ticket, code } => match auth::mfa(&self.client, &ticket, &code).await {
                Ok(token) => {
                    if let Err(message) = self.sign_in(token).await {
                        self.emit(Event::MfaFailed(auth::Error::Other(message)));
                    }
                }
                Err(error) => self.emit(Event::MfaFailed(error)),
            },
            Command::Token(token) => {
                let token = token.trim().trim_matches('"').to_owned();
                if let Err(message) = self.sign_in(token).await {
                    self.emit(Event::TokenFailed(message));
                }
            }
            Command::Forgot { login } => match auth::forgot(&self.client, &login).await {
                Ok(()) => self.emit(Event::ForgotSent),
                Err(error) => self.emit(Event::ForgotFailed(error)),
            },
            Command::StartQr => {
                if let Some(task) = self.qr.take() {
                    task.abort();
                }
                self.qr = Some(tokio::spawn(remote_auth::run(
                    self.client.clone(),
                    qr.clone(),
                )));
            }
            Command::LoadMessages { channel, before } => {
                self.spawn(move |client, token, events| async move {
                    match rest::messages(&client, &token, channel, before).await {
                        Ok(list) => {
                            let _ = events.send(Event::Messages {
                                channel,
                                list,
                                older: before.is_some(),
                            });
                        }
                        Err(error) => log::warn!("history of {channel}: {error}"),
                    }
                });
            }
            Command::SendVoice {
                channel,
                nonce,
                reply,
                samples,
                seconds,
                waveform,
            } => {
                self.spawn(move |client, token, events| async move {
                    let encoded =
                        tokio::task::spawn_blocking(move || crate::voice::encode(&samples)).await;
                    let result = match encoded {
                        Ok(Ok(ogg)) => {
                            let voice = rest::Voice {
                                nonce: nonce.clone(),
                                reply,
                                ogg,
                                seconds,
                                waveform,
                            };
                            rest::send_voice(&client, &token, channel, voice).await
                        }
                        Ok(Err(reason)) => Err(reason),
                        Err(error) => Err(error.to_string()),
                    };
                    if let Err(reason) = result {
                        log::warn!("voice message to {channel}: {reason}");
                        let _ = events.send(Event::SendFailed {
                            channel,
                            nonce,
                            reason,
                        });
                    }
                });
            }
            Command::Send { channel, message } => {
                self.spawn(move |client, token, events| async move {
                    let nonce = message.nonce.clone();
                    if let Err(reason) = rest::send(&client, &token, channel, message).await {
                        log::warn!("send to {channel}: {reason}");
                        let _ = events.send(Event::SendFailed {
                            channel,
                            nonce,
                            reason,
                        });
                    }
                });
            }
            Command::Edit {
                channel,
                message,
                content,
            } => {
                self.spawn(move |client, token, _| async move {
                    if let Err(error) =
                        rest::edit(&client, &token, channel, message, &content).await
                    {
                        log::warn!("edit {message}: {error}");
                    }
                });
            }
            Command::Delete { channel, message } => {
                self.spawn(move |client, token, _| async move {
                    if let Err(error) = rest::delete(&client, &token, channel, message).await {
                        log::warn!("delete {message}: {error}");
                    }
                });
            }
            Command::React {
                channel,
                message,
                emoji,
                add,
            } => {
                self.spawn(move |client, token, _| async move {
                    if let Err(error) =
                        rest::react(&client, &token, channel, message, &emoji, add).await
                    {
                        log::warn!("reaction on {message}: {error}");
                    }
                });
            }
            Command::Typing(channel) => {
                self.spawn(move |client, token, _| async move {
                    rest::typing(&client, &token, channel).await
                });
            }
            Command::EditProfile {
                display_name,
                bio,
                pronouns,
            } => {
                self.spawn(move |client, token, events| async move {
                    let result =
                        rest::edit_profile(&client, &token, &display_name, &bio, &pronouns).await;
                    let _ = events.send(Event::ProfileSaved(result));
                });
            }
            Command::SetAutostart(on) => {
                let (events, ctx) = (self.events.clone(), self.ctx.clone());
                tokio::task::spawn_blocking(move || {
                    let autostart = fastframe_autostart::Autostart::new("fastdisco", "FastDisco");
                    let start = on.then_some(fastframe_autostart::Start::Window);
                    if let Err(error) = autostart.set(start) {
                        log::warn!("autostart: {error}");
                    }
                    let _ = events.send(Event::Autostart(autostart_on(&autostart)));
                    ctx.wake();
                });
            }
            Command::AutostartState => {
                let (events, ctx) = (self.events.clone(), self.ctx.clone());
                tokio::task::spawn_blocking(move || {
                    let autostart = fastframe_autostart::Autostart::new("fastdisco", "FastDisco");
                    let _ = events.send(Event::Autostart(autostart_on(&autostart)));
                    ctx.wake();
                });
            }
            Command::Api {
                tag,
                method,
                path,
                body,
            } => {
                self.spawn(move |client, token, events| async move {
                    let result = rest::call(&client, &token, method, &path, body).await;
                    let _ = events.send(Event::Api { tag, result });
                });
            }
            Command::Profile { user, guild } => {
                self.spawn(move |client, token, events| async move {
                    let (profile, note) = tokio::join!(
                        rest::profile(&client, &token, user, guild),
                        rest::note(&client, &token, user)
                    );
                    match profile {
                        Ok(value) => {
                            let mut profile = parse::profile(&value);
                            profile.note = note;
                            let _ = events.send(Event::Profile {
                                user,
                                profile: Box::new(profile),
                            });
                        }
                        Err(error) => {
                            log::warn!("profile of {user}: {error}");
                            let _ = events.send(Event::ProfileFailed(user));
                        }
                    }
                });
            }
            Command::SetNote { user, note } => {
                self.spawn(move |client, token, _| async move {
                    if let Err(error) = rest::set_note(&client, &token, user, &note).await {
                        log::warn!("note on {user}: {error}");
                    }
                });
            }
            Command::OpenDirect(user) => {
                self.spawn(move |client, token, events| async move {
                    match rest::open_direct(&client, &token, user).await {
                        Ok(channel) => {
                            let _ = events.send(Event::DirectOpened(channel));
                        }
                        Err(error) => log::warn!("direct message with {user}: {error}"),
                    }
                });
            }
            Command::ApplicationIcon(application) => {
                self.spawn(move |client, token, events| async move {
                    let url = rest::application_icon(&client, &token, application).await;
                    let _ = events.send(Event::ApplicationIcon { application, url });
                });
            }
            Command::GifHome => {
                self.spawn(move |client, token, events| async move {
                    match rest::gif_home(&client, &token).await {
                        Ok(home) => {
                            let _ = events.send(Event::GifHome(home));
                        }
                        Err(error) => log::warn!("gif categories: {error}"),
                    }
                });
            }
            Command::LoadFavorites => {
                self.spawn(move |client, token, events| async move {
                    match discord::frecency::load(&client, &token).await {
                        Ok(settings) => {
                            let _ =
                                events.send(Event::Favorites(discord::frecency::parse(&settings)));
                        }
                        Err(error) => log::warn!("favourite gifs: {error}"),
                    }
                });
            }
            Command::Favorite { gif, add } => {
                self.spawn(move |client, token, events| async move {
                    // Read the settings fresh, so a change made elsewhere
                    // in the meantime is kept.
                    let result = async {
                        let settings = discord::frecency::load(&client, &token).await?;
                        let changed = discord::frecency::change(&settings, &gif, add)
                            .ok_or_else(|| "unreadable settings".to_owned())?;
                        discord::frecency::save(&client, &token, &changed).await?;
                        Ok::<_, String>(discord::frecency::parse(&changed))
                    }
                    .await;
                    match result {
                        Ok(list) => {
                            let _ = events.send(Event::Favorites(list));
                        }
                        Err(error) => log::warn!("favourite gif: {error}"),
                    }
                });
            }
            Command::Gifs(query) => {
                self.spawn(move |client, token, events| async move {
                    match rest::gifs(&client, &token, query.as_deref()).await {
                        Ok(list) => {
                            let _ = events.send(Event::Gifs { query, list });
                        }
                        Err(error) => log::warn!("gifs: {error}"),
                    }
                });
            }
            Command::PickFiles => {
                let (events, ctx) = (self.events.clone(), self.ctx.clone());
                tokio::spawn(async move {
                    let picked = rfd::AsyncFileDialog::new()
                        .set_title("Enviar arquivos")
                        .pick_files()
                        .await;
                    let paths: Vec<_> = picked
                        .into_iter()
                        .flatten()
                        .map(|f| f.path().to_owned())
                        .collect();
                    if !paths.is_empty() {
                        let _ = events.send(Event::FilesPicked(paths));
                        ctx.wake();
                    }
                });
            }
            Command::Ack { channel, message } => {
                let Some(token) = self.token.clone() else {
                    return;
                };
                let client = self.client.clone();
                tokio::spawn(async move { rest::ack(&client, &token, channel, message).await });
            }
            Command::Subscribe { guild, channel } => {
                if let Some((_, outgoing)) = &self.gateway {
                    let _ = outgoing.send(Outgoing::Subscribe { guild, channel });
                }
            }
            Command::FetchImage(url) => self.fetch_image(url),
            Command::Logout => {
                if let Some((task, _)) = self.gateway.take() {
                    task.abort();
                }
                if let Some(token) = self.token.take() {
                    auth::logout(&self.client, &token).await;
                }
                forget().await;
                self.emit(Event::SignedOut);
            }
        }
    }

    /// Checks a token, keeps it in the keyring and opens the app.
    async fn sign_in(&mut self, token: String) -> Result<(), String> {
        match auth::check(&self.client, &token).await {
            Ok(true) => {
                let saved = token.clone();
                if let Ok(Err(error)) =
                    tokio::task::spawn_blocking(move || session::save(&saved)).await
                {
                    log::warn!("{error:#}");
                }
                self.signed_in(token);
                Ok(())
            }
            Ok(false) => Err("Token inválido.".into()),
            Err(error) => Err(error.message()),
        }
    }

    fn signed_in(&mut self, token: String) {
        if let Some(task) = self.qr.take() {
            task.abort();
        }
        if let Some((task, _)) = self.gateway.take() {
            task.abort();
        }
        let (outgoing, receiver) = mpsc::unbounded_channel();
        let task = tokio::spawn(gateway::run(
            token.clone(),
            receiver,
            self.gateway_events.clone(),
        ));
        self.gateway = Some((task, outgoing));
        self.token = Some(token);
        self.emit(Event::SignedIn);
    }

    async fn gateway_event(&mut self, incoming: Incoming) {
        match incoming {
            Incoming::Connecting => self.emit(Event::Connection(Connection::Connecting)),
            Incoming::Unauthorized => {
                self.gateway = None;
                self.token = None;
                forget().await;
                self.emit(Event::SignedOut);
            }
            Incoming::Dispatch { kind, data } if kind == "READY" => {
                let model = tokio::task::spawn_blocking(move || parse::ready(&data)).await;
                if let Ok(model) = model {
                    log::info!(
                        "READY: {} servers, {} channels, {} direct messages",
                        model.guilds.len(),
                        model.channels.len(),
                        model.direct.len()
                    );
                    self.emit(Event::Ready(Box::new(model)));
                    self.emit(Event::Connection(Connection::Connected));
                }
            }
            Incoming::Dispatch { kind, data } => {
                if kind == "RESUMED" {
                    self.emit(Event::Connection(Connection::Connected));
                }
                self.emit(Event::Dispatch { kind, data });
            }
        }
    }

    /// Runs a request with the session's token; whatever it sends
    /// repaints the window.
    fn spawn<F, Fut>(&self, task: F)
    where
        F: FnOnce(reqwest::Client, String, Repaint) -> Fut,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let Some(token) = self.token.clone() else {
            return;
        };
        let events = Repaint {
            events: self.events.clone(),
            ctx: self.ctx.clone(),
        };
        tokio::spawn(task(self.client.clone(), token, events));
    }

    fn fetch_image(&self, url: String) {
        let client = self.client.clone();
        let events = self.events.clone();
        let ctx = self.ctx.clone();
        tokio::spawn(async move {
            let event = match download(&client, &url).await {
                Some((bytes, ext)) => Event::Image { url, bytes, ext },
                None => Event::ImageFailed(url),
            };
            let _ = events.send(event);
            ctx.wake();
        });
    }
}

/// An event sender that wakes the window.
#[derive(Clone)]
pub struct Repaint {
    events: std_mpsc::Sender<Event>,
    ctx: fastframe_shell::Waker,
}

impl Repaint {
    fn send(&self, event: Event) -> Result<(), std_mpsc::SendError<Event>> {
        let result = self.events.send(event);
        self.ctx.wake();
        result
    }
}

/// Pictures larger than this are not shown inline.
const IMAGE_LIMIT: usize = 40 << 20;

async fn download(client: &reqwest::Client, url: &str) -> Option<(Vec<u8>, &'static str)> {
    let response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    let kind = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    if response
        .content_length()
        .is_some_and(|l| l as usize > IMAGE_LIMIT)
    {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    let ext = match kind.as_str() {
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/jpeg" => "jpg",
        "image/svg+xml" => "svg",
        _ if bytes.starts_with(b"GIF8") => "gif",
        _ if bytes.starts_with(&[0xff, 0xd8]) => "jpg",
        _ if bytes.len() > 12 && &bytes[8..12] == b"WEBP" => "webp",
        _ => "png",
    };
    Some((bytes.to_vec(), ext))
}

async fn forget() {
    if let Ok(Err(error)) = tokio::task::spawn_blocking(session::forget).await {
        log::warn!("{error:#}");
    }
}

/// Whether FastDisco starts at login; `None` where the desktop cannot.
fn autostart_on(autostart: &fastframe_autostart::Autostart) -> Option<bool> {
    match autostart.state() {
        fastframe_autostart::State::Unsupported => None,
        fastframe_autostart::State::Off => Some(false),
        _ => Some(true),
    }
}
