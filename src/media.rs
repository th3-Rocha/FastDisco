//! In-app playback for videos, audio, voice messages and YouTube links.
//!
//! One thing plays at a time. A worker thread asks `ffprobe` what the
//! source holds, then runs `ffmpeg` twice: once for RGBA frames scaled to the
//! chat, once for 48 kHz stereo samples that rodio plays. The sound's
//! position is the clock; the interface shows the newest frame that is due.
//! YouTube links go through `yt-dlp` first for a direct stream. Without
//! ffmpeg, the caller opens the link in the browser instead.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use egui::{ColorImage, TextureHandle, TextureOptions};

/// Widest frame decoded, about the widest the chat shows.
const MAX_WIDTH: u32 = 640;
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
/// Sound decoded before playback starts, so the clock does not run ahead
/// of a sound that has not arrived.
const PREBUFFER: usize = (RATE as usize) * (CHANNELS as usize) / 4;

/// Whether ffmpeg can be used at all.
pub fn available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        ["ffmpeg", "ffprobe"].iter().all(|tool| {
            Command::new(tool)
                .arg("-version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        })
    })
}

pub fn has_ytdlp() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("yt-dlp")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Loading,
    Playing,
    Paused,
    Ended,
    Failed(String),
}

struct Probe {
    width: u32,
    height: u32,
    fps: f64,
    duration: Option<f64>,
    video: bool,
    audio: bool,
}

/// Where the picture and the sound come from: one file, or a video
/// stream and an audio stream (YouTube serves them apart).
#[derive(Clone, Debug)]
struct Source {
    video: String,
    audio: String,
}

enum Message {
    Ready { probe: Probe, source: Source },
    Failed(String),
}

type Frame = (f64, ColorImage);

pub struct Player {
    /// What it plays, as the view knows it: an attachment or embed URL.
    pub key: String,
    pub state: State,
    pub duration: Option<f64>,
    pub size: Option<[u32; 2]>,
    pub has_video: bool,
    pub muted: bool,
    pub volume: f32,
    source: Option<Source>,
    probe_fps: f64,
    has_audio: bool,
    /// Where the current decoders started.
    offset: f64,
    /// For sources without sound: when the clock last started, and the
    /// time already played before it.
    started: Option<Instant>,
    played: f64,
    position: f64,
    messages: Receiver<Message>,
    worker: Option<Worker>,
    audio: Option<Audio>,
    next_frame: Option<Frame>,
    /// Stay paused once a seek made while paused has loaded.
    hold: bool,
    pub texture: Option<TextureHandle>,
    ctx: egui::Context,
}

struct Worker {
    children: Vec<Child>,
    frames: Option<Receiver<Frame>>,
    /// The sound decoder has a head start buffered.
    primed: Arc<AtomicBool>,
    /// Each decoder has reached the end.
    video_done: Arc<AtomicBool>,
    audio_done: Arc<AtomicBool>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

struct Audio {
    _stream: rodio::OutputStream,
    sink: rodio::Sink,
}

impl Player {
    /// Starts playing `source` at once. A `ytdl:` prefix resolves the rest
    /// with yt-dlp.
    pub fn start(ctx: &egui::Context, key: String, source: String) -> Self {
        let (sender, messages) = std::sync::mpsc::sync_channel(8);
        let player = Self {
            key,
            state: State::Loading,
            duration: None,
            size: None,
            has_video: false,
            muted: false,
            volume: 1.0,
            source: None,
            probe_fps: 30.0,
            has_audio: false,
            offset: 0.0,
            started: None,
            played: 0.0,
            position: 0.0,
            messages,
            worker: None,
            audio: None,
            next_frame: None,
            hold: false,
            texture: None,
            ctx: ctx.clone(),
        };
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("media-probe".into())
            .spawn(move || {
                let result = resolve(&source).and_then(|source| {
                    let mut probed = probe(&source.video)?;
                    if source.audio != source.video {
                        probed.audio = true;
                    }
                    Ok((probed, source))
                });
                let message = match result {
                    Ok((probe, source)) => Message::Ready { probe, source },
                    Err(error) => Message::Failed(error),
                };
                let _ = sender.send(message);
                ctx.request_repaint();
            })
            .expect("the probe thread starts");
        player
    }

    /// The playback position in seconds.
    pub fn position(&self) -> f64 {
        self.position
    }

