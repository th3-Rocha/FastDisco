//! Frames into pictures: one `ffmpeg` per watched share, fed the frames
//! on its input and giving RGBA pictures of a fixed size on its output,
//! each on a thread of its own so neither the network nor the interface
//! waits on it.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};

use super::rtp::Codec;
use super::{Frame, Screen};

/// Every picture comes out this size, letterboxed, so the output can be
/// read in whole pictures without asking ffmpeg what it found.
pub const WIDTH: usize = 1280;
pub const HEIGHT: usize = 720;

pub struct Decoder {
    child: Child,
    frames: SyncSender<Vec<u8>>,
}

impl Decoder {
    pub fn start(
        codec: Codec,
        screen: Arc<Screen>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, String> {
        let input = match codec {
            Codec::H264 => "h264",
            Codec::Vp8 => "ivf",
        };
        let filter = format!(
            "scale={WIDTH}:{HEIGHT}:force_original_aspect_ratio=decrease,\
             pad={WIDTH}:{HEIGHT}:(ow-iw)/2:(oh-ih)/2"
        );
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-fflags",
                "nobuffer",
                "-flags",
                "low_delay",
                "-probesize",
                "32",
                "-analyzeduration",
                "0",
                "-f",
                input,
                "-i",
                "pipe:0",
                "-an",
                "-vf",
                &filter,
                "-fps_mode",
                "passthrough",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "ffmpeg não está instalado.".to_owned())?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let mut stdout = child.stdout.take().expect("piped stdout");
        // A few frames of slack; past that the newest are dropped rather
        // than the delay growing.
        let (frames, incoming) = sync_channel::<Vec<u8>>(8);
        std::thread::Builder::new()
            .name("stream-feed".into())
            .spawn(move || {
                if codec == Codec::Vp8 && stdin.write_all(&ivf_header()).is_err() {
                    return;
                }
                for (index, frame) in (0u64..).zip(incoming) {
                    let written = match codec {
                        Codec::H264 => stdin.write_all(&frame),
                        Codec::Vp8 => stdin
                            .write_all(&ivf_frame_header(frame.len(), index))
                            .and_then(|()| stdin.write_all(&frame)),
                    };
                    if written.and_then(|()| stdin.flush()).is_err() {
                        return;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("stream-picture".into())
            .spawn(move || {
                let mut buffer = vec![0u8; WIDTH * HEIGHT * 4];
                while stdout.read_exact(&mut buffer).is_ok() {
                    screen.show(Frame {
                        width: WIDTH,
                        height: HEIGHT,
                        rgba: buffer.clone(),
                    });
                    wake();
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { child, frames })
    }

    /// Hands a whole frame to ffmpeg. False once ffmpeg is gone.
    pub fn feed(&self, frame: Vec<u8>) -> bool {
        !matches!(
            self.frames.try_send(frame),
            Err(TrySendError::Disconnected(_))
        )
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The 32-byte IVF file header ffmpeg reads VP8 frames after.
fn ivf_header() -> Vec<u8> {
    let mut h = b"DKIF".to_vec();
    h.extend_from_slice(&0u16.to_le_bytes());
    h.extend_from_slice(&32u16.to_le_bytes());
    h.extend_from_slice(b"VP80");
    h.extend_from_slice(&(WIDTH as u16).to_le_bytes());
    h.extend_from_slice(&(HEIGHT as u16).to_le_bytes());
    // A 30 fps time base; the pictures are shown as they come anyway.
    h.extend_from_slice(&30u32.to_le_bytes());
    h.extend_from_slice(&1u32.to_le_bytes());
    h.extend_from_slice(&0u32.to_le_bytes());
    h.extend_from_slice(&0u32.to_le_bytes());
    h
}

fn ivf_frame_header(size: usize, index: u64) -> [u8; 12] {
    let mut h = [0u8; 12];
    h[..4].copy_from_slice(&(size as u32).to_le_bytes());
    h[4..].copy_from_slice(&index.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ivf_header_is_32_bytes_and_names_vp8() {
        let h = ivf_header();
        assert_eq!(h.len(), 32);
        assert_eq!(&h[8..12], b"VP80");
        assert_eq!(ivf_frame_header(5, 2)[..4], 5u32.to_le_bytes());
    }
}
