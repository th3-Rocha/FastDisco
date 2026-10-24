//! The network side. The interface sends `Command`s and reads `Event`s; all
//! waiting happens here on a tokio runtime, never on the interface thread.

use std::sync::mpsc as std_mpsc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::discord::{self, Me, auth, remote_auth};
use crate::session;

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
}

#[derive(Debug)]
pub enum Event {
    /// No saved token, or Discord no longer accepts it.
    SignedOut,
    SignedIn(Me),
    MfaRequired {
        ticket: String,
    },
    LoginFailed(auth::Error),
    MfaFailed(auth::Error),
    TokenFailed(String),
    ForgotSent,
    ForgotFailed(auth::Error),
    Qr(remote_auth::Event),
    /// An image for `ctx.include_bytes`.
    Image {
        uri: String,
        bytes: Vec<u8>,
    },
}

pub struct Backend {
    commands: mpsc::UnboundedSender<Command>,
    pub events: std_mpsc::Receiver<Event>,
}

impl Backend {
    pub fn start(ctx: egui::Context) -> Self {
        let (commands, receiver) = mpsc::unbounded_channel();
        let (sender, events) = std_mpsc::channel();
        std::thread::Builder::new()
            .name("backend".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("the tokio runtime starts");
                runtime.block_on(Worker::new(ctx, sender).run(receiver));
            })
            .expect("the backend thread starts");
        Self { commands, events }
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

struct Worker {
    ctx: egui::Context,
    events: std_mpsc::Sender<Event>,
    client: reqwest::Client,
    qr: Option<JoinHandle<()>>,
    token: Option<String>,
}

impl Worker {
    fn new(ctx: egui::Context, events: std_mpsc::Sender<Event>) -> Self {
        Self {
            ctx,
            events,
            client: discord::client(),
            qr: None,
            token: None,
        }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
        self.ctx.request_repaint();
    }

    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        let (qr_sender, mut qr_events) = mpsc::unbounded_channel();
        loop {
            tokio::select! {
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
                    Some(token) => match auth::me(&self.client, &token).await {
                        Ok(Some(me)) => self.signed_in(token, me),
                        Ok(None) => {
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
            Command::Logout => {
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
        match auth::me(&self.client, &token).await {
            Ok(Some(me)) => {
                let saved = token.clone();
                if let Ok(Err(error)) =
                    tokio::task::spawn_blocking(move || session::save(&saved)).await
                {
                    log::warn!("{error:#}");
                }
                self.signed_in(token, me);
                Ok(())
            }
            Ok(None) => Err("Token inválido.".into()),
            Err(error) => Err(error.message()),
        }
    }

    fn signed_in(&mut self, token: String, me: Me) {
        if let Some(task) = self.qr.take() {
            task.abort();
        }
        self.token = Some(token);
        if let Some(url) = me.avatar_url() {
            self.fetch_image(url);
        }
        self.emit(Event::SignedIn(me));
    }

    fn fetch_image(&self, url: String) {
        let client = self.client.clone();
        let events = self.events.clone();
        let ctx = self.ctx.clone();
        tokio::spawn(async move {
            let Ok(response) = client.get(&url).send().await else {
                return;
            };
            let Ok(bytes) = response.bytes().await else {
                return;
            };
            let _ = events.send(Event::Image {
                uri: url,
                bytes: bytes.to_vec(),
            });
            ctx.request_repaint();
        });
    }
}

async fn forget() {
    if let Ok(Err(error)) = tokio::task::spawn_blocking(session::forget).await {
        log::warn!("{error:#}");
    }
}
