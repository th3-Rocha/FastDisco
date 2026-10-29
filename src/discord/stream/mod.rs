//! Watching a screen share. Discord gives each Go Live its own voice
//! server, apart from the call's, so this does not touch songbird: a voice
//! gateway of its own, the UDP stream, its transport and DAVE encryption,
//! the video packets put back into frames, and `ffmpeg` turning them into
//! pictures. Only built with the `video` feature; without it, `watch`
//! says so.

use std::sync::Mutex;

#[cfg(feature = "video")]
mod connection;
#[cfg(feature = "video")]
mod decode;
#[cfg(feature = "video")]
mod rtp;

/// Where the share is, from STREAM_CREATE and STREAM_SERVER_UPDATE.
#[derive(Clone, Debug)]
#[cfg_attr(not(feature = "video"), allow(dead_code))]
pub struct Server {
    pub rtc_server_id: u64,
    pub user: u64,
    /// Who is sharing.
    pub streamer: u64,
    /// The call's session, from VOICE_STATE_UPDATE.
    pub session_id: String,
    pub endpoint: String,
    pub token: String,
}

/// One decoded picture, RGBA.
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// The newest picture, shared between the decoder and the interface,
/// which takes it to upload once. Older ones not yet shown are dropped.
#[derive(Default)]
pub struct Screen {
    latest: Mutex<Option<Frame>>,
}

impl Screen {
    #[cfg_attr(not(feature = "video"), allow(dead_code))]
    pub fn show(&self, frame: Frame) {
        *self.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(frame);
    }

    pub fn take(&self) -> Option<Frame> {
        self.latest.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

impl std::fmt::Debug for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Screen").finish_non_exhaustive()
    }
}

/// Connects and shows the share until it ends or the task is dropped.
/// `wake` redraws for each picture; `connected` once the connection is up.
#[cfg(feature = "video")]
pub async fn watch(
    server: Server,
    screen: std::sync::Arc<Screen>,
    wake: impl Fn() + Send + Sync + 'static,
    connected: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    connection::run(server, screen, wake, connected).await
}

#[cfg(not(feature = "video"))]
pub async fn watch(
    _server: Server,
    _screen: std::sync::Arc<Screen>,
    _wake: impl Fn() + Send + Sync + 'static,
    _connected: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    Err("Este Discofast foi compilado sem vídeo.".to_owned())
}
