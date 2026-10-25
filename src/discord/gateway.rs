//! The gateway: one WebSocket that sends everything live. It identifies
//! with the token, keeps a heartbeat, resumes after a drop and starts over
//! when Discord asks. Dispatches go to the backend as they arrive.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

const GATEWAY: &str = "wss://gateway.discord.gg";
const QUERY: &str = "/?v=9&encoding=json";

/// Close codes after which reconnecting cannot help.
const FATAL: [u16; 5] = [4004, 4010, 4011, 4013, 4014];

#[derive(Debug)]
pub enum Incoming {
    Connecting,
    Dispatch {
        kind: String,
        data: Value,
    },
    /// Discord refused the token: log in again.
    Unauthorized,
}

#[derive(Debug)]
pub enum Outgoing {
    /// Ask for a server's member list, as seen from a channel.
    Subscribe { guild: u64, channel: u64 },
    /// Join a voice channel (or a direct message call when `guild` is
    /// none), change mute and deafen, or leave with no channel.
    Voice {
        guild: Option<u64>,
        channel: Option<u64>,
        mute: bool,
        deaf: bool,
    },
}

struct Session {
    id: String,
    sequence: Option<u64>,
    resume_url: String,
}

pub async fn run(
    token: String,
    mut outgoing: mpsc::UnboundedReceiver<Outgoing>,
    incoming: mpsc::UnboundedSender<Incoming>,
) {
    let mut session: Option<Session> = None;
    let mut backoff = 1;
    loop {
        let _ = incoming.send(Incoming::Connecting);
        let url = session
            .as_ref()
            .map_or(GATEWAY, |s| s.resume_url.as_str())
            .to_owned()
            + QUERY;
        match connect(&url, &token, &mut session, &mut outgoing, &incoming).await {
            Ok(End::Closed(code)) if FATAL.contains(&code) => {
                log::warn!("gateway closed with {code}");
                if code == 4004 {
                    let _ = incoming.send(Incoming::Unauthorized);
                }
                return;
            }
            Ok(End::Closed(code)) => {
                log::info!("gateway closed with {code}, reconnecting");
                backoff = 1;
            }
            Ok(End::Reconnect) => backoff = 1,
            Ok(End::Stopped) => return,
            Err(error) => {
                log::warn!("gateway: {error}");
                backoff = (backoff * 2).min(60);
            }
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
    }
}

enum End {
    Closed(u16),
    Reconnect,
    Stopped,
}

async fn connect(
    url: &str,
    token: &str,
    session: &mut Option<Session>,
    outgoing: &mut mpsc::UnboundedReceiver<Outgoing>,
    incoming: &mpsc::UnboundedSender<Incoming>,
) -> Result<End, String> {
    // READY for an account in many servers runs to tens of megabytes.
    let config = WebSocketConfig::default()
        .max_message_size(Some(256 << 20))
        .max_frame_size(Some(64 << 20));
    let (socket, _) = tokio_tungstenite::connect_async_with_config(url, Some(config), false)
        .await
        .map_err(|e| e.to_string())?;
    let (mut sink, mut stream) = socket.split();

    let mut heartbeat = tokio::time::interval(Duration::from_secs(41));
    let mut acknowledged = true;
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if !acknowledged {
                    // A zombie connection: drop it and resume.
                    return Ok(End::Reconnect);
                }
                acknowledged = false;
                let sequence = session.as_ref().and_then(|s| s.sequence);
                send(&mut sink, json!({ "op": 1, "d": sequence })).await?;
            }
            request = outgoing.recv() => {
                match request {
                    Some(Outgoing::Subscribe { guild, channel }) => {
                        let payload = json!({
                            "op": 14,
                            "d": {
                                "guild_id": guild.to_string(),
                                "typing": true,
                                "activities": true,
                                "threads": false,
                                "channels": { channel.to_string(): [[0, 99]] },
                            }
                        });
                        send(&mut sink, payload).await?;
                    }
                    Some(Outgoing::Voice { guild, channel, mute, deaf }) => {
                        let payload = json!({
                            "op": 4,
                            "d": {
                                "guild_id": guild.map(|g| g.to_string()),
                                "channel_id": channel.map(|c| c.to_string()),
                                "self_mute": mute,
                                "self_deaf": deaf,
                                "self_video": false,
                            }
                        });
                        send(&mut sink, payload).await?;
                    }
                    None => return Ok(End::Stopped),
                }
            }
            frame = stream.next() => {
                let text = match frame {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(frame))) => {
                        return Ok(End::Closed(frame.map_or(1000, |f| u16::from(f.code))));
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => return Err(error.to_string()),
                    None => return Ok(End::Closed(1006)),
                };
                let mut value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                if let (Some(sequence), Some(session)) = (value["s"].as_u64(), session.as_mut()) {
                    session.sequence = Some(sequence);
                }
                match value["op"].as_u64() {
                    Some(10) => {
                        let ms = value["d"]["heartbeat_interval"].as_u64().unwrap_or(41_250);
                        heartbeat = tokio::time::interval(Duration::from_millis(ms));
                        heartbeat.reset();
                        match session {
                            Some(s) => {
                                let resume = json!({
                                    "op": 6,
                                    "d": { "token": token, "session_id": s.id, "seq": s.sequence }
                                });
                                send(&mut sink, resume).await?;
                            }
                            None => send(&mut sink, identify(token)).await?,
                        }
                    }
                    Some(11) => acknowledged = true,
                    Some(1) => {
                        let sequence = session.as_ref().and_then(|s| s.sequence);
                        send(&mut sink, json!({ "op": 1, "d": sequence })).await?;
                    }
                    Some(7) => return Ok(End::Reconnect),
                    Some(9) => {
                        // Not resumable: identify from scratch.
                        if value["d"].as_bool() != Some(true) {
                            *session = None;
                        }
                        return Ok(End::Reconnect);
                    }
                    Some(0) => {
                        let kind = value["t"].as_str().unwrap_or_default().to_owned();
                        let data = value["d"].take();
                        if kind == "READY" {
                            *session = Some(Session {
                                id: data["session_id"].as_str().unwrap_or_default().to_owned(),
                                sequence: value["s"].as_u64(),
                                resume_url: data["resume_gateway_url"].as_str().unwrap_or(GATEWAY).to_owned(),
                            });
                        }
                        let _ = incoming.send(Incoming::Dispatch { kind, data });
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Says who we are. FastDisco names itself; it does not pose as the
/// official client.
fn identify(token: &str) -> Value {
    json!({
        "op": 2,
        "d": {
            "token": token,
            "capabilities": 0,
            "properties": {
                "os": std::env::consts::OS,
                "browser": "FastDisco",
                "device": "FastDisco",
                "system_locale": "pt-BR",
            },
            "presence": { "status": "online", "since": 0, "activities": [], "afk": false },
            "compress": false,
        }
    })
}

async fn send<S>(sink: &mut S, value: Value) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    sink.send(Message::Text(value.to_string().into()))
        .await
        .map_err(|e| e.to_string())
}
