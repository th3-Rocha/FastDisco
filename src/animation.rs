//! GIFs that arrive as videos (`gifv` embeds from Imgur, Klipy, Discord's
//! own picker...) play the way Discord plays them: at once, silent and in
//! a loop. ffmpeg decodes each one a single time into small frames that
//! loop from memory; an animation unseen for a while is dropped.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use egui::{ColorImage, TextureHandle, TextureOptions};

/// Widest frame kept, about the widest a GIF is shown.
const MAX_WIDTH: u32 = 400;
const MAX_FPS: f64 = 30.0;
/// Frames kept per animation: ten seconds at 30 fps.
const MAX_FRAMES: usize = 300;
/// An animation not drawn for this long is dropped from memory.
const IDLE: Duration = Duration::from_secs(20);
/// Decoders running at once.
const DECODERS: usize = 2;

enum Load {
    Waiting,
    Decoding(Receiver<Decoded>),
    Ready,
    Failed,
}

enum Decoded {
    Fps(f64),
    Frame(ColorImage),
    Done,
}

struct Animation {
    load: Load,
    frames: Vec<TextureHandle>,
    fps: f64,
    started: Instant,
    seen: Instant,
}

#[derive(Default)]
pub struct Animations {
    list: HashMap<String, Animation>,
}

impl Animations {
    /// The frame of `url` to show now, starting its decoder the first
    /// time. `None` until the first frame is ready.
    pub fn frame(&mut self, ctx: &egui::Context, url: &str) -> Option<TextureHandle> {
        let now = Instant::now();
        let entry = self
            .list
            .entry(url.to_owned())
            .or_insert_with(|| Animation {
                load: Load::Waiting,
                frames: Vec::new(),
                fps: 15.0,
                started: now,
                seen: now,
            });
        entry.seen = now;
        if entry.frames.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
            return None;
        }
        let elapsed = now.duration_since(entry.started).as_secs_f64();
        let index = (elapsed * entry.fps) as usize % entry.frames.len();
        let next = ((elapsed * entry.fps).floor() + 1.0) / entry.fps - elapsed;
        ctx.request_repaint_after(Duration::from_secs_f64(next.max(0.005)));
        Some(entry.frames[index].clone())
    }

    /// Starts waiting decoders, takes decoded frames and drops idle
    /// animations. Call once per frame.
    pub fn update(&mut self, ctx: &egui::Context) {
        if !crate::media::available() {
            return;
        }
        self.list.retain(|_, a| a.seen.elapsed() < IDLE);
        let running = self
            .list
            .values()
            .filter(|a| matches!(a.load, Load::Decoding(_)))
            .count();
        let mut free = DECODERS.saturating_sub(running);
        for (url, animation) in &mut self.list {
            if free > 0 && matches!(animation.load, Load::Waiting) {
                animation.load = Load::Decoding(decode(ctx, url.clone()));
                free -= 1;
            }
            let Load::Decoding(receiver) = &animation.load else {
                continue;
            };
            let mut done = None;
            loop {
                match receiver.try_recv() {
                    Ok(Decoded::Fps(fps)) => animation.fps = fps,
                    Ok(Decoded::Frame(image)) => {
                        let name = format!("animation-{}", animation.frames.len());
                        animation.frames.push(ctx.load_texture(
                            name,
                            image,
                            TextureOptions::LINEAR,
                        ));
                        if animation.frames.len() == 1 {
                            animation.started = Instant::now();
                        }
                    }
                    Ok(Decoded::Done) | Err(TryRecvError::Disconnected) => {
                        done = Some(if animation.frames.is_empty() {
                            Load::Failed
                        } else {
                            Load::Ready
                        });
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
            if let Some(load) = done {
                animation.load = load;
            }
        }
    }

    /// Whether `url` could not be decoded, so the caller shows a poster.
    pub fn failed(&self, url: &str) -> bool {
        self.list
            .get(url)
            .is_some_and(|a| matches!(a.load, Load::Failed))
    }
}

fn decode(ctx: &egui::Context, url: String) -> Receiver<Decoded> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("animation".into())
        .spawn(move || {
            let Some((width, height, fps)) = probe(&url) else {
                let _ = sender.send(Decoded::Done);
                return;
            };
            let _ = sender.send(Decoded::Fps(fps));
            let filter = format!("scale={width}:{height},fps={fps}");
            let Ok(mut child) = Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-nostdin",
                    "-i",
                    &url,
                    "-an",
                    "-vf",
                    &filter,
                ])
                .args([
                    "-frames:v",
                    &MAX_FRAMES.to_string(),
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "rgba",
                    "-",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                let _ = sender.send(Decoded::Done);
                return;
            };
            let mut out = child.stdout.take().expect("piped stdout");
            let (w, h) = (width as usize, height as usize);
            let mut buffer = vec![0u8; w * h * 4];
            while out.read_exact(&mut buffer).is_ok() {
                let image = ColorImage::from_rgba_unmultiplied([w, h], &buffer);
                if sender.send(Decoded::Frame(image)).is_err() {
                    let _ = child.kill();
                    break;
                }
                ctx.request_repaint();
            }
            let _ = child.wait();
            let _ = sender.send(Decoded::Done);
            ctx.request_repaint();
        })
        .expect("the animation thread starts");
    receiver
}

/// Frame size (scaled down, even) and rate of the video at `url`.
fn probe(url: &str) -> Option<(u32, u32, f64)> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate",
            "-of",
            "json",
            url,
        ])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let stream = value["streams"].get(0)?;
    let (w, h) = (
        stream["width"].as_u64()? as u32,
        stream["height"].as_u64()? as u32,
    );
    if w == 0 || h == 0 {
        return None;
    }
    let width = w.min(MAX_WIDTH) & !1;
    let height = ((h as f64 * width as f64 / w as f64).round() as u32 & !1).max(2);
    let fps = stream["avg_frame_rate"]
        .as_str()
        .and_then(|r| {
            let (n, d) = r.split_once('/')?;
            let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
            (n > 0.0 && d > 0.0).then_some(n / d)
        })
        .unwrap_or(15.0)
        .clamp(1.0, MAX_FPS);
    Some((width.max(2), height, fps))
}
