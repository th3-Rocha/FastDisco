//! The room on LiveKit: the access token made from the server's key, the
//! connection, the user's camera (through ffmpeg) and screen (through
//! WebRTC's own capturer, the desktop's portal on Wayland) sent, and the
//! friends' video turned into RGBA pictures.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use livekit::options::{TrackPublishOptions, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::desktop_capturer::{
    DesktopCaptureSourceType, DesktopCapturer, DesktopCapturerOptions,
};
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::prelude::{
    I420Buffer, RtcVideoSource, VideoBuffer, VideoFrame, VideoResolution, VideoRotation,
};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit_api::access_token::{AccessToken, VideoGrants};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::{Control, Kind, Server, Videos};
use crate::discord::stream::Frame;

// WebRTC's screen capture talks to the desktop portal through GIO, and
// webrtc-sys leaves linking it to the application.
#[cfg(target_os = "linux")]
#[link(name = "gio-2.0")]
#[link(name = "gobject-2.0")]
#[link(name = "glib-2.0")]
unsafe extern "C" {}

/// The camera's size.
const CAMERA: (u32, u32) = (1280, 720);
/// The screen's rate and widest size, as Discord sends a screen by
/// default.
const SCREEN_FPS: u32 = 15;
const SCREEN_WIDTH: u32 = 1920;
/// The width of the user's own screen in their tile.
const PREVIEW_WIDTH: i32 = 640;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    server: Server,
    secret: String,
    room: String,
    me: u64,
    name: String,
    videos: Arc<Videos>,
    mut controls: mpsc::UnboundedReceiver<Control>,
    wake: impl Fn() + Send + Sync + 'static,
    connected: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    let token = token(&server, &secret, &room, me, &name)?;
    let (room, mut events) = Room::connect(&server.url, &token, RoomOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    connected();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
    // Each friend's video, read on a task of its own.
    let mut readers: HashMap<TrackSid, (u64, Kind, JoinHandle<()>)> = HashMap::new();
    let mut camera: Option<Sending> = None;
    let mut screen: Option<Sending> = None;
    let result = loop {
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { break Ok(()) };
                match event {
                    RoomEvent::TrackSubscribed { track: RemoteTrack::Video(track), publication, participant } => {
                        let Some(user) = participant.identity().as_str().parse::<u64>().ok() else {
                            continue;
                        };
                        let kind = kind(publication.source());
                        let task = tokio::spawn(read(track, user, kind, Arc::clone(&videos), Arc::clone(&wake)));
                        if let Some((_, _, old)) = readers.insert(publication.sid(), (user, kind, task)) {
                            old.abort();
                        }
                    }
                    RoomEvent::TrackUnsubscribed { publication, .. } => {
                        if let Some((user, kind, task)) = readers.remove(&publication.sid()) {
                            task.abort();
                            videos.remove(user, kind);
                            wake();
                        }
                    }
                    RoomEvent::Disconnected { reason } => {
                        break Err(format!("o servidor de vídeo desconectou ({reason:?})"));
                    }
                    _ => {}
                }
            }
            control = controls.recv() => {
                let Some(control) = control else { break Ok(()) };
                let (slot, kind, on) = match control {
                    Control::Camera(on) => (&mut camera, Kind::Camera, on),
                    Control::Screen(on) => (&mut screen, Kind::Screen, on),
                };
                if let Some(old) = slot.take() {
                    old.stop(&room).await;
                    videos.remove(me, kind);
                    wake();
                }
                if on {
                    match Sending::start(&room, kind, me, Arc::clone(&videos), Arc::clone(&wake)).await {
                        Ok(sending) => *slot = Some(sending),
                        Err(e) => log::warn!("own video {kind:?}: {e}"),
                    }
                }
            }
        }
    };
    for (_, (_, _, task)) in readers {
        task.abort();
    }
    for sending in [camera, screen].into_iter().flatten() {
        sending.stop(&room).await;
    }
    videos.clear();
    let _ = room.close().await;
    wake();
    result
}

/// The pass into the room, made here: whoever holds the secret runs the
/// server, so it is given only to friends.
fn token(server: &Server, secret: &str, room: &str, me: u64, name: &str) -> Result<String, String> {
    AccessToken::with_api_key(&server.key, secret)
        .with_identity(&me.to_string())
        .with_name(name)
        .with_ttl(Duration::from_secs(6 * 60 * 60))
        .with_grants(VideoGrants {
            room_join: true,
            room: room.to_owned(),
            can_publish: Some(true),
            can_subscribe: Some(true),
            ..Default::default()
        })
        .to_jwt()
        .map_err(|e| format!("chave do servidor de vídeo: {e}"))
}

fn kind(source: TrackSource) -> Kind {
    match source {
        TrackSource::Screenshare => Kind::Screen,
        _ => Kind::Camera,
    }
}

