//! Camera and screen between friends on their own LiveKit server, beside
//! the Discord call. The server's address and key are in the voice
//! settings and its secret in the OS keyring; each call's voice channel is
//! one room, so the friends in the same channel see each other. The sound
//! stays on Discord. Only built with the `selfhosted` feature.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::discord::stream::Frame;

#[cfg(feature = "selfhosted")]
mod live;

/// Whether this build can use a server of the user's own.
pub const AVAILABLE: bool = cfg!(feature = "selfhosted");

/// The server, as the settings name it. The secret is read from the
/// keyring on the backend, never kept by the interface.
#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "selfhosted"), allow(dead_code))]
pub struct Server {
    pub url: String,
    pub key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Camera,
    Screen,
}

/// What the interface asks of the room.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(feature = "selfhosted"), allow(dead_code))]
pub enum Control {
    Camera(bool),
    Screen(bool),
}

/// Everyone's video in the room, the user's own too: present while it is
/// being sent, with the newest picture until the interface takes it.
#[derive(Default)]
pub struct Videos {
    slots: Mutex<HashMap<(u64, Kind), Option<Frame>>>,
}

impl Videos {
    #[cfg_attr(not(feature = "selfhosted"), allow(dead_code))]
    pub fn show(&self, user: u64, kind: Kind, frame: Frame) {
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        slots.insert((user, kind), Some(frame));
    }

    #[cfg_attr(not(feature = "selfhosted"), allow(dead_code))]
    pub fn remove(&self, user: u64, kind: Kind) {
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        slots.remove(&(user, kind));
    }

    #[cfg_attr(not(feature = "selfhosted"), allow(dead_code))]
    pub fn clear(&self) {
        self.slots.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Who is sending what, and the pictures that came since last time.
    pub fn take(&self) -> Vec<((u64, Kind), Option<Frame>)> {
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        slots.iter_mut().map(|(k, f)| (*k, f.take())).collect()
    }
}

impl std::fmt::Debug for Videos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Videos").finish_non_exhaustive()
    }
}

/// The room's name on the server for a voice channel.
pub fn room_name(channel: u64) -> String {
    format!("discofast-{channel}")
}

/// Joins the room and keeps it until the task is dropped. `wake` redraws
/// for each picture; `connected` once the room is joined.
#[cfg(feature = "selfhosted")]
#[allow(clippy::too_many_arguments)]
pub async fn run(
    server: Server,
    secret: String,
    room: String,
    me: u64,
    name: String,
    videos: std::sync::Arc<Videos>,
    controls: tokio::sync::mpsc::UnboundedReceiver<Control>,
    wake: impl Fn() + Send + Sync + 'static,
    connected: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    live::run(
        server, secret, room, me, name, videos, controls, wake, connected,
    )
    .await
}

#[cfg(not(feature = "selfhosted"))]
#[allow(clippy::too_many_arguments)]
pub async fn run(
    _server: Server,
    _secret: String,
    _room: String,
    _me: u64,
    _name: String,
    _videos: std::sync::Arc<Videos>,
    _controls: tokio::sync::mpsc::UnboundedReceiver<Control>,
    _wake: impl Fn() + Send + Sync + 'static,
    _connected: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    Err("Este Discofast foi compilado sem servidor de vídeo próprio.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_taken_once_and_the_video_stays_until_removed() {
        let videos = Videos::default();
        let frame = || Frame {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        };
        videos.show(7, Kind::Camera, frame());
        let first = videos.take();
        assert!(matches!(&first[..], [((7, Kind::Camera), Some(_))]));
        assert!(matches!(&videos.take()[..], [((7, Kind::Camera), None)]));
        videos.remove(7, Kind::Camera);
        assert!(videos.take().is_empty());
        assert_eq!(room_name(5), "discofast-5");
    }
}
