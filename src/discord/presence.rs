//! Rich Presence: what the user is playing or listening to, shown to
//! others as Discord's own app shows it. Two sources:
//!
//! - Discord's local RPC socket (`discord-ipc-0`), which games and apps
//!   (music players, editors) talk to as they do with Discord's client:
//!   a handshake with their application id, then SET_ACTIVITY.
//! - The running processes, compared with Discord's list of detectable
//!   games, for games that do not talk to Discord.
//!
//! Both send `Update`s; the backend puts them into the gateway presence.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;

/// What changed in what the user is doing.
#[derive(Debug)]
pub enum Update {
    /// An app's activity over the RPC socket; none clears it.
    Rpc {
        connection: u64,
        client_id: String,
        activity: Option<Value>,
    },
    /// The app closed its connection.
    Closed { connection: u64 },
    /// A known game is running, or none is.
    Game(Option<Game>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub id: String,
    pub name: String,
    /// When it was first seen, in Unix milliseconds.
    pub since: i64,
}

/// The fake user the RPC handshake answers with: apps only show it.
fn ready() -> Value {
    json!({
        "cmd": "DISPATCH",
        "evt": "READY",
        "data": {
            "v": 1,
            "config": {
                "cdn_host": "cdn.discordapp.com",
                "api_endpoint": "//discord.com/api",
                "environment": "production",
            },
            "user": {
                "id": "0",
                "username": "discofast",
                "discriminator": "0",
                "global_name": "Discofast",
                "avatar": null,
                "bot": false,
                "flags": 0,
                "premium_type": 0,
            },
        },
    })
}

/// One frame: the opcode and the length, little endian, then the JSON.
fn frame(op: u32, body: &Value) -> Vec<u8> {
    let json = body.to_string().into_bytes();
    let mut out = Vec::with_capacity(8 + json.len());
    out.extend(op.to_le_bytes());
    out.extend((json.len() as u32).to_le_bytes());
    out.extend(json);
    out
}

/// Serves Discord's RPC socket until the process ends. Another program
/// already serving it (Discord itself) leaves the next free number, as
/// Discord does with 0 to 9.
pub async fn serve(updates: mpsc::UnboundedSender<Update>) {
    #[cfg(unix)]
    {
        let Some(listener) = bind() else {
            log::info!("rich presence: no free discord-ipc socket");
            return;
        };
        let mut next = 0u64;
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            next += 1;
            let updates = updates.clone();
            tokio::spawn(connection(stream, next, updates));
        }
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ServerOptions;
        let name = r"\\.\pipe\discord-ipc-0";
        let mut next = 0u64;
        let Ok(mut server) = ServerOptions::new().first_pipe_instance(true).create(name) else {
            log::info!("rich presence: discord-ipc-0 is taken");
            return;
        };
        loop {
            if server.connect().await.is_err() {
                continue;
            }
            let connected = server;
            let Ok(fresh) = ServerOptions::new().create(name) else {
                return;
            };
            server = fresh;
            next += 1;
            tokio::spawn(connection(connected, next, updates.clone()));
        }
    }
}

#[cfg(unix)]
fn bind() -> Option<tokio::net::UnixListener> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .or_else(|| std::env::var_os("TMPDIR"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    for n in 0..10 {
        let path = dir.join(format!("discord-ipc-{n}"));
        if path.exists() {
            // A socket nobody answers on is left over from a crash.
            if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                continue;
            }
            let _ = std::fs::remove_file(&path);
        }
        if let Ok(listener) = tokio::net::UnixListener::bind(&path) {
            log::info!("rich presence: listening on {}", path.display());
            return Some(listener);
        }
    }
    None
}

