//! Talking to Discord: the HTTP API and the gateway.

pub mod auth;
pub mod call;
pub mod frecency;
pub mod gateway;
pub mod parse;
pub mod remote_auth;
pub mod rest;

pub const API: &str = "https://discord.com/api/v9";
pub const CDN: &str = "https://cdn.discordapp.com";

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