    pub fn toggle(&mut self) {
        match self.state {
            State::Playing => self.pause(),
            State::Paused => self.resume(),
            State::Ended => self.seek(0.0),
            _ => {}
        }
    }

    fn pause(&mut self) {
        self.state = State::Paused;
        if let Some(audio) = &self.audio {
            audio.sink.pause();
        }
        if let Some(started) = self.started.take() {
            self.played += started.elapsed().as_secs_f64();
        }
    }

    fn resume(&mut self) {
        self.state = State::Playing;
        if let Some(audio) = &self.audio {
            audio.sink.play();
        } else {
            self.started = Some(Instant::now());
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.apply_volume();
    }

    fn apply_volume(&self) {
        if let Some(audio) = &self.audio {
            audio
                .sink
                .set_volume(if self.muted { 0.0 } else { self.volume });
        }
    }

    /// Restarts the decoders at `seconds`.
    pub fn seek(&mut self, seconds: f64) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let seconds = seconds.clamp(0.0, self.duration.unwrap_or(f64::MAX).max(0.0));
        let resume_paused = self.state == State::Paused;
        self.worker = None;
        self.audio = None;
        self.next_frame = None;
        self.offset = seconds;
        self.position = seconds;
        self.played = 0.0;
        self.started = None;
        let probe = Probe {
            width: self.size.map_or(0, |s| s[0]),
            height: self.size.map_or(0, |s| s[1]),
            fps: self.probe_fps,
            duration: self.duration,
            video: self.has_video,
            audio: self.has_audio,
        };
        match decode(&self.ctx, &source, &probe, seconds) {
            Ok((worker, samples)) => {
                if let Some(samples) = samples {
                    self.open_audio(samples);
                }
                self.worker = Some(worker);
                self.state = State::Loading;
                self.hold = resume_paused;
            }
            Err(error) => self.state = State::Failed(error),
        }
    }

    /// Takes decoded frames and starts the clock; call every frame the
    /// player shows.
    pub fn update(&mut self) {
        match self.messages.try_recv() {
            Ok(Message::Ready { probe, source }) => {
                self.duration = probe.duration;
                self.has_video = probe.video;
                self.has_audio = probe.audio;
                self.probe_fps = probe.fps;
                self.size = probe.video.then_some([probe.width, probe.height]);
                self.source = Some(source);
                self.seek(0.0);
            }
            Ok(Message::Failed(error)) => self.state = State::Failed(error),
            Err(_) => {}
        }
        let Some(worker) = &self.worker else { return };
        if self.next_frame.is_none()
            && let Some(frames) = &worker.frames
        {
            self.next_frame = frames.try_recv().ok();
        }
        // Start once there is something to show and hear.
        if self.state == State::Loading {
            let sound_ready = !self.has_audio
                || worker.primed.load(Ordering::Acquire)
                || worker.audio_done.load(Ordering::Acquire);
            let picture_ready = !self.has_video
                || self.next_frame.is_some()
                || worker.video_done.load(Ordering::Acquire);
            if sound_ready && picture_ready {
                if std::mem::take(&mut self.hold) {
                    self.state = State::Paused;
                } else {
                    self.state = State::Playing;
                    match &self.audio {
                        Some(audio) => audio.sink.play(),
                        None => self.started = Some(Instant::now()),
                    }
                }
            }
        }
        self.position = self.clock();
        // Skip frames that are already late, then show the one that is due.
        if let Some(frames) = self.worker.as_ref().and_then(|w| w.frames.as_ref()) {
            while self
                .next_frame
                .as_ref()
                .is_some_and(|(pts, _)| *pts + 0.1 < self.position)
            {
                match frames.try_recv() {
                    Ok(frame) => self.next_frame = Some(frame),
                    Err(_) => break,
                }
            }
        }
        self.show_due();
        if let Some(frames) = self.worker.as_ref().and_then(|w| w.frames.as_ref())
            && self.next_frame.is_none()
        {
            self.next_frame = frames.try_recv().ok();
        }
        let worker = self.worker.as_ref().expect("checked above");
        let video_over = !self.has_video
            || (worker.video_done.load(Ordering::Acquire) && self.next_frame.is_none());
        let audio_over = self.audio.as_ref().is_none_or(|a| a.sink.empty());
        if self.state == State::Playing && video_over && audio_over {
            self.state = State::Ended;
            if let Some(duration) = self.duration {
                self.position = duration;
            }
        }
        if matches!(self.state, State::Playing | State::Loading) {
            let wait = self
                .next_frame
                .as_ref()
                .map_or(Duration::from_millis(100), |(pts, _)| {
                    Duration::from_secs_f64((pts - self.position).clamp(0.005, 0.1))
                });
            self.ctx.request_repaint_after(wait);
        }
    }

