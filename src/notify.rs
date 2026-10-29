//! Desktop notifications for direct messages and mentions, as Discord's:
//! the author as the title, the message as the body; a click brings the
//! window forward on that conversation.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use fastframe_notifications::{App, Client, Event, Handle, Notification, Sound};

use crate::model::Id;

/// Notifications kept so their clicks still arrive.
const KEPT: usize = 20;

pub struct Notifier {
    client: Arc<Mutex<Option<Client>>>,
    shown: Arc<Mutex<Vec<Handle>>>,
    clicks: Sender<Id>,
    clicked: Receiver<Id>,
    ctx: fastframe_shell::Waker,
}

impl Notifier {
    /// Connects to the desktop's notification service off the interface
    /// thread; until then, nothing is shown.
    pub fn start(waker: &fastframe_shell::Waker) -> Self {
        let client = Arc::new(Mutex::new(None));
        let slot = client.clone();
        std::thread::Builder::new()
            .name("notifications".into())
            .spawn(move || {
                let app = App::new("io.github.th3_rocha.Discofast", "Discofast")
                    .desktop_entry("discofast")
                    .icon("discofast");
                match Client::new(app) {
                    Ok(c) => *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(c),
                    Err(error) => log::warn!("notifications: {error}"),
                }
            })
            .expect("the notifications thread starts");
        let (clicks, clicked) = channel();
        Self {
            client,
            shown: Arc::default(),
            clicks,
            clicked,
            ctx: waker.clone(),
        }
    }

    /// Shows a notification with the author's picture, as Discord's. The
    /// picture is fetched once into the cache folder, off the interface
    /// thread; without it, the app's icon stands in.
    pub fn show(&mut self, title: &str, body: &str, picture: Option<&str>, channel: Id, sound: bool) {
        let (title, body, picture) = (title.to_owned(), body.to_owned(), picture.map(str::to_owned));
        let (client, shown) = (Arc::clone(&self.client), Arc::clone(&self.shown));
        let (clicks, ctx) = (self.clicks.clone(), self.ctx.clone());
        let spawned = std::thread::Builder::new()
            .name("notification".into())
            .spawn(move || {
                let mut notification = Notification::new(title)
                    .body(body)
                    .category("im.received")
                    // One notification per conversation, replaced by the newest.
                    .key(format!("channel-{channel}"))
                    .sound(if sound { Sound::Default } else { Sound::Silent });
                if let Some(path) = picture.as_deref().and_then(cached_picture) {
                    notification = notification.image(path);
                }
                let client = client.lock().unwrap_or_else(|e| e.into_inner());
                let Some(client) = client.as_ref() else {
                    return;
                };
                match client.show_with(notification, move |event| {
                    if event == Event::Activated {
                        let _ = clicks.send(channel);
                        ctx.wake();
                    }
                }) {
                    Ok(handle) => {
                        let mut shown = shown.lock().unwrap_or_else(|e| e.into_inner());
                        shown.push(handle);
                        if shown.len() > KEPT {
                            shown.remove(0);
                        }
                    }
                    Err(error) => log::warn!("notification: {error}"),
                }
            });
        if let Err(error) = spawned {
            log::warn!("notification: {error}");
        }
    }

    /// The conversation of a clicked notification.
    pub fn clicked(&self) -> Option<Id> {
        self.clicked.try_recv().ok()
    }
}

/// An avatar on disk for the notification service, which takes a file:
/// fetched once into the cache folder, named after its URL.
fn cached_picture(url: &str) -> Option<std::path::PathBuf> {
    use std::hash::{Hash, Hasher};
    let folder = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))?
        .join("discofast")
        .join("avatars");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    let path = folder.join(format!("{:016x}.png", hasher.finish()));
    if path.exists() {
        return Some(path);
    }
    std::fs::create_dir_all(&folder).ok()?;
    let bytes = reqwest::blocking::Client::new()
        .get(url)
        .timeout(std::time::Duration::from_secs(4))
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .and_then(reqwest::blocking::Response::bytes)
        .map_err(|e| log::debug!("notification picture: {e}"))
        .ok()?;
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

/// A message as plain text for a notification: mentions as names,
/// markdown marks dropped, spoilers hidden.
pub fn plain(text: &str, name: impl Fn(Id) -> String, channel: impl Fn(Id) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find('>') else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let inner = &tail[1..end];
        let replaced = if let Some(id) = inner
            .strip_prefix("@!")
            .or_else(|| inner.strip_prefix('@'))
            .and_then(|i| i.parse().ok())
        {
            format!("@{}", name(id))
        } else if let Some(id) = inner.strip_prefix('#').and_then(|i| i.parse().ok()) {
            format!("#{}", channel(id))
        } else if let Some(emoji) = inner.strip_prefix("a:").or_else(|| inner.strip_prefix(':')) {
            format!(":{}:", emoji.split(':').next().unwrap_or_default())
        } else {
            tail[..=end].to_owned()
        };
        out.push_str(&replaced);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    let mut out = hide_spoilers(&out);
    for mark in ["***", "**", "__", "~~", "`"] {
        out = out.replace(mark, "");
    }
    let out: String = out.chars().take(300).collect();
    out.trim().to_owned()
}

fn hide_spoilers(text: &str) -> String {
    let mut out = String::new();
    let mut parts = text.split("||");
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    let rest: Vec<&str> = parts.collect();
    for (i, part) in rest.iter().enumerate() {
        // Odd pieces are inside a closed pair.
        if i % 2 == 0 && i + 1 < rest.len() {
            out.push_str("[spoiler]");
        } else {
            out.push_str(part);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_reads_like_discord() {
        let text = "oi <@1> veja **isso** em <#2> ||segredo|| <:pog:3>";
        let out = plain(text, |_| "Ana".into(), |_| "geral".into());
        assert_eq!(out, "oi @Ana veja isso em #geral [spoiler] :pog:");
    }
}