/// A friend's video into pictures.
async fn read(
    track: RemoteVideoTrack,
    user: u64,
    kind: Kind,
    videos: Arc<Videos>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let mut stream = NativeVideoStream::new(track.rtc_track());
    while let Some(frame) = stream.next().await {
        // The interface still has the last one: skip this rather than fall
        // behind, which would delay every picture after it.
        if !videos.wants(user, kind) {
            continue;
        }
        if let Some(frame) = rgba(frame.buffer.as_ref()) {
            videos.show(user, kind, frame);
            wake();
        }
    }
}

/// A decoded picture as RGBA (libyuv's "ABGR" is R, G, B, A in memory).
fn rgba(buffer: &dyn VideoBuffer) -> Option<Frame> {
    let (width, height) = (buffer.width(), buffer.height());
    let mut out = vec![0u8; width as usize * height as usize * 4];
    if let Some(i420) = buffer.as_i420() {
        let (y, u, v) = i420.data();
        let (sy, su, sv) = i420.strides();
        yuv_helper::i420_to_abgr(
            y,
            sy,
            u,
            su,
            v,
            sv,
            &mut out,
            width * 4,
            width as i32,
            height as i32,
        );
    } else {
        let nv12 = buffer.as_nv12()?;
        let (y, uv) = nv12.data();
        let (sy, suv) = nv12.strides();
        yuv_helper::nv12_to_abgr(
            y,
            sy,
            uv,
            suv,
            &mut out,
            width * 4,
            width as i32,
            height as i32,
        );
    }
    Some(Frame {
        width: width as usize,
        height: height as usize,
        rgba: out,
    })
}

/// The user's camera or screen being sent.
struct Sending {
    sid: TrackSid,
    stop: Arc<AtomicBool>,
    camera: Arc<Mutex<Option<Child>>>,
}

impl Sending {
    async fn start(
        room: &Room,
        kind: Kind,
        me: u64,
        videos: Arc<Videos>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        let mut resolution = VideoResolution::default();
        (resolution.width, resolution.height) = match kind {
            Kind::Camera => CAMERA,
            Kind::Screen => (1920, 1080),
        };
        let source = NativeVideoSource::new(resolution, kind == Kind::Screen);
        let stop = Arc::new(AtomicBool::new(false));
        let ffmpeg: Arc<Mutex<Option<Child>>> = Arc::default();
        match kind {
            Kind::Camera => camera(
                source.clone(),
                me,
                videos,
                wake,
                Arc::clone(&stop),
                Arc::clone(&ffmpeg),
            )?,
            Kind::Screen => screen(source.clone(), me, videos, wake, Arc::clone(&stop))?,
        }
        let track = LocalVideoTrack::create_video_track(
            match kind {
                Kind::Camera => "camera",
                Kind::Screen => "screen",
            },
            RtcVideoSource::Native(source),
        );
        // A screen in one layer at its own rate: simulcast would encode it
        // several times over.
        let options = match kind {
            Kind::Camera => TrackPublishOptions {
                source: TrackSource::Camera,
                video_encoding: Some(VideoEncoding {
                    max_bitrate: 1_700_000,
                    max_framerate: 30.0,
                }),
                ..Default::default()
            },
            Kind::Screen => TrackPublishOptions {
                source: TrackSource::Screenshare,
                video_encoding: Some(VideoEncoding {
                    max_bitrate: 2_500_000,
                    max_framerate: f64::from(SCREEN_FPS),
                }),
                simulcast: false,
                ..Default::default()
            },
        };
        let publication = room
            .local_participant()
            .publish_track(LocalTrack::Video(track), options)
            .await;
        match publication {
            Ok(publication) => Ok(Self {
                sid: publication.sid(),
                stop,
                camera: ffmpeg,
            }),
            Err(e) => {
                halt(&stop, &ffmpeg);
                Err(e.to_string())
            }
        }
    }

    async fn stop(self, room: &Room) {
        halt(&self.stop, &self.camera);
        let _ = room.local_participant().unpublish_track(&self.sid).await;
    }
}