async fn connection<S>(mut stream: S, id: u64, updates: mpsc::UnboundedSender<Update>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut client_id = String::new();
    loop {
        let mut head = [0u8; 8];
        if stream.read_exact(&mut head).await.is_err() {
            break;
        }
        let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        if len > 1 << 20 {
            break;
        }
        let mut body = vec![0u8; len];
        if stream.read_exact(&mut body).await.is_err() {
            break;
        }
        let message: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let reply = match op {
            // Handshake.
            0 => {
                client_id = message["client_id"].as_str().unwrap_or_default().to_owned();
                Some(frame(1, &ready()))
            }
            1 => {
                let cmd = message["cmd"].as_str().unwrap_or_default();
                let nonce = message["nonce"].clone();
                match cmd {
                    "SET_ACTIVITY" => {
                        let activity = message["args"]["activity"].clone();
                        let activity = (!activity.is_null()).then_some(activity);
                        let _ = updates.send(Update::Rpc {
                            connection: id,
                            client_id: client_id.clone(),
                            activity: activity.clone(),
                        });
                        Some(frame(
                            1,
                            &json!({ "cmd": cmd, "evt": null, "data": activity, "nonce": nonce }),
                        ))
                    }
                    // Subscriptions and the rest: said yes to, nothing comes.
                    _ => Some(frame(
                        1,
                        &json!({ "cmd": cmd, "evt": null, "data": {}, "nonce": nonce }),
                    )),
                }
            }
            2 => break,
            // Ping: pong with the same body.
            3 => Some(frame(4, &message)),
            _ => None,
        };
        if let Some(bytes) = reply
            && stream.write_all(&bytes).await.is_err()
        {
            break;
        }
    }
    let _ = updates.send(Update::Closed { connection: id });
}

/// Programs too common to name a game by.
const GENERIC: &[&str] = &[
    "java", "javaw", "python", "python3", "node", "electron", "wine", "wine64",
    "wineserver", "bash", "sh", "steam", "proton", "mono", "love", "godot",
];

/// A game Discord can tell from its executable.
struct Detectable {
    id: String,
    name: String,
    /// Executable names, lowercase, without folders.
    executables: Vec<String>,
}

/// Watches the running programs for a game from Discord's list, every
/// twenty seconds.
pub async fn scan(client: reqwest::Client, token: String, updates: mpsc::UnboundedSender<Update>) {
    let list = match super::rest::call(&client, &token, reqwest::Method::GET, "/applications/detectable", None).await {
        Ok(Value::Array(list)) => list,
        Ok(_) => return,
        Err(error) => {
            log::info!("rich presence: no list of games: {error}");
            return;
        }
    };
    let os = if cfg!(windows) { "win32" } else if cfg!(target_os = "macos") { "darwin" } else { "linux" };
    let games: Vec<Detectable> = list
        .iter()
        .filter_map(|g| {
            let executables: Vec<String> = g["executables"]
                .as_array()?
                .iter()
                .filter(|e| e["os"].as_str() == Some(os) && !e["is_launcher"].as_bool().unwrap_or(false))
                .filter_map(|e| e["name"].as_str())
                .map(|n| n.rsplit(['/', '\\']).next().unwrap_or(n).to_lowercase())
                .filter(|n| n.len() > 2 && !GENERIC.contains(&n.as_str()))
                .collect();
            (!executables.is_empty()).then(|| Detectable {
                id: g["id"].as_str().unwrap_or_default().to_owned(),
                name: g["name"].as_str().unwrap_or_default().to_owned(),
                executables,
            })
        })
        .collect();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, game) in games.iter().enumerate() {
        for exe in &game.executables {
            index.entry(exe.clone()).or_insert(i);
        }
    }
    log::info!("rich presence: {} detectable games", games.len());
    let mut current: Option<Game> = None;
    loop {
        let running = tokio::task::spawn_blocking(processes).await.unwrap_or_default();
        let found = running.iter().find_map(|name| index.get(name)).map(|i| &games[*i]);
        let next = match (found, &current) {
            (Some(game), Some(now)) if now.id == game.id => current.clone(),
            (Some(game), _) => Some(Game {
                id: game.id.clone(),
                name: game.name.clone(),
                since: jiff::Timestamp::now().as_millisecond(),
            }),
            (None, _) => None,
        };
        if next != current {
            current = next.clone();
            if updates.send(Update::Game(next)).is_err() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
}

/// The executable names running now, lowercase. Linux reads /proc; other
/// systems are not scanned.
fn processes() -> HashSet<String> {
    let mut out = HashSet::new();
    #[cfg(target_os = "linux")]
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for entry in dir.flatten() {
            let path = entry.path();
            if let Ok(exe) = std::fs::read_link(path.join("exe"))
                && let Some(name) = exe.file_name()
            {
                out.insert(name.to_string_lossy().to_lowercase());
            }
            // Games under Wine or Proton run as wine; their own name is in
            // the command line.
            if let Ok(cmd) = std::fs::read(path.join("cmdline"))
                && let Some(first) = cmd.split(|b| *b == 0).next()
            {
                let first = String::from_utf8_lossy(first);
                if let Some(name) = first.rsplit(['/', '\\']).next() {
                    out.insert(name.to_lowercase());
                }
            }
        }
    }
    out
}

