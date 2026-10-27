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
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("{status} {body}"));
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
    /// A small looping MP4, as Discord's picker shows.
    pub src: String,
    pub width: u32,
    pub height: u32,
}

/// The picker's first screen: the trending searches with a preview each.
#[derive(Clone, Debug, Default)]
pub struct GifHome {
    pub categories: Vec<(String, String)>,
    /// A preview for the "trending" tile.
    pub trending: Option<String>,
}

fn gif_of(g: &Value) -> Option<Gif> {
    Some(Gif {
        url: g["url"].as_str()?.to_owned(),
        src: g["src"].as_str().or(g["gif_src"].as_str())?.to_owned(),
        width: g["width"].as_u64().unwrap_or(200) as u32,
        height: g["height"].as_u64().unwrap_or(150) as u32,
    })
}

/// Trending GIFs with no query, else the search.
pub async fn gifs(
    client: &reqwest::Client,
    token: &str,
    query: Option<&str>,
) -> Result<Vec<Gif>, String> {
    let url = match query {
        Some(q) => format!(
            "{API}/gifs/search?q={}&media_format=mp4&provider=tenor&locale=pt-BR&limit=40",
            percent(q)
        ),
        None => format!(
            "{API}/gifs/trending-gifs?media_format=mp4&provider=tenor&locale=pt-BR&limit=40"
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
    Ok(list.iter().filter_map(gif_of).collect())
}

pub async fn gif_home(client: &reqwest::Client, token: &str) -> Result<GifHome, String> {
    let url = format!("{API}/gifs/trending?provider=tenor&locale=pt-BR&media_format=mp4");
    let response = client
        .get(url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let value: Value = response.json().await.map_err(|e| e.to_string())?;
    let categories = value["categories"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some((
                c["name"].as_str()?.to_owned(),
                c["src"].as_str()?.to_owned(),
            ))
        })
        .collect();
    let trending = value["gifs"]
        .as_array()
        .and_then(|l| l.first())
        .and_then(gif_of)
        .map(|g| g.src);
    Ok(GifHome {
        categories,
        trending,
    })
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

/// A user's profile, as seen from a server when `guild` is given.
pub async fn profile(
    client: &reqwest::Client,
    token: &str,
    user: u64,
    guild: Option<u64>,
) -> Result<Value, String> {
    let mut url = format!(
        "{API}/users/{user}/profile?with_mutual_guilds=true&with_mutual_friends_count=true"
    );
    if let Some(guild) = guild {
        url.push_str(&format!("&guild_id={guild}"));
    }
    let response = client
        .get(url)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(response.status().to_string());
    }
    response.json().await.map_err(|e| e.to_string())
}

/// The user's private note about someone.
pub async fn note(client: &reqwest::Client, token: &str, user: u64) -> Option<String> {
    let response = client
        .get(format!("{API}/users/@me/notes/{user}"))
        .header("Authorization", token)
        .send()
        .await
        .ok()?;
    let value: Value = response.json().await.ok()?;
    value["note"].as_str().map(str::to_owned)
}

pub async fn set_note(
    client: &reqwest::Client,
    token: &str,
    user: u64,
    note: &str,
) -> Result<(), String> {
    let response = client
        .put(format!("{API}/users/@me/notes/{user}"))
        .header("Authorization", token)
        .json(&json!({ "note": note }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(response.status().to_string())
    }
}

/// Opens (or finds) the direct message with a user.
pub async fn open_direct(
    client: &reqwest::Client,
    token: &str,
    user: u64,
) -> Result<Value, String> {
    let response = client
        .post(format!("{API}/users/@me/channels"))
        .header("Authorization", token)
        .json(&json!({ "recipients": [user.to_string()] }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(response.status().to_string());
    }
    response.json().await.map_err(|e| e.to_string())
}

/// The icon of a game, for Rich Presence without pictures of its own.
pub async fn application_icon(
    client: &reqwest::Client,
    token: &str,
    application: u64,
) -> Option<String> {
    let response = client
        .get(format!("{API}/applications/{application}/rpc"))
        .header("Authorization", token)
        .send()
        .await
        .ok()?;
    let value: Value = response.json().await.ok()?;
    let icon = value["icon"].as_str()?;
    Some(format!(
        "https://cdn.discordapp.com/app-icons/{application}/{icon}.png?size=128"
    ))
}

/// Changes the display name, the about me and the pronouns.
pub async fn edit_profile(
    client: &reqwest::Client,
    token: &str,
    display_name: &str,
    bio: &str,
    pronouns: &str,
) -> Result<(), String> {
    let name = client
        .patch(format!("{API}/users/@me"))
        .header("Authorization", token)
        .json(&json!({ "global_name": if display_name.is_empty() { Value::Null } else { json!(display_name) } }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !name.status().is_success() {
        let body: Value = name.json().await.unwrap_or(Value::Null);
        return Err(body["message"]
            .as_str()
            .unwrap_or("Não foi possível mudar o nome.")
            .to_owned());
    }
    let profile = client
        .patch(format!("{API}/users/@me/profile"))
        .header("Authorization", token)
        .json(&json!({ "bio": bio, "pronouns": pronouns }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if profile.status().is_success() {
        Ok(())
    } else {
        let body: Value = profile.json().await.unwrap_or(Value::Null);
        Err(body["message"]
            .as_str()
            .unwrap_or("Não foi possível salvar o perfil.")
            .to_owned())
    }
}

/// Any API call: the method, the path under /api/v9 and a JSON body. The
/// reply's JSON (`null` when empty), or Discord's message on failure.
pub async fn call(
    client: &reqwest::Client,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let mut request = client
        .request(method, format!("{API}{path}"))
        .header("Authorization", token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status.is_success() {
        Ok(value)
    } else {
        let message = value["message"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| status.to_string());
        Err(match value["code"].as_i64() {
            Some(50013) => "Você não tem permissão para isso.".into(),
            Some(50001) => "Sem acesso.".into(),
            _ => message,
        })
    }
}

/// A voice message to send: the OGG file and what Discord's player shows.
#[derive(Clone, Debug)]
pub struct Voice {
    pub nonce: String,
    pub reply: Option<u64>,
    pub ogg: Vec<u8>,
    pub seconds: f32,
    pub waveform: Vec<u8>,
}

/// Sends a voice message as the phone app does: the file goes up to the
/// upload URL Discord hands out, then a message with the voice flag names
/// it, its length and its waveform.
pub async fn send_voice(
    client: &reqwest::Client,
    token: &str,
    channel: u64,
    voice: Voice,
) -> Result<(), String> {
    use base64::Engine;
    const NAME: &str = "voice-message.ogg";
    const IS_VOICE_MESSAGE: u64 = 1 << 13;
    let slot: Value = client
        .post(format!("{API}/channels/{channel}/attachments"))
        .header("Authorization", token)
        .json(&json!({ "files": [{ "id": "0", "filename": NAME, "file_size": voice.ogg.len() }] }))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    let file = &slot["attachments"][0];
    let (Some(url), Some(uploaded)) = (
        file["upload_url"].as_str(),
        file["upload_filename"].as_str(),
    ) else {
        return Err(slot["message"]
            .as_str()
            .unwrap_or("O Discord recusou o envio do áudio.")
            .to_owned());
    };
    let put = client
        .put(url)
        .header("Content-Type", "audio/ogg")
        .body(voice.ogg)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !put.status().is_success() {
        return Err(format!("Envio do áudio falhou: {}", put.status()));
    }
    let mut payload = json!({
        "content": "",
        "nonce": voice.nonce,
        "flags": IS_VOICE_MESSAGE,
        "attachments": [{
            "id": "0",
            "filename": NAME,
            "uploaded_filename": uploaded,
            "duration_secs": voice.seconds,
            "waveform": base64::engine::general_purpose::STANDARD.encode(&voice.waveform),
        }],
    });
    if let Some(reply) = voice.reply {
        payload["message_reference"] =
            json!({ "message_id": reply.to_string(), "channel_id": channel.to_string() });
    }
    let response = client
        .post(format!("{API}/channels/{channel}/messages"))
        .header("Authorization", token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let body: Value = response.json().await.unwrap_or(Value::Null);
    Err(match body["code"].as_i64() {
        Some(50013) => "Você não tem permissão para enviar mensagens de voz aqui.".into(),
        _ => body["message"]
            .as_str()
            .map_or_else(|| status.to_string(), str::to_owned),
    })
}