/// Stops the capture: the screen's thread at its next picture, the
/// camera's ffmpeg at once.
fn halt(stop: &AtomicBool, camera: &Mutex<Option<Child>>) {
    stop.store(true, Ordering::Relaxed);
    if let Some(mut child) = camera.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// The camera through ffmpeg, as RGBA pictures of a fixed size, into the
/// track and into the user's own tile. Webcams give 720p at 30 fps only
/// compressed, so MJPEG is asked for first and the camera's own format
/// after, if it has no MJPEG.
fn camera(
    source: NativeVideoSource,
    me: u64,
    videos: Arc<Videos>,
    wake: Arc<dyn Fn() + Send + Sync>,
    stop: Arc<AtomicBool>,
    slot: Arc<Mutex<Option<Child>>>,
) -> Result<(), String> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err("a câmera ainda só funciona no Linux e no macOS".to_owned());
    }
    let (w, h) = CAMERA;
    std::thread::Builder::new()
        .name("own-camera".into())
        .spawn(move || {
            let mut picture = vec![0u8; w as usize * h as usize * 4];
            for compressed in [true, false] {
                let Some(mut out) = open_camera(compressed, &slot) else {
                    continue;
                };
                let mut count = 0u64;
                while !stop.load(Ordering::Relaxed) && out.read_exact(&mut picture).is_ok() {
                    send(&source, &picture, w, h);
                    // The user's own tile needs half the pictures.
                    if count.is_multiple_of(2) {
                        videos.show(
                            me,
                            Kind::Camera,
                            Frame {
                                width: w as usize,
                                height: h as usize,
                                rgba: picture.clone(),
                            },
                        );
                        wake();
                    }
                    count += 1;
                }
                if count > 0 || stop.load(Ordering::Relaxed) {
                    return;
                }
                log::info!("own camera: no picture with compressed={compressed}");
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Starts ffmpeg on the camera and keeps it in `slot` to be stopped.
fn open_camera(compressed: bool, slot: &Mutex<Option<Child>>) -> Option<std::process::ChildStdout> {
    let (w, h) = CAMERA;
    let size = format!("{w}x{h}");
    let mut input: Vec<&str> = Vec::new();
    if cfg!(target_os = "linux") {
        input.extend(["-f", "v4l2"]);
        if compressed {
            input.extend(["-input_format", "mjpeg", "-video_size", &size]);
        }
        input.extend(["-framerate", "30", "-i", "/dev/video0"]);
    } else {
        input.extend(["-f", "avfoundation", "-framerate", "30"]);
        if compressed {
            input.extend(["-video_size", &size]);
        }
        input.extend(["-i", "0"]);
    }
    let filter = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2"
    );
    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-fflags", "nobuffer"])
        .args(&input)
        .args([
            "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .inspect_err(|e| log::warn!("own camera: ffmpeg: {e}"))
        .ok()?;
    let out = child.stdout.take();
    let old = slot
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(child);
    if let Some(mut old) = old {
        let _ = old.kill();
        let _ = old.wait();
    }
    out
}

/// The screen through WebRTC's capturer; on Wayland the desktop asks the
/// user which screen or window to share.
fn screen(
    source: NativeVideoSource,
    me: u64,
    videos: Arc<Videos>,
    wake: Arc<dyn Fn() + Send + Sync>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    std::thread::Builder::new()
        .name("own-screen".into())
        .spawn(move || {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            let kind = DesktopCaptureSourceType::Generic;
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            let kind = DesktopCaptureSourceType::Screen;
            let Some(mut capturer) = DesktopCapturer::new(DesktopCapturerOptions::new(kind)) else {
                log::warn!("own screen: no capturer on this desktop");
                return;
            };
            let mut shown = 0u32;
            capturer.start_capture(None, move |result| {
                let Ok(frame) = result else {
                    return;
                };
                let (w, h) = (frame.width() as u32, frame.height() as u32);
                if w == 0 || h == 0 {
                    return;
                }
                // The capturer's pictures are B, G, R, A in memory: libyuv's
                // "ARGB".
                let mut buffer = I420Buffer::new(w, h);
                let (sy, su, sv) = buffer.strides();
                let (y, u, v) = buffer.data_mut();
                yuv_helper::argb_to_i420(
                    frame.data(),
                    frame.stride(),
                    y,
                    sy,
                    u,
                    su,
                    v,
                    sv,
                    w as i32,
                    h as i32,
                );
                if w > SCREEN_WIDTH {
                    let height = (h * SCREEN_WIDTH / w) & !1;
                    buffer = buffer.scale(SCREEN_WIDTH as i32, height as i32);
                }
                // The user's own tile: a small picture once a second.
                if shown.is_multiple_of(SCREEN_FPS) {
                    let (bw, bh) = (buffer.width() as i32, buffer.height() as i32);
                    let small = buffer.scale(PREVIEW_WIDTH, (bh * PREVIEW_WIDTH / bw) & !1);
                    if let Some(picture) = rgba(&small) {
                        videos.show(me, Kind::Screen, picture);
                        wake();
                    }
                }
                shown = shown.wrapping_add(1);
                source.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, buffer));
            });
            while !stop.load(Ordering::Relaxed) {
                capturer.capture_frame();
                std::thread::sleep(Duration::from_millis(1000 / u64::from(SCREEN_FPS)));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// RGBA pictures into the track.
fn send(source: &NativeVideoSource, rgba: &[u8], w: u32, h: u32) {
    let mut buffer = I420Buffer::new(w, h);
    let (sy, su, sv) = buffer.strides();
    let (y, u, v) = buffer.data_mut();
    yuv_helper::abgr_to_i420(rgba, w * 4, y, sy, u, su, v, sv, w as i32, h as i32);
    source.capture_frame(&VideoFrame::new(VideoRotation::VideoRotation0, buffer));
}