/// An RPC activity as the gateway takes it: the application, its name,
/// and asset names turned into the application's asset ids.
pub fn gateway_activity(client_id: &str, name: &str, activity: &Value, assets: &HashMap<String, String>) -> Value {
    let mut out = json!({
        "type": activity["type"].as_u64().unwrap_or(0),
        "name": name,
        "application_id": client_id,
    });
    for key in ["details", "state", "timestamps", "party", "instance"] {
        if !activity[key].is_null() {
            out[key] = activity[key].clone();
        }
    }
    let mut picture = json!({});
    for key in ["large_image", "small_image"] {
        if let Some(value) = activity["assets"][key].as_str() {
            let resolved = if value.starts_with("mp:") || value.chars().all(|c| c.is_ascii_digit()) {
                Some(value.to_owned())
            } else {
                assets.get(&value.to_lowercase()).cloned()
            };
            if let Some(id) = resolved {
                picture[key] = json!(id);
            }
        }
    }
    for key in ["large_text", "small_text"] {
        if let Some(value) = activity["assets"][key].as_str() {
            picture[key] = json!(value);
        }
    }
    if picture.as_object().is_some_and(|o| !o.is_empty()) {
        out["assets"] = picture;
    }
    // Buttons go as their labels, with the links in the metadata.
    if let Some(buttons) = activity["buttons"].as_array() {
        let labels: Vec<&str> = buttons.iter().filter_map(|b| b["label"].as_str()).collect();
        let urls: Vec<&str> = buttons.iter().filter_map(|b| b["url"].as_str()).collect();
        if !labels.is_empty() {
            out["buttons"] = json!(labels);
            out["metadata"] = json!({ "button_urls": urls });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_carry_their_opcode_and_length() {
        let bytes = frame(1, &json!({ "a": 1 }));
        assert_eq!(&bytes[..4], &1u32.to_le_bytes());
        assert_eq!(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize, bytes.len() - 8);
    }

    #[test]
    fn asset_names_become_ids() {
        let assets = HashMap::from([("logo".to_owned(), "123".to_owned())]);
        let activity = json!({ "details": "x", "assets": { "large_image": "Logo", "large_text": "t" } });
        let out = gateway_activity("9", "App", &activity, &assets);
        assert_eq!(out["assets"]["large_image"], "123");
        assert_eq!(out["assets"]["large_text"], "t");
        assert_eq!(out["application_id"], "9");
    }

    #[tokio::test]
    async fn an_app_sets_its_activity_over_the_socket() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut app, server) = tokio::io::duplex(4096);
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(connection(server, 1, tx));
        app.write_all(&frame(0, &json!({ "v": 1, "client_id": "42" }))).await.unwrap();
        let mut head = [0u8; 8];
        app.read_exact(&mut head).await.unwrap();
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let mut body = vec![0; len];
        app.read_exact(&mut body).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("READY"));
        let set = json!({ "cmd": "SET_ACTIVITY", "nonce": "n", "args": { "pid": 1, "activity": { "details": "Jogando" } } });
        app.write_all(&frame(1, &set)).await.unwrap();
        match rx.recv().await.unwrap() {
            Update::Rpc { client_id, activity, .. } => {
                assert_eq!(client_id, "42");
                assert_eq!(activity.unwrap()["details"], "Jogando");
            }
            other => panic!("{other:?}"),
        }
    }
}
