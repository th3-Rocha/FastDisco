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
    shown: Vec<Handle>,
    clicks: Sender<Id>,
    clicked: Receiver<Id>,
    ctx: egui::Context,
}

impl Notifier {
    /// Connects to the desktop's notification service off the interface
    /// thread; until then, nothing is shown.
    pub fn start(ctx: &egui::Context) -> Self {
        let client = Arc::new(Mutex::new(None));
        let slot = client.clone();
        std::thread::Builder::new()
            .name("notifications".into())
            .spawn(move || {
                let app = App::new("io.github.th3_rocha.FastDisco", "FastDisco")
                    .desktop_entry("fastdisco")
                    .icon("fastdisco");
                match Client::new(app) {
                    Ok(c) => *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(c),
                    Err(error) => log::warn!("notifications: {error}"),
                }
            })
            .expect("the notifications thread starts");
        let (clicks, clicked) = channel();
        Self {
            client,
            shown: Vec::new(),
            clicks,
            clicked,
            ctx: ctx.clone(),
        }
    }

    pub fn show(&mut self, title: &str, body: &str, channel: Id, sound: bool) {
        let client = self.client.lock().unwrap_or_else(|e| e.into_inner());
        let Some(client) = client.as_ref() else {
            return;
        };
        let notification = Notification::new(title)
            .body(body)
            // One notification per conversation, replaced by the newest.
            .key(format!("channel-{channel}"))
            .sound(if sound { Sound::Default } else { Sound::Silent });
        let (clicks, ctx) = (self.clicks.clone(), self.ctx.clone());
        match client.show_with(notification, move |event| {
            if event == Event::Activated {
                let _ = clicks.send(channel);
                ctx.request_repaint();
            }
        }) {
            Ok(handle) => {
                self.shown.push(handle);
                if self.shown.len() > KEPT {
                    self.shown.remove(0);
                }
            }
            Err(error) => log::warn!("notification: {error}"),
        }
    }

    /// The conversation of a clicked notification.
    pub fn clicked(&self) -> Option<Id> {
        self.clicked.try_recv().ok()
    }
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
