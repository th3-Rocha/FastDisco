//! Discofast's additions, for Discord's voice: encoded frames through Rust
//! (DAVE), a remote track's volume, and a tap on the platform audio
//! device. See include/livekit/discofast.h.

use std::sync::{Arc, RwLock};

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    extern "C++" {
        include!("livekit/rtp_sender.h");
        include!("livekit/rtp_receiver.h");
        include!("livekit/media_stream_track.h");

        type RtpSender = crate::rtp_sender::ffi::RtpSender;
        type RtpReceiver = crate::rtp_receiver::ffi::RtpReceiver;
        type MediaStreamTrack = crate::media_stream_track::ffi::MediaStreamTrack;
    }

    unsafe extern "C++" {
        include!("livekit/discofast.h");

        fn set_sender_transformer(sender: &RtpSender, context: u64);
        fn set_receiver_transformer(receiver: &RtpReceiver, context: u64);
        fn set_audio_track_volume(track: &MediaStreamTrack, volume: f64);
    }

    extern "Rust" {
        fn discofast_transform(
            context: u64,
            encrypt: bool,
            ssrc: u32,
            data: &[u8],
            out: &mut Vec<u8>,
        ) -> bool;
        fn discofast_recorded(samples: &mut [i16], channels: usize, rate: u32);
        fn discofast_playout(samples: &mut [i16], channels: usize, rate: u32);
    }
}

/// Transforms one encoded frame: `(context, encrypt, ssrc, frame, out)`;
/// false drops the frame.
pub type Transform = dyn Fn(u64, bool, u32, &[u8], &mut Vec<u8>) -> bool + Send + Sync;
/// Interleaved 16-bit sound, its channels and rate.
pub type Tap = dyn Fn(&mut [i16], usize, u32) + Send + Sync;

static TRANSFORM: RwLock<Option<Arc<Transform>>> = RwLock::new(None);
static RECORDED: RwLock<Option<Arc<Tap>>> = RwLock::new(None);
static PLAYOUT: RwLock<Option<Arc<Tap>>> = RwLock::new(None);

pub fn set_transform(f: Option<Arc<Transform>>) {
    *TRANSFORM.write().unwrap_or_else(|e| e.into_inner()) = f;
}

pub fn set_recorded(f: Option<Arc<Tap>>) {
    *RECORDED.write().unwrap_or_else(|e| e.into_inner()) = f;
}

pub fn set_playout(f: Option<Arc<Tap>>) {
    *PLAYOUT.write().unwrap_or_else(|e| e.into_inner()) = f;
}

fn discofast_transform(
    context: u64,
    encrypt: bool,
    ssrc: u32,
    data: &[u8],
    out: &mut Vec<u8>,
) -> bool {
    let f = TRANSFORM.read().unwrap_or_else(|e| e.into_inner()).clone();
    match f {
        Some(f) => f(context, encrypt, ssrc, data, out),
        None => {
            out.extend_from_slice(data);
            true
        }
    }
}

fn discofast_recorded(samples: &mut [i16], channels: usize, rate: u32) {
    if let Some(f) = RECORDED.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        f(samples, channels, rate);
    }
}

fn discofast_playout(samples: &mut [i16], channels: usize, rate: u32) {
    if let Some(f) = PLAYOUT.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        f(samples, channels, rate);
    }
}