    fn open_audio(&mut self, samples: Receiver<Vec<f32>>) {
        let Ok(mut stream) = rodio::OutputStreamBuilder::open_default_stream() else {
            // No sound device: play silently on the wall clock.
            self.has_audio = false;
            return;
        };
        stream.log_on_drop(false);
        let sink = rodio::Sink::connect_new(stream.mixer());
        sink.pause();
        sink.append(Pcm {
            samples,
            buffer: Vec::new(),
            at: 0,
        });
        sink.set_volume(if self.muted { 0.0 } else { self.volume });
        self.audio = Some(Audio {
            _stream: stream,
            sink,
        });
    }

    fn clock(&self) -> f64 {
        match &self.audio {
            Some(audio) => self.offset + audio.sink.get_pos().as_secs_f64(),
            None => {
                self.offset + self.played + self.started.map_or(0.0, |s| s.elapsed().as_secs_f64())
            }
        }
    }

    /// Uploads the newest frame that is due.
    fn show_due(&mut self) {
        let due = self
            .next_frame
            .as_ref()
            .is_some_and(|(pts, _)| *pts <= self.position + 0.01)
            || (self.texture.is_none() && self.next_frame.is_some());
        if !due {
            return;
        }
        let Some((_, image)) = self.next_frame.take() else {
            return;
        };
        match &mut self.texture {
            Some(texture) => texture.set(image, TextureOptions::LINEAR),
            None => {
                self.texture = Some(self.ctx.load_texture(
                    "media-frame",
                    image,
                    TextureOptions::LINEAR,
                ))
            }
        }
    }
}

/// Plays samples as they come; silence while the decoder catches up.
struct Pcm {
    samples: Receiver<Vec<f32>>,
    buffer: Vec<f32>,
    at: usize,
}

impl Iterator for Pcm {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.at >= self.buffer.len() {
            match self.samples.try_recv() {
                Ok(next) => {
                    self.buffer = next;
                    self.at = 0;
                }
                Err(TryRecvError::Empty) => return Some(0.0),
                Err(TryRecvError::Disconnected) => return None,
            }
        }
        let sample = self.buffer.get(self.at).copied();
        self.at += 1;
        sample.or(Some(0.0))
    }
}

impl rodio::Source for Pcm {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        CHANNELS
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Direct streams for `ytdl:` sources; anything else as it is.
fn resolve(source: &str) -> Result<Source, String> {
    let Some(page) = source.strip_prefix("ytdl:") else {
        return Ok(Source {
            video: source.to_owned(),
            audio: source.to_owned(),
        });
    };
    // H.264 and AAC up to 480p, as two streams or one file.
    let format =
        "bv*[height<=480][vcodec^=avc1]+ba[acodec^=mp4a]/bv*[height<=480]+ba/b[height<=480]/b";
    let output = Command::new("yt-dlp")
        .args(["-f", format, "-g", "--no-playlist", "--no-warnings", page])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "yt-dlp não está instalado.".to_owned())?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut urls = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("http"));
    let video = urls
        .next()
        .ok_or_else(|| "Não foi possível abrir este vídeo.".to_owned())?
        .to_owned();
    let audio = urls.next().map_or_else(|| video.clone(), str::to_owned);
    Ok(Source { video, audio })
}

