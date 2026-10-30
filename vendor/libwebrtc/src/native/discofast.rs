//! Discofast's additions for Discord's voice; see webrtc-sys's discofast.

use std::sync::Arc;

use webrtc_sys::discofast as sys;

pub use sys::{Tap, Transform};

use crate::audio_track::RtcAudioTrack;
use crate::rtp_receiver::RtpReceiver;
use crate::rtp_sender::RtpSender;

/// Every encoded frame `sender` sends passes through the transform first,
/// with `context` and `encrypt` true.
pub fn set_sender_transformer(sender: &RtpSender, context: u64) {
    sys::ffi::set_sender_transformer(&sender.handle.sys_handle, context);
}

/// Every encoded frame `receiver` gets passes through the transform before
/// it is decoded, with `context` and `encrypt` false.
pub fn set_receiver_transformer(receiver: &RtpReceiver, context: u64) {
    sys::ffi::set_receiver_transformer(&receiver.handle.sys_handle, context);
}

/// A remote track's volume, 0 to 10 (1 as received).
pub fn set_audio_track_volume(track: &RtcAudioTrack, volume: f64) {
    sys::ffi::set_audio_track_volume(&track.handle.sys_handle(), volume);
}

pub fn set_transform(f: Option<Arc<Transform>>) {
    sys::set_transform(f);
}

/// What the platform microphone captured, before the voice engine.
pub fn set_recorded(f: Option<Arc<Tap>>) {
    sys::set_recorded(f);
}

/// What the platform speakers play, after the voice engine.
pub fn set_playout(f: Option<Arc<Tap>>) {
    sys::set_playout(f);
}
