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
    /// The newest messages of a channel.
    LoadMessages(u64),
    Send {
        channel: u64,
        content: String,
        nonce: String,
    },
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
    },
    SendFailed {
        channel: u64,
        nonce: String,
    },
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
    ctx: egui::Context,
    events: std_mpsc::Sender<Event>,
    client: reqwest::Client,
    qr: Option<JoinHandle<()>>,
    token: Option<String>,
    gateway: Option<(JoinHandle<()>, mpsc::UnboundedSender<Outgoing>)>,
    gateway_events: mpsc::UnboundedSender<Incoming>,
}

impl Worker {
    fn new(
        ctx: egui::Context,
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
        self.ctx.request_repaint();
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
            Command::LoadMessages(channel) => {
                let Some(token) = self.token.clone() else {
                    return;
                };
                let (client, events, ctx) =
                    (self.client.clone(), self.events.clone(), self.ctx.clone());
                tokio::spawn(async move {
                    match rest::messages(&client, &token, channel).await {
                        Ok(list) => {
                            let _ = events.send(Event::Messages { channel, list });
                            ctx.request_repaint();
                        }
                        Err(error) => log::warn!("history of {channel}: {error}"),
                    }
                });
            }
            Command::Send {
                channel,
                content,
                nonce,
            } => {
                let Some(token) = self.token.clone() else {
                    return;
                };
                let (client, events, ctx) =
                    (self.client.clone(), self.events.clone(), self.ctx.clone());
                tokio::spawn(async move {
                    if let Err(error) = rest::send(&client, &token, channel, &content, &nonce).await
                    {
                        log::warn!("send to {channel}: {error}");
                        let _ = events.send(Event::SendFailed { channel, nonce });
                        ctx.request_repaint();
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