fn probe(source: &str) -> Result<Probe, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height,avg_frame_rate:format=duration",
            "-of",
            "json",
            source,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "ffmpeg não está instalado.".to_owned())?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "Arquivo de mídia ilegível.".to_owned())?;
    let streams = value["streams"].as_array().cloned().unwrap_or_default();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["width"].as_u64().unwrap_or(0) > 0);
    let audio = streams.iter().any(|s| s["codec_type"] == "audio");
    if video.is_none() && !audio {
        return Err("Arquivo de mídia ilegível.".into());
    }
    let (width, height) = video.map_or((0, 0), |v| {
        let w = v["width"].as_u64().unwrap_or(0) as u32;
        let h = v["height"].as_u64().unwrap_or(0) as u32;
        let scaled_w = w.min(MAX_WIDTH) & !1;
        let scaled_h = ((h as f64 * scaled_w as f64 / w.max(1) as f64).round() as u32) & !1;
        (scaled_w.max(2), scaled_h.max(2))
    });
    let fps = video
        .and_then(|v| v["avg_frame_rate"].as_str())
        .and_then(|r| {
            let (n, d) = r.split_once('/')?;
            let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
            (d > 0.0 && n > 0.0).then_some(n / d)
        })
        .unwrap_or(30.0)
        .clamp(1.0, 60.0);
    let duration = value["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse().ok());
    Ok(Probe {
        width,
        height,
        fps,
        duration,
        video: video.is_some(),
        audio,
    })
}

fn decode(
    ctx: &egui::Context,
    source: &Source,
    probe: &Probe,
    start: f64,
) -> Result<(Worker, Option<Receiver<Vec<f32>>>), String> {
    let mut worker = Worker {
        children: Vec::new(),
        frames: None,
        primed: Arc::new(AtomicBool::new(false)),
        video_done: Arc::new(AtomicBool::new(!probe.video)),
        audio_done: Arc::new(AtomicBool::new(!probe.audio)),
    };
    let seek = format!("{start:.3}");
    if probe.video {
        let filter = format!(
            "scale={}:{},fps={}",
            probe.width,
            probe.height,
            probe.fps.min(30.0)
        );
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-ss",
                &seek,
                "-i",
                &source.video,
                "-an",
                "-vf",
                &filter,
            ])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "ffmpeg não está instalado.".to_owned())?;
        let mut out = child.stdout.take().expect("piped stdout");
        let (w, h) = (probe.width as usize, probe.height as usize);
        let fps = probe.fps.min(30.0);
        let (sender, frames) = std::sync::mpsc::sync_channel::<Frame>(6);
        let done = worker.video_done.clone();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("media-video".into())
            .spawn(move || {
                let mut buffer = vec![0u8; w * h * 4];
                let mut n = 0u64;
                while out.read_exact(&mut buffer).is_ok() {
                    let image = ColorImage::from_rgba_unmultiplied([w, h], &buffer);
                    let pts = start + n as f64 / fps;
                    n += 1;
                    if sender.send((pts, image)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
                done.store(true, Ordering::Release);
                ctx.request_repaint();
            })
            .expect("the video thread starts");
        worker.frames = Some(frames);
        worker.children.push(child);
    }
    let mut samples = None;
    if probe.audio {
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-ss",
                &seek,
                "-i",
                &source.audio,
                "-vn",
            ])
            .args(["-f", "f32le", "-ac", "2", "-ar", "48000", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "ffmpeg não está instalado.".to_owned())?;
        let mut out = child.stdout.take().expect("piped stdout");
        // About three seconds ahead; the decoder waits when it is full.
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Vec<f32>>(36);
        samples = Some(receiver);
        let primed = worker.primed.clone();
        let done = worker.audio_done.clone();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("media-audio".into())
            .spawn(move || {
                let mut bytes = vec![0u8; 4096 * 4];
                let mut sent = 0;
                loop {
                    let mut filled = 0;
                    while filled < bytes.len() {
                        match out.read(&mut bytes[filled..]) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => filled += n,
                        }
                    }
                    if filled < 4 {
                        break;
                    }
                    let chunk: Vec<f32> = bytes[..filled - filled % 4]
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    sent += chunk.len();
                    if sender.send(chunk).is_err() {
                        break;
                    }
                    if sent >= PREBUFFER && !primed.swap(true, Ordering::AcqRel) {
                        ctx.request_repaint();
                    }
                }
                primed.store(true, Ordering::Release);
                done.store(true, Ordering::Release);
                ctx.request_repaint();
            })
            .expect("the audio thread starts");
        worker.children.push(child);
    }
    Ok((worker, samples))
}

