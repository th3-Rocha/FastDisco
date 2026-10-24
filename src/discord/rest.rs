//! The HTTP calls the chat needs: history, sending and marking as read.

use serde_json::{Value, json};

use super::API;

/// The newest messages of a channel, newest first, as Discord returns them.
pub async fn messages(
    client: &reqwest::Client,
    token: &str,
    channel: u64,
) -> Result<Vec<Value>, String> {
    let response = client
        .get(format!("{API}/channels/{channel}/messages?limit=50"))
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
    content: &str,
    nonce: &str,
) -> Result<(), String> {
    let response = client
        .post(format!("{API}/channels/{channel}/messages"))
        .header("Authorization", token)
        .json(&json!({ "content": content, "nonce": nonce, "tts": false, "flags": 0 }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("{}", response.status()))
    }
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
