//! The QR login. This app makes a one-off RSA key and connects to Discord's
//! remote auth gateway; Discord answers with a fingerprint for the QR code.
//! The phone scans it, Discord sends the user to show while the phone asks
//! "Log in?", and once approved, a ticket that trades for the token. Every
//! secret arrives encrypted to the key, which never leaves this process.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use futures_util::{SinkExt, StreamExt};
use rsa::pkcs8::EncodePublicKey;
use rsa::{Oaep, RsaPrivateKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use super::auth;

const GATEWAY: &str = "wss://remote-auth-gateway.discord.gg/?v=2";

#[derive(Debug, Clone)]
pub enum Event {
    /// Encode this in the QR code.
    Code(String),
    /// The phone scanned it and is asking the user to confirm.
    Scanned {
        username: String,
        avatar: Option<String>,
    },
    Token(String),
    /// The user said no on the phone, or the code expired: start over.
    Restart,
    Failed(String),
}

/// Runs one QR login until it ends. Events go to `events`; the caller
/// starts another for a fresh code.
pub async fn run(client: reqwest::Client, events: mpsc::UnboundedSender<Event>) {
    if let Err(message) = session(&client, &events).await {
        let _ = events.send(Event::Failed(message));
    }
}

async fn session(
    client: &reqwest::Client,
    events: &mpsc::UnboundedSender<Event>,
) -> Result<(), String> {
    let key = tokio::task::spawn_blocking(|| RsaPrivateKey::new(&mut rand::thread_rng(), 2048))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let public = key
        .to_public_key()
        .to_public_key_der()
        .map_err(|e| e.to_string())?;

    let mut request = GATEWAY.into_client_request().map_err(|e| e.to_string())?;
    request.headers_mut().insert(
        "Origin",
        "https://discord.com".parse().expect("static header"),
    );
    let (socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(|_| unreachable())?;
    let (mut sink, mut stream) = socket.split();

    let decrypt = |text: &str| -> Result<Vec<u8>, String> {
        let bytes = STANDARD.decode(text).map_err(|e| e.to_string())?;
        key.decrypt(Oaep::new::<Sha256>(), &bytes)
            .map_err(|e| e.to_string())
    };

    let mut heartbeat = tokio::time::interval(Duration::from_secs(41));
    heartbeat.reset();
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                send(&mut sink, json!({ "op": "heartbeat" })).await?;
            }
            message = stream.next() => {
                let text = match message {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_))) | None => {
                        // Discord closes the socket when the code times out.
                        let _ = events.send(Event::Restart);
                        return Ok(());
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(_)) => return Err(unreachable()),
                };
                let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                match value["op"].as_str().unwrap_or_default() {
                    "hello" => {
                        if let Some(ms) = value["heartbeat_interval"].as_u64() {
                            heartbeat = tokio::time::interval(Duration::from_millis(ms));
                            heartbeat.reset();
                        }
                        let encoded = STANDARD.encode(public.as_bytes());
                        send(&mut sink, json!({ "op": "init", "encoded_public_key": encoded })).await?;
                    }
                    "nonce_proof" => {
                        let nonce = decrypt(value["encrypted_nonce"].as_str().unwrap_or_default())?;
                        let proof = URL_SAFE_NO_PAD.encode(Sha256::digest(&nonce));
                        send(&mut sink, json!({ "op": "nonce_proof", "proof": proof })).await?;
                    }
                    "pending_remote_init" => {
                        let fingerprint = value["fingerprint"].as_str().unwrap_or_default();
                        let _ = events.send(Event::Code(format!("https://discord.com/ra/{fingerprint}")));
                    }
                    "pending_ticket" => {
                        let payload = decrypt(value["encrypted_user_payload"].as_str().unwrap_or_default())?;
                        let payload = String::from_utf8_lossy(&payload);
                        // id:discriminator:avatar:username
                        let mut parts = payload.splitn(4, ':');
                        let id = parts.next().unwrap_or_default().to_owned();
                        let _discriminator = parts.next();
                        let avatar = parts.next().and_then(|hash| super::avatar_url(&id, hash));
                        let username = parts.next().unwrap_or_default().to_owned();
                        let _ = events.send(Event::Scanned { username, avatar });
                    }
                    "pending_login" => {
                        let ticket = value["ticket"].as_str().unwrap_or_default();
                        let encrypted = auth::remote_login(client, ticket).await.map_err(|e| e.message())?;
                        let token = decrypt(&encrypted)?;
                        let _ = events.send(Event::Token(String::from_utf8_lossy(&token).into_owned()));
                        return Ok(());
                    }
                    // Older protocol versions send the token here.
                    "finish" => {
                        let token = decrypt(value["encrypted_token"].as_str().unwrap_or_default())?;
                        let _ = events.send(Event::Token(String::from_utf8_lossy(&token).into_owned()));
                        return Ok(());
                    }
                    "cancel" => {
                        let _ = events.send(Event::Restart);
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn send<S>(sink: &mut S, value: Value) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
{
    sink.send(Message::Text(value.to_string().into()))
        .await
        .map_err(|_| unreachable())
}

fn unreachable() -> String {
    "Não foi possível falar com o Discord.".into()
}