/// "1:05", "1:02:03".
pub fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays a generated clip through the real pipeline, muted. Needs
    /// ffmpeg: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn plays_a_clip_to_the_end() {
        let dir = std::env::temp_dir().join(format!("fastdisco-media-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("clip.mp4");
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=25:duration=2",
            ])
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-shortest",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(status.success());
        let ctx = egui::Context::default();
        let mut player = Player::start(&ctx, "k".into(), clip.to_string_lossy().into_owned());
        player.muted = true;
        let started = Instant::now();
        let mut shown = false;
        while player.state != State::Ended && started.elapsed() < Duration::from_secs(10) {
            player.update();
            shown |= player.texture.is_some();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            player.state,
            State::Ended,
            "did not finish: {:?}",
            player.state
        );
        assert!(player.has_video && player.size == Some([320, 240]));
        assert!(shown);
        let took = started.elapsed().as_secs_f64();
        assert!(took > 1.5 && took < 6.0, "a two second clip took {took}s");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Resolves and plays a YouTube video for a second. Needs network,
    /// yt-dlp and ffmpeg: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn plays_youtube() {
        let ctx = egui::Context::default();
        let mut player = Player::start(
            &ctx,
            "yt".into(),
            "ytdl:https://www.youtube.com/watch?v=jNQXAC9IVRw".into(),
        );
        player.muted = true;
        let started = Instant::now();
        while player.position() < 1.0 && started.elapsed() < Duration::from_secs(40) {
            player.update();
            if let State::Failed(error) = &player.state {
                panic!("{error}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            player.has_video && player.texture.is_some(),
            "{:?}",
            player.state
        );
        assert!(player.position() >= 1.0);
    }
}

/// A picture's main colour, as Discord takes an avatar's for its call
/// tile: the most common vivid shade, darkened enough for white text.
pub fn main_color(bytes: &[u8]) -> Option<[u8; 3]> {
    let picture = image::load_from_memory(bytes)
        .ok()?
        .thumbnail(32, 32)
        .to_rgba8();
    // Shades in 4-bit buckets, weighed by how vivid they are.
    let mut buckets: std::collections::HashMap<u16, (f32, [f32; 3])> = Default::default();
    for p in picture.pixels() {
        let [r, g, b, a] = p.0;
        if a < 128 {
            continue;
        }
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let saturation = if max == 0 {
            0.0
        } else {
            f32::from(max - min) / f32::from(max)
        };
        let light = f32::from(max) / 255.0;
        // Near black and near white say little about a picture.
        let weight = 0.15 + saturation * (0.2 + light);
        let key = (u16::from(r >> 4) << 8) | (u16::from(g >> 4) << 4) | u16::from(b >> 4);
        let entry = buckets.entry(key).or_insert((0.0, [0.0; 3]));
        entry.0 += weight;
        entry.1[0] += f32::from(r) * weight;
        entry.1[1] += f32::from(g) * weight;
        entry.1[2] += f32::from(b) * weight;
    }
    let (weight, sum) = buckets.into_values().max_by(|a, b| a.0.total_cmp(&b.0))?;
    let mut color = sum.map(|c| c / weight);
    // Bright shades come down so the name on the tile stays readable.
    let luma = 0.299 * color[0] + 0.587 * color[1] + 0.114 * color[2];
    if luma > 110.0 {
        let k = 110.0 / luma;
        color = color.map(|c| c * k);
    }
    Some(color.map(|c| c.round().clamp(0.0, 255.0) as u8))
}

#[cfg(test)]
mod color_tests {
    use super::main_color;

    fn png(f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let img = image::RgbaImage::from_fn(64, 64, |x, y| image::Rgba(f(x, y)));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn a_dark_picture_with_a_purple_hood_is_purple() {
        // Mostly near black, a third purple.
        let bytes = png(|x, _| {
            if x < 22 {
                [90, 40, 120, 255]
            } else {
                [12, 12, 14, 255]
            }
        });
        let [r, g, b] = main_color(&bytes).unwrap();
        assert!(r > g + 20 && b > g + 40, "{r} {g} {b}");
    }

    #[test]
    fn white_backgrounds_lose_to_the_drawing_and_light_colours_come_down() {
        let bytes = png(|x, y| {
            if (x / 16 + y / 16) % 3 == 0 {
                [230, 60, 160, 255]
            } else {
                [250, 250, 250, 255]
            }
        });
        let [r, g, b] = main_color(&bytes).unwrap();
        assert!(r > g + 40 && b > g, "{r} {g} {b}");
        let luma = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
        assert!(luma <= 111.0, "{luma}");
    }
}
