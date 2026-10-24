//! Talking to Discord: the HTTP API and, later, the gateway.

pub mod auth;
pub mod remote_auth;

use serde::Deserialize;

pub const API: &str = "https://discord.com/api/v9";
pub const CDN: &str = "https://cdn.discordapp.com";

/// The signed-in user, from `GET /users/@me`.
#[derive(Clone, Debug, Deserialize)]
pub struct Me {
    pub id: String,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar: Option<String>,
}

impl Me {
    /// The name Discord shows: the display name, else the username.
    pub fn display_name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }

    pub fn avatar_url(&self) -> Option<String> {
        avatar_url(&self.id, self.avatar.as_deref()?)
    }
}

pub fn avatar_url(user: &str, hash: &str) -> Option<String> {
    (!hash.is_empty() && hash != "0").then(|| format!("{CDN}/avatars/{user}/{hash}.png?size=128"))
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!(
            "FastDisco/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/th3-Rocha/FastDisco)"
        ))
        .build()
        .expect("the HTTP client builds with static settings")
}
