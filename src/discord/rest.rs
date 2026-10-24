//! The HTTP calls the chat needs: history, sending with files, editing,
//! deleting, reactions, typing, marking as read and the GIF search.

use serde_json::{Value, json};

use super::API;

/// A file to send: read from disk, or made in memory (a long message sent
/// as `message.txt`).
#[derive(Clone, Debug)]
pub struct Upload {
    pub name: String,
    pub source: UploadSource,
}

#[derive(Clone, Debug)]
pub enum UploadSource {
    Path(std::path::PathBuf),
    Bytes(Vec<u8>),
}

/// What a sent message holds besides its text.
#[derive(Clone, Debug, Default)]
pub struct Outgoing {
    pub content: String,
    pub nonce: String,
    pub reply: Option<u64>,
    pub sticker: Option<u64>,
    pub files: Vec<Upload>,
}

/// Messages of a channel, newest first, as Discord returns them; with
/// `before`, the ones older than that message.
pub async fn messages(
    client: &reqwest::Client,
    token: &str,
    channel: u64,
    before: Option<u64>,
) -> Result<Vec<Value>, String> {
    let mut url = format!("{API}/channels/{channel}/messages?limit=50");
    if let Some(before) = before {
        url.push_str(&format!("&before={before}"));
    }
    let response = client
        .get(url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("{}", response.status()));
    }
    response.json().await.map_err(|e| e.to_string())
}

pub async fn send(
    client: &reqwest::Client,
    token: &str,
    channel: u64,
    message: Outgoing,
) -> Result<(), String> {
    let mut payload =
        json!({ "content": message.content, "nonce": message.nonce, "tts": false, "flags": 0 });
    if let Some(reply) = message.reply {
        payload["message_reference"] =
            json!({ "message_id": reply.to_string(), "channel_id": channel.to_string() });
    }
    if let Some(sticker) = message.sticker {
        payload["sticker_ids"] = json!([sticker.to_string()]);
    }
    let request = client
        .post(format!("{API}/channels/{channel}/messages"))
        .header("Authorization", token);
    let request = if message.files.is_empty() {
        request.json(&payload)
    } else {
        payload["attachments"] = message
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| json!({ "id": i, "filename": f.name }))
            .collect();
        let mut form = reqwest::multipart::Form::new().text("payload_json", payload.to_string());
        for (i, file) in message.files.into_iter().enumerate() {
            let bytes = match file.source {
                UploadSource::Bytes(bytes) => bytes,
                UploadSource::Path(path) => {
                    tokio::fs::read(&path).await.map_err(|e| e.to_string())?
                }
            };
            form = form.part(
                format!("files[{i}]"),
                reqwest::multipart::Part::bytes(bytes).file_name(file.name),
            );
        }
        request.multipart(form)
    };
    let response = request.send().await.map_err(|e| e.to_string())?;
    match response.status() {
        status if status.is_success() => Ok(()),
        reqwest::StatusCode::PAYLOAD_TOO_LARGE => Err("Arquivo grande demais.".into()),
        status => {
            let body: Value = response.json().await.unwrap_or(Value::Null);
            Err(body["message"]
                .as_str()
                .map_or_else(|| status.to_string(), str::to_owned))
        }
    }
}

pub async fn edit(
    client: &reqwest::Client,
    token: &str,
    channel: u64,
    message: u64,
    content: &str,
) -> Result<(), String> {
    let response = client
        .patch(format!("{API}/channels/{channel}/messages/{message}"))
        .header("Authorization", token)
        .json(&json!({ "content": content }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(response.status().to_string())
    }
}

pub async fn delete(
    client: &reqwest::Client,
    token: &str,
    channel: u64,
    message: u64,
) -> Result<(), String> {
    let response = client
        .delete(format!("{API}/channels/{channel}/messages/{message}"))
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(response.status().to_string())
    }
}

/// Adds or removes the user's reaction. `emoji` is already in path form.
pub async fn react(
    client: &reqwest::Client,
    token: &str,
    channel: u64,
    message: u64,
    emoji: &str,
    add: bool,
) -> Result<(), String> {
    let url = format!("{API}/channels/{channel}/messages/{message}/reactions/{emoji}/@me");
    let request = if add {
        client.put(url)
    } else {
        client.delete(url)
    };
    let response = request
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(response.status().to_string())
    }
}

/// "Is typing…" for about ten seconds.
pub async fn typing(client: &reqwest::Client, token: &str, channel: u64) {
    let _ = client
        .post(format!("{API}/channels/{channel}/typing"))
        .header("Authorization", token)
        .send()
        .await;
}

/// Marks a channel read up to a message, on every device.
pub async fn ack(client: &reqwest::Client, token: &str, channel: u64, message: u64) {
    let _ = client
        .post(format!("{API}/channels/{channel}/messages/{message}/ack"))
        .header("Authorization", token)
        .json(&json!({ "token": null }))
        .send()
        .await;
}

/// A GIF from Discord's Tenor search.
#[derive(Clone, Debug)]
pub struct Gif {
    /// The page link that is sent as the message.
    pub url: String,
    /// The animated picture.
    pub src: String,
    pub width: u32,
    pub height: u32,
}

/// Trending GIFs with no query, else the search.
pub async fn gifs(
    client: &reqwest::Client,
    token: &str,
    query: Option<&str>,
) -> Result<Vec<Gif>, String> {
    let url = match query {
        Some(q) => format!(
            "{API}/gifs/search?q={}&media_format=gif&provider=tenor&locale=pt-BR&limit=30",
            percent(q)
        ),
        None => format!(
            "{API}/gifs/trending-gifs?media_format=gif&provider=tenor&locale=pt-BR&limit=30"
        ),
    };
    let response = client
        .get(url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let value: Value = response.json().await.map_err(|e| e.to_string())?;
    let list = value
        .as_array()
        .or_else(|| value["gifs"].as_array())
        .cloned()
        .unwrap_or_default();
    Ok(list
        .iter()
        .filter_map(|g| {
            Some(Gif {
                url: g["url"].as_str()?.to_owned(),
                src: g["gif_src"].as_str().or(g["src"].as_str())?.to_owned(),
                width: g["width"].as_u64().unwrap_or(200) as u32,
                height: g["height"].as_u64().unwrap_or(150) as u32,
            })
        })
        .collect())
}

fn percent(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
