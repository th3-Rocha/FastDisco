//! A voice call's sound on this machine: the microphone into a ring the
//! voice driver reads, and what the others say from a ring into the
//! speakers. Both at Discord's 48 kHz. The streams stay on the interface
//! thread (they cannot move between threads on every platform); the rings
//! are shared with the driver in the backend.
//!
//! The way through, end to end:
//! microphone (cpal, any rate) → 48 kHz mono → `Shared::mic` →
//! `MicSource` in songbird's mixer: echo canceller → noise suppression →
//! level control → voice gate → songbird's Opus encoder → Discord.
//! Discord → songbird (decrypt, DAVE, playout buffer) → `discord::call`
//! decodes each person's Opus, conceals lost packets, mixes →
//! `Shared::speakers` → speakers (cpal, any rate), whose sound also goes
//! back to the echo canceller through `Shared::played`.
//!
//! `DISCOFAST_AUDIO_DEBUG=<folder>` records three WAVs there for each call:
//! the microphone as the device gives it, the sound after the filter, and
//! what was received after decoding.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

use crate::model::Id;

/// Discord's voice rate.
pub const RATE: u32 = 48_000;
/// Interleaved stereo.
const CHANNELS: usize = 2;
/// Ten milliseconds of one channel: the filter's step.
const CHUNK: usize = RATE as usize / 100;
/// Twenty milliseconds of one channel: one Opus frame, what songbird reads.
const BLOCK: usize = CHUNK * 2;
/// The most a ring keeps before its reader skips ahead: a fifth of a
/// second, so the delay never grows.
const RING_SECONDS: f32 = 0.2;
/// The most the two clocks a ring sits between are bent to meet: two
/// samples in a block, 0.2%, which nobody hears.
const DRIFT: f32 = 0.002;

/// Samples handed from one thread to another without a lock, for one
/// writer and one reader: the sound card's callback never waits and never
/// allocates. Reading starts only once `cushion` samples are in, and again
/// after it runs dry, so the small differences in when each side runs do
/// not become gaps that click.
pub struct Ring {
    buf: Box<[AtomicU32]>,
    mask: usize,
    /// Samples ever written and ever read; their difference is the fill.
    write: AtomicUsize,
    read: AtomicUsize,
    primed: AtomicBool,
    /// What builds up before playing; grows after running dry when the
    /// ring adapts, and creeps back once the sound is steady.
    cushion: AtomicUsize,
    base: usize,
    adaptive: bool,
    /// Pops since the last time the ring ran dry.
    steady: AtomicU32,
    max: usize,
    /// Times the reader found it dry.
    pub underruns: AtomicU32,
    /// Times sound was thrown away: no room for the writer, or the reader
    /// skipping ahead because the delay grew.
    pub overruns: AtomicU32,
    /// The largest an adaptive cushion grows.
    cap: usize,
}

impl Ring {
    /// A ring of `channels` interleaved, holding `cushion` seconds before
    /// it plays.
    pub fn new(channels: usize, cushion: f32) -> Self {
        let per_second = RATE as f32 * channels as f32;
        let max = (per_second * RING_SECONDS) as usize;
        let size = (max * 2).next_power_of_two();
        let cushion = (per_second * cushion) as usize;
        Self {
            buf: (0..size).map(|_| AtomicU32::new(0)).collect(),
            mask: size - 1,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            primed: AtomicBool::new(false),
            cushion: AtomicUsize::new(cushion),
            base: cushion,
            adaptive: false,
            steady: AtomicU32::new(0),
            max,
            underruns: AtomicU32::new(0),
            overruns: AtomicU32::new(0),
            cap: cushion,
        }
    }

    /// A ring whose cushion grows by `step` seconds each time it runs dry,
    /// up to `most`, and shrinks back after a minute without, as WebRTC's
    /// jitter buffer: late bursts of sound then stop breaking it up.
    pub fn adaptive(channels: usize, cushion: f32, most: f32) -> Self {
        let per_second = RATE as f32 * channels as f32;
        let mut ring = Self::new(channels, cushion);
        ring.adaptive = true;
        ring.cap = (per_second * most) as usize;
        // Room for the deepest cushion and a little over before skipping,
        // within what the buffer holds.
        ring.max = ring.max.max(ring.cap + ring.base).min(ring.buf.len() * 3 / 4);
        ring
    }

    /// The cushion now, in samples.
    pub fn cushion(&self) -> usize {
        self.cushion.load(Ordering::Relaxed)
    }

    /// The writer's side. What does not fit is dropped and counted.
    pub fn push(&self, data: &[f32]) {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        let free = self.buf.len() - write.wrapping_sub(read);
        let n = data.len().min(free);
        for (i, sample) in data[..n].iter().enumerate() {
            self.buf[(write + i) & self.mask].store(sample.to_bits(), Ordering::Relaxed);
        }
        self.write.store(write + n, Ordering::Release);
        if n < data.len() {
            self.overruns.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn len(&self) -> usize {
        // The reader's count first: the writer's only grows, so the
        // difference never goes below zero.
        let read = self.read.load(Ordering::Acquire);
        let write = self.write.load(Ordering::Acquire);
        write.wrapping_sub(read).min(self.buf.len())
    }

    /// The reader's side: fills `out`, with silence while the cushion
    /// builds.
    pub fn pop(&self, out: &mut [f32]) {
        let mut read = self.read.load(Ordering::Relaxed);
        let mut have = self.write.load(Ordering::Acquire).wrapping_sub(read);
        let cushion = self.cushion();
        if have > self.max {
            // The delay grew: skip to the newest sound.
            let keep = cushion.max(out.len());
            read += have - keep;
            have = keep;
            self.overruns.fetch_add(1, Ordering::Relaxed);
        }
        if !self.primed.load(Ordering::Relaxed) {
            if have < cushion.max(out.len()) {
                self.read.store(read, Ordering::Release);
                out.fill(0.0);
                return;
            }
            self.primed.store(true, Ordering::Relaxed);
        }
        let n = have.min(out.len());
        for (i, slot) in out[..n].iter_mut().enumerate() {
            *slot = f32::from_bits(self.buf[(read + i) & self.mask].load(Ordering::Relaxed));
        }
        self.read.store(read + n, Ordering::Release);
        out[n..].fill(0.0);
        if n < out.len() {
            // Ran dry: build the cushion again, a deeper one when adapting.
            self.primed.store(false, Ordering::Relaxed);
            self.underruns.fetch_add(1, Ordering::Relaxed);
            if self.adaptive {
                let step = self.base / 3;
                self.cushion
                    .store((cushion + step).min(self.cap), Ordering::Relaxed);
                self.steady.store(0, Ordering::Relaxed);
            }
        } else if self.adaptive && cushion > self.base {
            // A minute or two of steady sound (pops come every 10 to 20
            // ms): give back a little of the delay.
            let steady = self.steady.fetch_add(1, Ordering::Relaxed) + 1;
            if steady >= 6_000 {
                self.steady.store(0, Ordering::Relaxed);
                self.cushion
                    .store(cushion.saturating_sub(self.base / 6).max(self.base), Ordering::Relaxed);
            }
        }
    }

    /// The reader's side: everything there is, with no cushion.
    fn drain(&self, out: &mut Vec<f32>) {
        let read = self.read.load(Ordering::Relaxed);
        let have = self.write.load(Ordering::Acquire).wrapping_sub(read);
        out.extend((0..have).map(|i| {
            f32::from_bits(self.buf[(read + i) & self.mask].load(Ordering::Relaxed))
        }));
        self.read.store(read + have, Ordering::Release);
    }

    /// How far to bend the reader's clock toward the writer's: positive
    /// when sound piles up (read faster), negative when it runs short, at
    /// most `DRIFT`. `ahead` is what the reader is about to take.
    fn drift(&self, ahead: usize) -> f32 {
        let target = (self.cushion() + ahead) as f32;
        ((self.len() as f32 - target) / target.max(1.0)).clamp(-1.0, 1.0) * DRIFT
    }
}

/// Linear interpolation of `input` onto `out`, ends on ends: how a block
/// a sample or two longer or shorter is bent to the size songbird wants.
fn stretch(input: &[f32], out: &mut [f32]) {
    if input.len() == out.len() || input.len() < 2 || out.len() < 2 {
        let n = input.len().min(out.len());
        out[..n].copy_from_slice(&input[..n]);
        out[n..].fill(0.0);
        return;
    }
    let scale = (input.len() - 1) as f32 / (out.len() - 1) as f32;
    for (i, slot) in out.iter_mut().enumerate() {
        let x = i as f32 * scale;
        let k = x as usize;
        let a = input[k];
        let b = input.get(k + 1).copied().unwrap_or(a);
        *slot = a + (b - a) * (x - k as f32);
    }
}

/// A WAV recording of one point of the call, for `DISCOFAST_AUDIO_DEBUG`.
/// The audio thread only pushes into a ring; a thread of its own writes
/// the file every 50 ms, so the sound never waits on the disk.
pub struct Tap {
    ring: Arc<Ring>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Tap {
    /// None unless `DISCOFAST_AUDIO_DEBUG` names a folder.
    pub fn open(name: &str, rate: u32, channels: u16) -> Option<Self> {
        let folder = std::env::var_os("DISCOFAST_AUDIO_DEBUG")?;
        let folder = std::path::PathBuf::from(folder);
        if let Err(error) = std::fs::create_dir_all(&folder) {
            log::warn!("audio debug: {}: {error}", folder.display());
            return None;
        }
        let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S");
        let path = folder.join(format!("{stamp}-{name}.wav"));
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = match hound::WavWriter::create(&path, spec) {
            Ok(writer) => writer,
            Err(error) => {
                log::warn!("audio debug: {}: {error}", path.display());
                return None;
            }
        };
        log::info!("audio debug: recording {} ({rate} Hz, {channels} ch)", path.display());
        // Four seconds of room, whatever the rate.
        let ring = Arc::new(Ring::new(
            (usize::from(channels) * rate as usize * 10 / RATE as usize).max(1),
            0.0,
        ));
        let stop = Arc::new(AtomicBool::new(false));
        let (from, done) = (Arc::clone(&ring), Arc::clone(&stop));
        let thread = std::thread::Builder::new()
            .name(format!("tap-{name}"))
            .spawn(move || {
                let mut samples = Vec::new();
                loop {
                    let last = done.load(Ordering::Acquire);
                    samples.clear();
                    from.drain(&mut samples);
                    for sample in &samples {
                        let _ = writer.write_sample(*sample);
                    }
                    // The header is good after every flush, so a crash
                    // still leaves a file that plays.
                    let _ = writer.flush();
                    if last {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                let _ = writer.finalize();
            })
            .ok()?;
        Some(Self {
            ring,
            stop,
            thread: Some(thread),
        })
    }

    pub fn push(&self, data: &[f32]) {
        self.ring.push(data);
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The voice filter's settings, which can change during a call.
pub struct Filter {
    /// 0 off, 1 standard (WebRTC's), 2 and up DeepFilterNet's neural network.
    pub noise: AtomicU8,
    /// How far DeepFilterNet may lower the noise, in dB: 100 all the way,
    /// lower leaves some of the room in.
    pub limit: AtomicU8,
    pub echo: AtomicBool,
    pub gain: AtomicBool,
    /// Sends nothing while the user is not talking, as Discord's
    /// automatic input sensitivity; otherwise only above `threshold`.
    pub gate: AtomicBool,
    /// The manual sensitivity in dB, as f32 bits.
    pub threshold: AtomicU32,
    /// Bumped on a change, for the filter to be built again.
    pub version: AtomicU32,
}

impl Filter {
    pub fn set(&self, settings: &crate::settings::Settings) {
        let filter = settings.voice_filter();
        self.noise.store(filter.noise, Ordering::Relaxed);
        self.limit.store(settings.noise_limit_db, Ordering::Relaxed);
        self.echo.store(filter.echo, Ordering::Relaxed);
        self.gain.store(filter.gain, Ordering::Relaxed);
        self.gate.store(filter.auto, Ordering::Relaxed);
        self.threshold
            .store(filter.threshold.to_bits(), Ordering::Relaxed);
        self.version.fetch_add(1, Ordering::Relaxed);
    }
}

/// What the interface and the voice driver share.
pub struct Shared {
    /// The microphone, 48 kHz mono, for the driver to filter and send.
    pub mic: Ring,
    /// What the speakers play, 48 kHz mono, for the echo canceller.
    pub played: Ring,
    /// Everyone else mixed, 48 kHz stereo, for the speakers.
    pub speakers: Ring,
    /// Deafened: what others say is not played.
    pub deaf: AtomicBool,
    pub filter: Filter,
    /// The user is talking, by the filter's voice detection.
    pub talking: AtomicBool,
    /// Who else is talking right now.
    pub speaking: Mutex<HashSet<Id>>,
    /// The gain for each person who is not at 100%: 0 for one silenced.
    pub volumes: Mutex<HashMap<Id, f32>>,
    /// The microphone's and the speakers' volume, as f32 bits.
    pub mic_gain: AtomicU32,
    pub speaker_gain: AtomicU32,
    /// The microphone's level in dB, as f32 bits, for the meters.
    pub level: AtomicU32,
    /// Frames in the last microphone and speaker callback, for the log.
    pub mic_frames: AtomicU32,
    pub speaker_frames: AtomicU32,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            mic: Ring::new(1, 0.04),
            played: Ring::new(1, 0.0),
            // 60 ms, up to 180 ms when the sound comes in bursts.
            speakers: Ring::adaptive(CHANNELS, 0.06, 0.18),
            deaf: AtomicBool::new(false),
            filter: Filter {
                noise: AtomicU8::new(2),
                limit: AtomicU8::new(100),
                echo: AtomicBool::new(true),
                gain: AtomicBool::new(true),
                gate: AtomicBool::new(true),
                threshold: AtomicU32::new((-60f32).to_bits()),
                version: AtomicU32::new(0),
            },
            talking: AtomicBool::new(false),
            speaking: Mutex::new(HashSet::new()),
            volumes: Mutex::new(HashMap::new()),
            mic_gain: AtomicU32::new(1f32.to_bits()),
            speaker_gain: AtomicU32::new(1f32.to_bits()),
            level: AtomicU32::new((-100f32).to_bits()),
            mic_frames: AtomicU32::new(0),
            speaker_frames: AtomicU32::new(0),
        }
    }
}

impl Shared {
    /// The microphone's and the speakers' volume from the settings.
    pub fn set_gains(&self, settings: &crate::settings::Settings) {
        let gain = |percent: u16| (f32::from(percent) / 100.0).to_bits();
        self.mic_gain
            .store(gain(settings.input_volume), Ordering::Relaxed);
        self.speaker_gain
            .store(gain(settings.output_volume), Ordering::Relaxed);
    }

    /// The microphone's level now, in dB.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Takes each person's volume and local mute from the settings.
    pub fn set_volumes(&self, settings: &crate::settings::Settings) {
        let mut volumes: HashMap<Id, f32> = settings
            .user_volumes
            .iter()
            .map(|(user, percent)| (*user, f32::from(*percent) / 100.0))
            .collect();
        for user in &settings.local_mutes {
            volumes.insert(*user, 0.0);
        }
        *self.volumes.lock().unwrap_or_else(|e| e.into_inner()) = volumes;
    }

    /// One line for the log: how full each ring is and how often it ran
    /// dry or over since the last report.
    pub fn report(&self) -> String {
        let ring = |name: &str, ring: &Ring, channels: usize| {
            format!(
                "{name} {} ms of {} ms cushion (underruns {}, overruns {})",
                ring.len() / channels * 1000 / RATE as usize,
                ring.cushion() / channels * 1000 / RATE as usize,
                ring.underruns.swap(0, Ordering::Relaxed),
                ring.overruns.swap(0, Ordering::Relaxed),
            )
        };
        format!(
            "{}; {}; {}; callbacks: mic {} frames, speakers {} frames",
            ring("mic", &self.mic, 1),
            ring("speakers", &self.speakers, CHANNELS),
            ring("echo reference", &self.played, 1),
            self.mic_frames.load(Ordering::Relaxed),
            self.speaker_frames.load(Ordering::Relaxed),
        )
    }
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("call::Shared")
    }
}

/// Whether the filter asks for DeepFilterNet rather than WebRTC's
/// suppression.
fn neural(filter: &Filter) -> bool {
    filter.noise.load(Ordering::Relaxed) >= 2
}

/// The filter's stages in WebRTC's order: echo cancelling, then noise
/// suppression, then level control. With DeepFilterNet the suppression
/// runs between two WebRTC passes, so the level control never raises the
/// noise before it is taken out.
struct Stages {
    /// High-pass and echo cancelling, and with the standard suppression
    /// also its noise suppression and level control.
    first: Option<sonora::AudioProcessing>,
    denoiser: Option<Denoiser>,
    /// The level control after DeepFilterNet.
    gain: Option<sonora::AudioProcessing>,
}

impl Stages {
    /// `echo`: whether the echo canceller may run; not in the microphone
    /// test, where it would take out the user's own voice played back.
    fn new(filter: &Filter, echo: bool) -> Self {
        let echo = echo && filter.echo.load(Ordering::Relaxed);
        let gain = filter.gain.load(Ordering::Relaxed);
        if neural(filter) {
            let limit = f32::from(filter.limit.load(Ordering::Relaxed));
            Self {
                first: processor(echo, false, false),
                denoiser: Some(Denoiser::start(limit)),
                gain: processor(false, false, gain),
            }
        } else {
            let noise = filter.noise.load(Ordering::Relaxed) == 1;
            Self {
                first: processor(echo, noise, gain),
                denoiser: None,
                gain: None,
            }
        }
    }
}

/// WebRTC's processing: echo cancelling against what the speakers play,
/// the standard noise suppression and level control, as asked.
fn processor(echo: bool, noise: bool, gain: bool) -> Option<sonora::AudioProcessing> {
    use sonora::config::{
        AdaptiveDigital, EchoCanceller, GainController2, HighPassFilter, NoiseSuppression,
        NoiseSuppressionLevel,
    };
    if !noise && !echo && !gain {
        return None;
    }
    let stream = sonora::StreamConfig::new(RATE, 1);
    Some(
        sonora::AudioProcessing::builder()
            .config(sonora::Config {
                high_pass_filter: (echo || noise).then(HighPassFilter::default),
                echo_canceller: echo.then(EchoCanceller::default),
                noise_suppression: noise.then(|| NoiseSuppression {
                    level: NoiseSuppressionLevel::High,
                    ..NoiseSuppression::default()
                }),
                // The digital gain only: the system's microphone volume is
                // the user's to set.
                gain_controller2: gain.then(|| GainController2 {
                    input_volume_controller: false,
                    adaptive_digital: Some(AdaptiveDigital {
                        max_gain_db: 20.0,
                        initial_gain_db: 0.0,
                        ..AdaptiveDigital::default()
                    }),
                    ..GainController2::default()
                }),
                ..sonora::Config::default()
            })
            .capture_config(stream)
            .render_config(stream)
            .build(),
    )
}

/// Tells talking from silence on the filtered microphone: louder than the
/// room's floor by a margin, held a moment after the voice stops so the
/// ring does not flicker between words.
struct VoiceDetector {
    floor: f32,
    above: u32,
    /// Chunks left before a voice that stopped counts as stopped.
    hold: u32,
}

impl VoiceDetector {
    /// 400 ms of 10 ms chunks.
    const HOLD: u32 = 40;

    fn new() -> Self {
        Self {
            floor: -60.0,
            above: 0,
            hold: 0,
        }
    }

    /// One 10 ms chunk; whether the user is talking. With a threshold,
    /// as Discord's manual sensitivity, louder than it is talking; with
    /// the denoiser's voice probability, by that rather than by loudness.
    fn feed(&mut self, chunk: &[f32], threshold: Option<f32>, voice: Option<f32>) -> bool {
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        let db = 20.0 * rms.max(1e-6).log10();
        if let Some(threshold) = threshold {
            if threshold <= -99.0 {
                return true;
            }
            if db > threshold {
                self.hold = Self::HOLD;
            } else {
                self.hold = self.hold.saturating_sub(1);
            }
            return self.hold > 0;
        }
        // The floor follows quiet quickly, and a steady sound (a fan the
        // level control raised) within about half a second; speech rises
        // and falls too often to be taken for it.
        self.floor = if db < self.floor {
            self.floor + (db - self.floor) * 0.2
        } else {
            self.floor + (db - self.floor) * 0.01
        };
        let loud = match voice {
            Some(probability) => probability > 0.6 && db > -60.0,
            None => db > (self.floor + 10.0).max(-45.0),
        };
        self.above = if loud { self.above + 1 } else { 0 };
        // Two chunks in a row start it: a click does not.
        if self.above >= 2 {
            self.hold = Self::HOLD;
        } else {
            self.hold = self.hold.saturating_sub(1);
        }
        self.hold > 0
    }
}

/// The microphone as songbird reads it: filtered, gated while the user is
/// quiet, as raw little-endian f32 mono; never ending. songbird's mixer
/// thread reads it every 20 ms, so it never sleeps waiting for the
/// microphone: the ring's cushion and the clock bending keep sound there.
pub struct MicSource {
    shared: Arc<Shared>,
    pending: Vec<u8>,
    filter: Mutex<Filtering>,
    /// What leaves the filter, for `DISCOFAST_AUDIO_DEBUG`.
    tap: Option<Tap>,
    /// The microphone test's source: no echo cancelling.
    test: bool,
}

struct Filtering {
    stages: Stages,
    version: u32,
    voice: VoiceDetector,
    /// The gate's gain now, moving smoothly so it never clicks.
    open: f32,
    /// What came out of the microphone ring, a sample or two off a block.
    taken: Vec<f32>,
}

impl MicSource {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self::with(shared, false)
    }

    fn with(shared: Arc<Shared>, test: bool) -> Self {
        Self {
            filter: Mutex::new(Filtering {
                stages: Stages::new(&shared.filter, !test),
                version: shared.filter.version.load(Ordering::Relaxed),
                voice: VoiceDetector::new(),
                open: 0.0,
                taken: Vec::with_capacity(BLOCK + 8),
            }),
            shared,
            pending: Vec::new(),
            tap: Tap::open("2-processado", RATE, 1),
            test,
        }
    }

    /// Twenty milliseconds of the microphone, filtered, mono.
    fn next_block(&mut self) -> Vec<f32> {
        let shared = &self.shared;
        let mut filtering = self.filter.lock().unwrap_or_else(|e| e.into_inner());
        let f = &mut *filtering;
        // The microphone's clock and songbird's never agree exactly: take a
        // sample or two more when sound piles up, less when it runs short.
        let bend = shared.mic.drift(BLOCK);
        let take = (BLOCK as f32 * (1.0 + bend)).round() as usize;
        f.taken.resize(take, 0.0);
        shared.mic.pop(&mut f.taken);
        let mut mono = vec![0f32; BLOCK];
        stretch(&f.taken, &mut mono);
        let version = shared.filter.version.load(Ordering::Relaxed);
        if version != f.version {
            f.version = version;
            f.stages = Stages::new(&shared.filter, !self.test);
        }
        let auto = shared.filter.gate.load(Ordering::Relaxed);
        let threshold =
            (!auto).then(|| f32::from_bits(shared.filter.threshold.load(Ordering::Relaxed)));
        let mut talking = false;
        let mut played = [0f32; CHUNK];
        let mut sink = [0f32; CHUNK];
        let mut out = [0f32; CHUNK];
        for chunk in mono.chunks_mut(CHUNK) {
            if let Some(p) = &mut f.stages.first {
                played.fill(0.0);
                if shared.played.len() >= CHUNK {
                    shared.played.pop(&mut played);
                }
                let _ = p.process_render_f32(&[&played], &mut [&mut sink]);
                if p.process_capture_f32(&[&*chunk], &mut [&mut out]).is_ok() {
                    chunk.copy_from_slice(&out);
                }
            }
            // After the echo canceller, so the network never hears the call.
            let voice = f
                .stages
                .denoiser
                .as_mut()
                .and_then(|denoiser| denoiser.clean(chunk));
            if let Some(p) = &mut f.stages.gain
                && p.process_capture_f32(&[&*chunk], &mut [&mut out]).is_ok()
            {
                chunk.copy_from_slice(&out);
            }
            talking = f.voice.feed(chunk, threshold, voice);
            let target = if talking { 1.0 } else { 0.0 };
            for sample in chunk.iter_mut() {
                f.open += (target - f.open) * 0.01;
                *sample = (*sample * f.open).clamp(-1.0, 1.0);
            }
        }
        drop(filtering);
        shared.talking.store(talking, Ordering::Relaxed);
        if let Some(tap) = &self.tap {
            tap.push(&mono);
        }
        mono
    }
}

/// DeepFilterNet on a thread of its own: its model cannot move between
/// threads, and the microphone source must. Each 10 ms chunk goes there
/// and comes back clean, a fraction of a millisecond later.
struct Denoiser {
    chunks: std::sync::mpsc::SyncSender<Vec<f32>>,
    cleaned: std::sync::mpsc::Receiver<(Vec<f32>, f32)>,
    /// The model has loaded; until then the sound passes as it is.
    ready: Arc<std::sync::atomic::AtomicBool>,
    /// The thread stopped answering: the sound passes as it is.
    broken: bool,
}

impl Denoiser {
    /// `limit`: the most, in dB, the noise is lowered.
    fn start(limit: f32) -> Self {
        let (chunks, inbox) = std::sync::mpsc::sync_channel::<Vec<f32>>(4);
        let (outbox, cleaned) = std::sync::mpsc::sync_channel(4);
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&ready);
        let spawned = std::thread::Builder::new()
            .name("deepfilternet".into())
            .spawn(move || {
                use df::tract::{DfParams, DfTract, RuntimeParams};
                let mut params = RuntimeParams::default_with_ch(1);
                params.atten_lim_db = limit.clamp(0.0, 100.0);
                let mut model = match DfTract::new(DfParams::default(), &params) {
                    Ok(model) => model,
                    Err(error) => {
                        log::warn!("DeepFilterNet: {error}");
                        return;
                    }
                };
                log::info!("DeepFilterNet: ready, noise lowered by up to {} dB", params.atten_lim_db);
                let mut noisy = ndarray::Array2::<f32>::zeros((1, CHUNK));
                let mut clean = ndarray::Array2::<f32>::zeros((1, CHUNK));
                flag.store(true, Ordering::Release);
                // Until the microphone source drops its end.
                while let Ok(chunk) = inbox.recv() {
                    noisy.row_mut(0).assign(&ndarray::ArrayView1::from(&chunk[..]));
                    let snr = model
                        .process(noisy.view(), clean.view_mut())
                        .unwrap_or(-15.0);
                    if outbox.send((clean.row(0).to_vec(), snr)).is_err() {
                        break;
                    }
                }
            });
        Self {
            chunks,
            cleaned,
            ready,
            broken: spawned.is_err(),
        }
    }

    /// Cleans one 10 ms chunk in place; the chance it is a voice, from the
    /// model's signal-to-noise estimate (-15 dB for noise alone, speech
    /// well above zero). None while the model loads.
    fn clean(&mut self, chunk: &mut [f32]) -> Option<f32> {
        if self.broken || !self.ready.load(Ordering::Acquire) || chunk.len() != CHUNK {
            return None;
        }
        if self.chunks.send(chunk.to_vec()).is_err() {
            self.broken = true;
            return None;
        }
        match self.cleaned.recv_timeout(Duration::from_millis(30)) {
            Ok((clean, snr)) => {
                for (sample, clean) in chunk.iter_mut().zip(clean) {
                    *sample = clean.clamp(-1.0, 1.0);
                }
                Some((snr / 10.0).clamp(0.0, 1.0))
            }
            Err(_) => {
                // Late answers would land on the wrong chunk from now on.
                log::warn!("DeepFilterNet: too slow, the noise passes from now on");
                self.broken = true;
                None
            }
        }
    }
}

impl Read for MicSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            let block = self.next_block();
            self.pending = block.iter().flat_map(|s| s.to_le_bytes()).collect();
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

impl Seek for MicSource {
    fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

impl symphonia_core::io::MediaSource for MicSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// Turns a device's rate and channels into 48 kHz mono, a callback at a
/// time. The microphone is opened at 48 kHz whenever the device takes it,
/// so this only interpolates on the rare one that does not.
struct Resampler {
    from: u32,
    channels: usize,
    position: f64,
    last: f32,
}

impl Resampler {
    fn new(from: u32, channels: usize) -> Self {
        Self {
            from,
            channels: channels.max(1),
            position: 0.0,
            last: 0.0,
        }
    }

    /// `scratch` holds the downmix; both vectors keep their room between
    /// callbacks, so nothing is allocated once they have grown.
    fn feed(&mut self, input: &[f32], scratch: &mut Vec<f32>, out: &mut Vec<f32>) {
        let scale = 1.0 / self.channels as f32;
        scratch.clear();
        scratch.extend(
            input
                .chunks_exact(self.channels)
                .map(|f| f.iter().sum::<f32>() * scale),
        );
        if self.from == RATE {
            out.extend_from_slice(scratch);
            return;
        }
        let step = f64::from(self.from) / f64::from(RATE);
        while (self.position as usize) < scratch.len() {
            let i = self.position as usize;
            let t = (self.position - i as f64) as f32;
            let a = if i == 0 { self.last } else { scratch[i - 1] };
            let b = scratch[i];
            out.push(a + (b - a) * t);
            self.position += step;
        }
        self.position -= scratch.len() as f64;
        if let Some(last) = scratch.last() {
            self.last = *last;
        }
    }
}

/// "Teste do microfone": the microphone played back on the speakers as
/// the others would hear it, through the same filter, so changing the
/// noise suppression during the test is heard at once; and its level for
/// the meter, until it is dropped.
pub struct MicTest {
    pub shared: Arc<Shared>,
    _audio: Audio,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MicTest {
    pub fn start(settings: &crate::settings::Settings) -> Result<Self, String> {
        let shared = Arc::new(Shared::default());
        shared.filter.set(settings);
        shared.set_gains(settings);
        let audio = Audio::start(
            Arc::clone(&shared),
            settings.input_device.clone(),
            settings.output_device.clone(),
        )?;
        let stop = Arc::new(AtomicBool::new(false));
        let mut source = MicSource::with(Arc::clone(&shared), true);
        let (to, done) = (Arc::clone(&shared), Arc::clone(&stop));
        // A block every 20 ms, as songbird's mixer reads it in a call.
        let thread = std::thread::Builder::new()
            .name("mic-test".into())
            .spawn(move || {
                let period = Duration::from_millis(20);
                let mut next = Instant::now();
                let mut stereo = Vec::with_capacity(BLOCK * CHANNELS);
                while !done.load(Ordering::Relaxed) {
                    let block = source.next_block();
                    stereo.clear();
                    stereo.extend(block.iter().flat_map(|s| [*s, *s]));
                    to.speakers.push(&stereo);
                    next += period;
                    let now = Instant::now();
                    if next > now {
                        std::thread::sleep(next - now);
                    } else if now - next > period * 5 {
                        // Fell far behind (the machine was busy): start over.
                        next = now;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            shared,
            _audio: audio,
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for MicTest {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The call's two streams; dropping it closes them.
pub struct Audio {
    _mic: Option<cpal::Stream>,
    _speakers: cpal::Stream,
}

impl Audio {
    /// Opens the default microphone and speakers. A missing microphone
    /// still lets the user listen.
    pub fn start(
        shared: Arc<Shared>,
        input: Option<String>,
        output: Option<String>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        let mic = match host.default_input_device() {
            Some(device) => match open_mic(&device, &shared) {
                Ok(stream) => Some(stream),
                Err(e) => {
                    log::warn!("call: no microphone: {e}");
                    None
                }
            },
            None => None,
        };
        let device = host
            .default_output_device()
            .ok_or("Nenhuma saída de áudio encontrada.")?;
        let speakers = open_speakers(&device, &shared)?;
        crate::devices::route(input, output);
        Ok(Self {
            _mic: mic,
            _speakers: speakers,
        })
    }
}

/// 48 kHz in the device's own format when it takes it (PipeWire and
/// PulseAudio convert well), else the device's default.
fn mic_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, String> {
    let usable = |f: cpal::SampleFormat| {
        matches!(
            f,
            cpal::SampleFormat::F32
                | cpal::SampleFormat::I16
                | cpal::SampleFormat::U16
                | cpal::SampleFormat::I32
        )
    };
    let wanted = device.supported_input_configs().ok().and_then(|configs| {
        configs
            .filter(|c| {
                usable(c.sample_format())
                    && c.min_sample_rate().0 <= RATE
                    && c.max_sample_rate().0 >= RATE
            })
            // Mono first, then the fewest channels; f32 first.
            .min_by_key(|c| (c.channels(), c.sample_format() != cpal::SampleFormat::F32))
            .map(|c| c.with_sample_rate(cpal::SampleRate(RATE)))
    });
    match wanted {
        Some(config) => Ok(config),
        None => device.default_input_config().map_err(|e| e.to_string()),
    }
}

fn open_mic(device: &cpal::Device, shared: &Arc<Shared>) -> Result<cpal::Stream, String> {
    let config = mic_config(device)?;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    log::info!(
        "call microphone: {}, {} Hz, {} ch, {format}{}",
        device.name().unwrap_or_default(),
        config.sample_rate.0,
        config.channels,
        if config.sample_rate.0 == RATE { "" } else { ", resampled to 48 kHz" },
    );
    let stream = match format {
        cpal::SampleFormat::F32 => mic_stream::<f32>(device, &config, shared),
        cpal::SampleFormat::I16 => mic_stream::<i16>(device, &config, shared),
        cpal::SampleFormat::U16 => mic_stream::<u16>(device, &config, shared),
        cpal::SampleFormat::I32 => mic_stream::<i32>(device, &config, shared),
        other => return Err(format!("formato {other}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

fn mic_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: &Arc<Shared>,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let shared = Arc::clone(shared);
    let mut resampler = Resampler::new(config.sample_rate.0, usize::from(config.channels));
    // Grown on the first callbacks and reused: nothing is allocated, no
    // lock is taken and nothing is written to disk on the audio thread.
    let mut converted = Vec::with_capacity(16_384);
    let mut scratch = Vec::with_capacity(16_384);
    let mut mono = Vec::with_capacity(16_384);
    let raw = Tap::open("1-captura", config.sample_rate.0, config.channels);
    let channels = usize::from(config.channels).max(1);
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            converted.clear();
            converted.extend(
                data.iter()
                    .map(|s| <f32 as FromSample<T>>::from_sample_(*s)),
            );
            if let Some(raw) = &raw {
                raw.push(&converted);
            }
            shared
                .mic_frames
                .store((data.len() / channels) as u32, Ordering::Relaxed);
            mono.clear();
            // The filter works on one channel.
            resampler.feed(&converted, &mut scratch, &mut mono);
            let gain = f32::from_bits(shared.mic_gain.load(Ordering::Relaxed));
            if gain != 1.0 {
                for sample in &mut mono {
                    *sample = (*sample * gain).clamp(-1.0, 1.0);
                }
            }
            if !mono.is_empty() {
                let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32).sqrt();
                let db = (20.0 * rms.max(1e-5).log10()).max(-100.0);
                shared.level.store(db.to_bits(), Ordering::Relaxed);
            }
            shared.mic.push(&mono);
        },
        |error| log::warn!("call microphone: {error}"),
        None,
    )
}

fn open_speakers(device: &cpal::Device, shared: &Arc<Shared>) -> Result<cpal::Stream, String> {
    // 48 kHz stereo when the device takes it, as PipeWire and most do.
    let wanted = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .find(|c| {
            c.channels() == 2
                && c.sample_format() == cpal::SampleFormat::F32
                && c.min_sample_rate().0 <= RATE
                && c.max_sample_rate().0 >= RATE
        })
        .map(|c| c.with_sample_rate(cpal::SampleRate(RATE)));
    let config = match wanted {
        Some(c) => c,
        None => device.default_output_config().map_err(|e| e.to_string())?,
    };
    if config.sample_format() != cpal::SampleFormat::F32 {
        return Err(format!(
            "Saída de áudio em {} não suportada.",
            config.sample_format()
        ));
    }
    let rate = config.sample_rate().0;
    let channels = usize::from(config.channels());
    log::info!(
        "call speakers: {}, {rate} Hz, {channels} ch, f32{}",
        device.name().unwrap_or_default(),
        if rate == RATE { "" } else { ", resampled from 48 kHz" },
    );
    let config: cpal::StreamConfig = config.into();
    let mut playback = Playback::new(Arc::clone(shared), rate, channels);
    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| playback.fill(data),
            |error| log::warn!("call speakers: {error}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

/// The speakers' side of the call: takes 48 kHz stereo from the ring at
/// the sound card's own rate and clock, and hands the echo canceller what
/// was played. Apart from the callback so it can be driven in tests.
pub struct Playback {
    shared: Arc<Shared>,
    channels: usize,
    /// 48 kHz frames waiting to be played, and how far into them.
    pending: VecDeque<[f32; 2]>,
    position: f64,
    pulled: Vec<f32>,
    /// What was played, mono, for the echo canceller.
    reference: Vec<f32>,
    base: f64,
}

impl Playback {
    pub fn new(shared: Arc<Shared>, rate: u32, channels: usize) -> Self {
        Self {
            shared,
            channels: channels.max(1),
            pending: VecDeque::with_capacity(16_384),
            position: 0.0,
            pulled: Vec::with_capacity(32_768),
            reference: Vec::with_capacity(16_384),
            base: f64::from(RATE) / f64::from(rate),
        }
    }

    /// One callback's worth of interleaved frames.
    pub fn fill(&mut self, data: &mut [f32]) {
        let shared = &self.shared;
        let channels = self.channels;
        let frames = data.len() / channels;
        shared.speaker_frames.store(frames as u32, Ordering::Relaxed);
        // The network's clock and the sound card's never agree exactly:
        // play a hair faster when sound piles up, slower when it runs
        // short, so the ring neither skips nor runs dry.
        let bend = shared.speakers.drift(frames * CHANNELS);
        let step = self.base * (1.0 + f64::from(bend));
        // Exactly the frames this callback covers, no more, so the ring
        // is not drained faster than it fills.
        let needed = (self.position + frames as f64 * step).ceil() as usize + 1;
        if self.pending.len() < needed {
            self.pulled.resize((needed - self.pending.len()) * CHANNELS, 0.0);
            shared.speakers.pop(&mut self.pulled);
            self.pending
                .extend(self.pulled.as_chunks::<2>().0.iter().copied());
        }
        let deaf = shared.deaf.load(Ordering::Relaxed);
        let gain = if deaf {
            0.0
        } else {
            f32::from_bits(shared.speaker_gain.load(Ordering::Relaxed))
        };
        for (i, frame) in data.chunks_mut(channels).enumerate() {
            let at = self.position + i as f64 * step;
            let k = at as usize;
            let t = (at - k as f64) as f32;
            let a = self.pending.get(k).copied().unwrap_or([0.0; 2]);
            let b = self.pending.get(k + 1).copied().unwrap_or(a);
            let l = (a[0] + (b[0] - a[0]) * t) * gain;
            let r = (a[1] + (b[1] - a[1]) * t) * gain;
            for (c, sample) in frame.iter_mut().enumerate() {
                *sample = if c % 2 == 0 { l } else { r };
            }
        }
        let advanced = self.position + frames as f64 * step;
        let used = (advanced as usize).min(self.pending.len());
        self.reference.clear();
        self.reference.extend(
            self.pending
                .range(..used)
                .map(|[l, r]| (l + r) * 0.5 * gain),
        );
        shared.played.push(&self.reference);
        self.pending.drain(..used);
        self.position = advanced - used as f64;
    }
}

/// How far the call has got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Asked the gateway; waiting for the voice server.
    Joining,
    Connecting,
    Connected,
    Failed(String),
}

/// The user's call: where, how far, and its sound.
pub struct Session {
    pub guild: Option<Id>,
    pub channel: Id,
    pub state: State,
    /// From the gateway, for the voice server.
    pub session_id: Option<String>,
    pub server: Option<(String, String)>,
    pub shared: Arc<Shared>,
    /// Held so the microphone and speakers stay open.
    pub _audio: Option<Audio>,
    /// The connection info has gone to the driver.
    pub sent: bool,
    /// When the call started, for the time beside the channel.
    pub since: Instant,
    /// The screen share being watched, if any.
    pub watching: Option<Watch>,
    /// The room on the user's own video server, when one is set.
    pub room: Option<OwnRoom>,
}

/// The call's room on the user's own video server: everyone's camera and
/// screen there, and what the user sends.
pub struct OwnRoom {
    pub state: State,
    pub videos: Arc<crate::room::Videos>,
    pub camera: bool,
    pub screen: bool,
    /// Whose screen is on the stage.
    pub focus: Option<Id>,
    /// Who sends what, each with its newest picture on the graphics card
    /// once one came.
    pub textures: HashMap<(Id, crate::room::Kind), Option<egui::TextureHandle>>,
}

impl OwnRoom {
    /// The videos as they are now: new pictures uploaded, ended ones gone.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        let live = self.videos.take();
        self.textures
            .retain(|key, _| live.iter().any(|(k, _)| k == key));
        for (key, frame) in live {
            let slot = self.textures.entry(key).or_default();
            let Some(frame) = frame else {
                continue;
            };
            let image =
                egui::ColorImage::from_rgba_unmultiplied([frame.width, frame.height], &frame.rgba);
            match slot {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => {
                    *slot = Some(ctx.load_texture(
                        format!("video-{}-{:?}", key.0, key.1),
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
        }
        if self
            .focus
            .is_some_and(|u| !self.textures.contains_key(&(u, crate::room::Kind::Screen)))
        {
            self.focus = None;
        }
    }

    pub fn video(&self, user: Id, kind: crate::room::Kind) -> Option<&egui::TextureHandle> {
        self.textures.get(&(user, kind)).and_then(Option::as_ref)
    }

    pub fn sends(&self, user: Id, kind: crate::room::Kind) -> bool {
        self.textures.contains_key(&(user, kind))
    }
}

/// Someone's screen share, watched in the call. Discord gives it its own
/// voice server: STREAM_CREATE names it and STREAM_SERVER_UPDATE says
/// where it is.
pub struct Watch {
    pub user: Id,
    pub key: String,
    pub state: State,
    pub rtc_server_id: Option<u64>,
    pub server: Option<(String, String)>,
    /// The connection info has gone to the backend.
    pub sent: bool,
    /// The newest picture, written by the decoder.
    pub screen: Arc<crate::discord::stream::Screen>,
    /// The newest picture on the graphics card.
    pub texture: Option<egui::TextureHandle>,
}

/// Whether this build can show a screen share at all.
pub const CAN_WATCH: bool = cfg!(feature = "video");

/// How Discord names a screen share: by server, channel and person, or by
/// the direct message call and person.
pub fn stream_key(guild: Option<Id>, channel: Id, user: Id) -> String {
    match guild {
        Some(guild) => format!("guild:{guild}:{channel}:{user}"),
        None => format!("call:{channel}:{user}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_keys_name_the_server_or_the_call() {
        assert_eq!(stream_key(Some(1), 2, 3), "guild:1:2:3");
        assert_eq!(stream_key(None, 2, 3), "call:2:3");
    }

    #[test]
    fn any_rate_comes_out_at_48k_mono() {
        let mut r = Resampler::new(44_100, 2);
        let (mut scratch, mut out) = (Vec::new(), Vec::new());
        // One second of stereo fed in pieces, as callbacks come.
        for _ in 0..10 {
            r.feed(&vec![0.5; 4_410 * 2], &mut scratch, &mut out);
        }
        assert!((out.len() as i64 - 48_000).abs() < 10, "{}", out.len());
        assert!(out[10..].iter().all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn the_ring_waits_for_its_cushion_and_skips_old_sound() {
        let ring = Ring::new(1, 0.01);
        let mut out = vec![9.0; 100];
        ring.push(&[1.0; 200]);
        // 480 samples of cushion: not yet.
        ring.pop(&mut out);
        assert!(out.iter().all(|s| *s == 0.0));
        ring.push(&[1.0; 400]);
        ring.pop(&mut out);
        assert!(out.iter().all(|s| *s == 1.0));
        // Too much: the reader skips to the newest, back to its cushion.
        ring.push(&vec![2.0; 20_000]);
        ring.pop(&mut out);
        assert!(out.iter().all(|s| *s == 2.0));
        assert_eq!(ring.len(), 480 - 100);
        assert_eq!(ring.overruns.load(Ordering::Relaxed), 1);
        // Dry: counted, and the cushion builds again.
        let mut big = vec![0.0; 1_000];
        ring.pop(&mut big);
        assert_eq!(ring.underruns.load(Ordering::Relaxed), 1);
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn an_adaptive_ring_deepens_after_running_dry() {
        let ring = Ring::adaptive(1, 0.06, 0.18);
        let base = ring.cushion();
        let mut out = vec![0.0; 480];
        ring.push(&vec![1.0; base]);
        ring.pop(&mut out);
        let mut big = vec![0.0; base * 2];
        ring.pop(&mut big);
        assert_eq!(ring.underruns.load(Ordering::Relaxed), 1);
        assert!(ring.cushion() > base);
        for _ in 0..20 {
            ring.pop(&mut big);
        }
        assert!(ring.cushion() <= (RATE as f32 * 0.18) as usize);
    }

    #[test]
    fn the_ring_survives_a_writer_and_a_reader_on_two_threads() {
        let ring = Arc::new(Ring::new(1, 0.0));
        let writer = Arc::clone(&ring);
        let thread = std::thread::spawn(move || {
            for i in 0..2_000u32 {
                while writer.len() > 8_000 {
                    std::thread::yield_now();
                }
                writer.push(&[i as f32; 10]);
            }
        });
        let mut got = Vec::new();
        let mut out = [0f32; 10];
        while got.len() < 2_000 {
            if ring.len() >= 10 {
                ring.pop(&mut out);
                got.push(out[0]);
                assert!(out.iter().all(|s| *s == out[0]));
            }
        }
        thread.join().unwrap();
        assert!(got.windows(2).all(|w| w[1] == w[0] + 1.0));
    }

    #[test]
    fn a_stretched_block_keeps_its_ends_and_its_size() {
        let input: Vec<f32> = (0..962).map(|i| i as f32).collect();
        let mut out = [0f32; BLOCK];
        stretch(&input, &mut out);
        assert_eq!(out[0], 0.0);
        assert!((out[BLOCK - 1] - 961.0).abs() < 1e-3);
        assert!(out.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn the_filter_passes_a_voice_and_gates_the_silence_after() {
        let shared = Arc::new(Shared::default());
        let mut source = MicSource::new(Arc::clone(&shared));
        // Half a second of a voice-like tone over a little noise.
        let mut seed = 1u32;
        let mut noise = move || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (seed >> 16) as f32 / 65_536.0 - 0.5
        };
        let voiced: Vec<f32> = (0..RATE as usize / 2)
            .map(|i| (i as f32 * 0.04).sin() * 0.3 + noise() * 0.002)
            .collect();
        let mut loud = 0.0f32;
        for block in voiced.chunks(CHUNK * 2) {
            shared.mic.push(block);
            let out = source.next_block();
            assert_eq!(out.len(), BLOCK);
            assert!(out.iter().all(|s| s.is_finite()));
            loud = loud.max(out.iter().fold(0f32, |p, s| p.max(s.abs())));
        }
        assert!(loud > 0.05, "{loud}");
        assert!(shared.talking.load(Ordering::Relaxed));
        // A second of near silence: the gate closes.
        let quiet: Vec<f32> = (0..RATE as usize).map(|_| noise() * 0.001).collect();
        let mut last = Vec::new();
        for block in quiet.chunks(CHUNK * 2) {
            shared.mic.push(block);
            last = source.next_block();
        }
        assert!(!shared.talking.load(Ordering::Relaxed));
        let peak = last.iter().fold(0f32, |p, s| p.max(s.abs()));
        assert!(peak < 1e-3, "{peak}");
    }

    #[test]
    fn deepfilternet_quiets_a_hiss_and_does_not_call_it_a_voice() {
        let mut denoiser = Denoiser::start(100.0);
        let start = Instant::now();
        while !denoiser.ready.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Deterministic white noise around -30 dBFS.
        let mut seed = 1u32;
        let mut hiss = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 * 0.06 - 0.03
        };
        let (mut before, mut after, mut voice) = (0f32, 0f32, 0f32);
        for frame in 0..200 {
            let mut chunk: Vec<f32> = (0..CHUNK).map(|_| hiss()).collect();
            let energy = chunk.iter().map(|s| s * s).sum::<f32>();
            let probability = denoiser.clean(&mut chunk).expect("the model answers");
            // The network needs a moment to learn the room.
            if frame >= 100 {
                before += energy;
                after += chunk.iter().map(|s| s * s).sum::<f32>();
                voice = voice.max(probability);
            }
        }
        assert!(after < before / 3.0, "{after} of {before}");
        assert!(voice < 0.5, "{voice}");
    }

    /// The send path in real time, as in a call: a microphone thread
    /// pushing 10 ms at a time with the jitter a sound server has, and
    /// songbird's mixer taking 20 ms every 20 ms. Reports how long the
    /// filter takes, how often the ring ran dry and where the sound broke.
    /// `cargo test --release realtime -- --ignored --nocapture`
    fn send_in_real_time(noise: u8, seconds: u64) -> (Vec<f32>, Vec<Duration>, u32, u32) {
        send_with_callbacks(noise, seconds, 0)
    }

    /// As above, the microphone handing `callback` frames at a time at its
    /// own steady pace (PipeWire's quantum), or 5 to 15 ms with 0.
    fn send_with_callbacks(noise: u8, seconds: u64, callback: usize) -> (Vec<f32>, Vec<Duration>, u32, u32) {
        let shared = Arc::new(Shared::default());
        shared.filter.noise.store(noise, Ordering::Relaxed);
        // The gate always open, so only real breaks are silence.
        shared.filter.gate.store(false, Ordering::Relaxed);
        shared.filter.threshold.store((-100f32).to_bits(), Ordering::Relaxed);
        shared.filter.version.fetch_add(1, Ordering::Relaxed);
        let mut source = MicSource::new(Arc::clone(&shared));
        if noise >= 2 {
            // Let the model load before the clock starts, as a call does
            // while it connects.
            let _ = source.next_block();
            std::thread::sleep(Duration::from_secs(3));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let mic = {
            let shared = Arc::clone(&shared);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let start = Instant::now();
                let mut sent = 0usize;
                let mut seed = 7u32;
                while !stop.load(Ordering::Relaxed) {
                    let due = if callback == 0 {
                        // A sound server hands 5 to 15 ms at a time.
                        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                        let wait = 5 + (seed >> 16) % 11;
                        std::thread::sleep(Duration::from_millis(u64::from(wait)));
                        (start.elapsed().as_secs_f64() * f64::from(RATE)) as usize
                    } else {
                        let next = sent + callback;
                        // A little lateness, now and then more, as a busy
                        // desktop hands the microphone over.
                        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                        let late = f64::from((seed >> 16) % 3)
                            + if (seed >> 8) % 50 == 0 { 12.0 } else { 0.0 };
                        let at = start
                            + Duration::from_secs_f64(next as f64 / f64::from(RATE) + late / 1000.0);
                        if let Some(wait) = at.checked_duration_since(Instant::now()) {
                            std::thread::sleep(wait);
                        }
                        next
                    };
                    let block: Vec<f32> = (sent..due)
                        .map(|i| {
                            let t = i as f32 / RATE as f32;
                            // A voice-like tone that never stops.
                            (t * 2.0 * std::f32::consts::PI * 220.0).sin()
                                * (0.25 + 0.1 * (t * 3.0).sin())
                        })
                        .collect();
                    sent = due;
                    shared.mic.push(&block);
                }
            })
        };
        let start = Instant::now();
        let mut out = Vec::new();
        let mut took = Vec::new();
        let mut tick = 0u32;
        while start.elapsed() < Duration::from_secs(seconds) {
            tick += 1;
            let due = start + Duration::from_millis(20 * u64::from(tick));
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
            let began = Instant::now();
            out.extend(source.next_block());
            took.push(began.elapsed());
        }
        stop.store(true, Ordering::Relaxed);
        mic.join().unwrap();
        (
            out,
            took,
            shared.mic.underruns.load(Ordering::Relaxed),
            shared.mic.overruns.load(Ordering::Relaxed),
        )
    }

    /// Breaks in a sound that should never stop: runs of 1 ms or more
    /// near silence, after the first half second.
    fn breaks(sound: &[f32]) -> usize {
        let mut count = 0;
        let mut quiet = 0;
        for s in &sound[(RATE / 2) as usize..] {
            if s.abs() < 1e-3 {
                quiet += 1;
                if quiet == 48 {
                    count += 1;
                }
            } else {
                quiet = 0;
            }
        }
        count
    }

    #[test]
    #[ignore = "real time, ten seconds each"]
    fn realtime_send_quantum() {
        for callback in [256usize, 480, 1024, 2048] {
            let (out, _, under, over) = send_with_callbacks(0, 10, callback);
            println!(
                "microphone in blocks of {callback} frames: mic ring underruns {under} overruns {over}; {} breaks",
                breaks(&out)
            );
        }
    }

    #[test]
    #[ignore = "real time, ten seconds each"]
    fn realtime_send_path() {
        for noise in [0, 1, 2] {
            let (out, took, under, over) = send_in_real_time(noise, 10);
            let max = took.iter().max().copied().unwrap_or_default();
            let slow = took.iter().filter(|t| **t > Duration::from_millis(10)).count();
            let mean = took.iter().sum::<Duration>() / took.len().max(1) as u32;
            println!(
                "noise {noise}: {} blocks, filter mean {mean:?} max {max:?}, {slow} over 10 ms; mic ring underruns {under} overruns {over}; {} breaks",
                took.len(),
                breaks(&out),
            );
        }
    }

    /// The receive path in real time: songbird's ticks of 20 ms reaching
    /// the ring late by up to `jitter` ms (and now and then a stall of
    /// `stall` ms), and a sound card pulling `frames` at a time on a clock
    /// `skew` off. Returns the breaks heard and the ring's underruns.
    fn receive_in_real_time(jitter: u64, stall: u64, frames: usize, skew: f64, seconds: u64) -> (usize, u32, u32) {
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let network = {
            let shared = Arc::clone(&shared);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let start = Instant::now();
                let mut seed = 3u32;
                let mut n = 0u64;
                let mut last = start;
                while !stop.load(Ordering::Relaxed) {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    let mut late = if jitter > 0 { u64::from(seed >> 16) % jitter } else { 0 };
                    if stall > 0 && n % 250 == 125 {
                        late += stall;
                    }
                    let due = (start + Duration::from_millis(20 * n + late)).max(last);
                    if let Some(wait) = due.checked_duration_since(Instant::now()) {
                        std::thread::sleep(wait);
                    }
                    last = due;
                    let tick: Vec<f32> = (0..960)
                        .flat_map(|i| {
                            let t = (n as usize * 960 + i) as f32 / RATE as f32;
                            let s = (t * 2.0 * std::f32::consts::PI * 220.0).sin() * 0.3;
                            [s, s]
                        })
                        .collect();
                    shared.speakers.push(&tick);
                    n += 1;
                }
            })
        };
        let mut playback = Playback::new(Arc::clone(&shared), RATE, 2);
        let period = frames as f64 / f64::from(RATE) / skew;
        let start = Instant::now();
        let mut heard = Vec::new();
        let mut data = vec![0f32; frames * 2];
        let mut k = 0u32;
        while start.elapsed() < Duration::from_secs(seconds) {
            k += 1;
            let due = start + Duration::from_secs_f64(period * f64::from(k));
            if let Some(wait) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
            playback.fill(&mut data);
            heard.extend(data.iter().step_by(2));
        }
        stop.store(true, Ordering::Relaxed);
        network.join().unwrap();
        (
            breaks(&heard),
            shared.speakers.underruns.load(Ordering::Relaxed),
            shared.speakers.overruns.load(Ordering::Relaxed),
        )
    }

    #[test]
    #[ignore = "real time, a minute in all"]
    fn realtime_receive_path() {
        for (name, jitter, stall, frames, skew) in [
            ("ideal", 0, 0, 480, 1.0),
            ("PipeWire quantum 1024", 0, 0, 1024, 1.0),
            ("card 0.3% fast", 0, 0, 1024, 1.003),
            ("events up to 30 ms late", 30, 0, 1024, 1.0),
            ("events up to 50 ms late", 50, 0, 1024, 1.0),
            ("a 100 ms stall every 5 s", 10, 100, 1024, 1.0),
            ("card in blocks of 2048", 0, 0, 2048, 1.0),
            ("card 2048, events up to 10 ms late", 10, 0, 2048, 1.0),
            ("card 2048, events up to 20 ms late", 20, 0, 2048, 1.0),
            ("card 2048, events up to 30 ms late", 30, 0, 2048, 1.0),
        ] {
            let (b, under, over) = receive_in_real_time(jitter, stall, frames, skew, 10);
            println!("{name}: {b} breaks, underruns {under}, overruns {over}");
        }
    }

    /// A 32-bit float WAV's samples, first channel.
    fn read_wav(path: &str) -> Vec<f32> {
        let bytes = std::fs::read(path).unwrap();
        let mut pos = 12;
        let mut channels = 1usize;
        while pos + 8 <= bytes.len() {
            let id = &bytes[pos..pos + 4];
            let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let body = &bytes[pos + 8..(pos + 8 + size).min(bytes.len())];
            if id == b"fmt " {
                channels = usize::from(u16::from_le_bytes([body[2], body[3]]));
            }
            if id == b"data" {
                return body
                    .chunks_exact(4 * channels)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
            }
            pos += 8 + size + (size & 1);
        }
        panic!("no data in {path}");
    }

    fn write_wav(path: &str, samples: &[f32]) {
        let mut out = Vec::new();
        let data = (samples.len() * 4) as u32;
        out.extend(b"RIFF");
        out.extend((36 + data).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(3u16.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(RATE.to_le_bytes());
        out.extend((RATE * 4).to_le_bytes());
        out.extend(4u16.to_le_bytes());
        out.extend(32u16.to_le_bytes());
        out.extend(b"data");
        out.extend(data.to_le_bytes());
        for s in samples {
            out.extend(s.to_le_bytes());
        }
        std::fs::write(path, out).unwrap();
    }

    /// Runs a recording through the send filter as a call would, for each
    /// noise suppression, and writes what would be sent beside it:
    /// `DISCOFAST_FILTER_IN=voice.wav cargo test --release filter_a_recording -- --ignored`
    #[test]
    #[ignore = "needs a recording"]
    fn filter_a_recording() {
        let Ok(input) = std::env::var("DISCOFAST_FILTER_IN") else {
            return;
        };
        let samples = read_wav(&input);
        for noise in [0u8, 1, 2] {
            let shared = Arc::new(Shared::default());
            shared.filter.noise.store(noise, Ordering::Relaxed);
            shared.filter.version.fetch_add(1, Ordering::Relaxed);
            let mut source = MicSource::new(Arc::clone(&shared));
            if noise >= 2 {
                let _ = source.next_block();
                std::thread::sleep(Duration::from_secs(3));
            }
            let mut out = Vec::with_capacity(samples.len());
            for block in samples.chunks(BLOCK) {
                shared.mic.push(block);
                out.extend(source.next_block());
            }
            write_wav(&format!("{input}.sent-{noise}.wav"), &out);
        }
    }

    /// The real microphone for five seconds, opened as a call opens it:
    /// how many frames each callback brings and how far apart they come.
    /// `cargo test --release microphone_callbacks -- --ignored --nocapture`
    #[test]
    #[ignore = "uses the microphone"]
    fn microphone_callbacks() {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let device = host.default_input_device().expect("a microphone");
        let config = mic_config(&device).unwrap();
        println!("{} {:?} {:?}", device.name().unwrap_or_default(), config, config.buffer_size());
        let seen = Arc::new(Mutex::new(Vec::<(Instant, usize)>::new()));
        let log = Arc::clone(&seen);
        let channels = usize::from(config.channels());
        let stream = device
            .build_input_stream(
                &config.config(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    log.lock().unwrap().push((Instant::now(), data.len() / channels));
                },
                |e| println!("error {e}"),
                None,
            )
            .unwrap();
        stream.play().unwrap();
        std::thread::sleep(Duration::from_secs(5));
        drop(stream);
        let seen = seen.lock().unwrap();
        let sizes: HashSet<usize> = seen.iter().map(|s| s.1).collect();
        let gaps: Vec<f64> = seen.windows(2).map(|w| (w[1].0 - w[0].0).as_secs_f64() * 1000.0).collect();
        let total: usize = seen.iter().map(|s| s.1).sum();
        let mut sorted = gaps.clone();
        sorted.sort_by(f64::total_cmp);
        println!(
            "{} callbacks, frames {:?}, {} frames in 5 s; spacing median {:.1} ms, p99 {:.1} ms, max {:.1} ms",
            seen.len(),
            sizes,
            total,
            sorted[sorted.len() / 2],
            sorted[sorted.len() * 99 / 100],
            sorted.last().unwrap()
        );
    }

    #[test]
    fn the_voice_detector_holds_between_words_and_ignores_a_click() {
        let mut v = VoiceDetector::new();
        let quiet = vec![0.0005f32; CHUNK];
        let voice: Vec<f32> = (0..CHUNK).map(|i| (i as f32 * 0.3).sin() * 0.2).collect();
        for _ in 0..50 {
            assert!(!v.feed(&quiet, None, None));
        }
        // One loud chunk alone is a click.
        assert!(!v.feed(&voice, None, None));
        assert!(!v.feed(&quiet, None, None));
        assert!(!v.feed(&voice, None, None));
        assert!(v.feed(&voice, None, None));
        // A pause between words keeps it on, a long silence turns it off.
        for _ in 0..20 {
            assert!(v.feed(&quiet, None, None));
        }
        for _ in 0..20 {
            v.feed(&quiet, None, None);
        }
        assert!(!v.feed(&quiet, None, None));
    }
}

impl crate::app::App {
    /// Joins a voice channel, as a click on it does: opens the sound
    /// devices and tells the gateway, which answers with the voice server.
    pub fn join_voice(&mut self, channel: Id) {
        self.call_view = true;
        if self.call.as_ref().is_some_and(|c| c.channel == channel) {
            return;
        }
        let guild = self.model.channel(channel).and_then(|c| c.guild);
        if self.call.is_some() {
            self.backend().send(crate::backend::Command::LeaveVoice);
        }
        let shared = Arc::new(Shared::default());
        shared.deaf.store(self.settings.deafened, Ordering::Relaxed);
        shared.filter.set(&self.settings);
        shared.set_volumes(&self.settings);
        shared.set_gains(&self.settings);
        // The microphone test gives its devices to the call.
        self.mic_test = None;
        let audio = match Audio::start(
            Arc::clone(&shared),
            self.settings.input_device.clone(),
            self.settings.output_device.clone(),
        ) {
            Ok(audio) => Some(audio),
            Err(error) => {
                self.chat.dialog = Some(crate::chat::Dialog::Failed(error));
                None
            }
        };
        self.call = Some(Session {
            guild,
            channel,
            state: State::Joining,
            session_id: None,
            server: None,
            shared,
            _audio: audio,
            sent: false,
            since: Instant::now(),
            watching: None,
            room: None,
        });
        self.send_voice_state();
    }

    /// Calls a direct message or a group, as the phone in its header: the
    /// call starts with the user in it, and the others' apps ring.
    pub fn start_call(&mut self, channel: Id) {
        self.voice_taken = None;
        self.join_voice(channel);
        self.backend().send(crate::backend::Command::Api {
            tag: crate::backend::Tag::Admin(crate::admin::Tag::VoiceMember),
            method: reqwest::Method::POST,
            path: format!("/channels/{channel}/call/ring"),
            body: Some(serde_json::json!({ "recipients": null })),
        });
    }

    /// "Iniciar chamada" on someone: their direct message's call, opening
    /// the conversation first when there is none yet.
    pub fn call_user(&mut self, user: Id) {
        let existing = self.model.direct.iter().copied().find(|c| {
            self.model
                .channel(*c)
                .is_some_and(|c| c.kind == crate::model::ChannelKind::Direct && c.recipients == [user])
        });
        match existing {
            Some(channel) => {
                self.actions.push(crate::app::Action::OpenChannel(channel));
                self.start_call(channel);
            }
            None => {
                self.call_when_open = Some(user);
                self.apply_chat(crate::chat::ChatAction::MessageUser {
                    user,
                    text: String::new(),
                });
            }
        }
    }

    /// Picks up the ringing call, or stops it ringing for the user.
    pub fn answer_call(&mut self, accept: bool) {
        let Some((channel, _)) = self.ringing.take() else {
            return;
        };
        if accept {
            self.voice_taken = None;
            self.join_voice(channel);
            self.actions.push(crate::app::Action::OpenChannel(channel));
        } else {
            let me = self.model.me;
            self.backend().send(crate::backend::Command::Api {
                tag: crate::backend::Tag::Admin(crate::admin::Tag::VoiceMember),
                method: reqwest::Method::POST,
                path: format!("/channels/{channel}/call/stop-ringing"),
                body: Some(serde_json::json!({ "recipients": [me.to_string()] })),
            });
        }
    }

    /// CALL_CREATE, CALL_UPDATE and CALL_DELETE: whether a direct message
    /// call rings for the user.
    pub fn call_event(&mut self, kind: &str, data: &serde_json::Value) {
        let Some(channel) = crate::discord::parse::id(&data["channel_id"]) else {
            return;
        };
        for state in data["voice_states"].as_array().into_iter().flatten() {
            crate::discord::parse::voice_state(state, None, &mut self.model);
        }
        let me = self.model.me;
        let rings = kind != "CALL_DELETE"
            && data["ringing"]
                .as_array()
                .is_some_and(|l| l.iter().any(|u| crate::discord::parse::id(u) == Some(me)))
            && self.call.as_ref().is_none_or(|c| c.channel != channel);
        match (&self.ringing, rings) {
            (None, true) => {
                self.ringing = Some((channel, Instant::now()));
                self.sound(crate::sounds::Sound::Ring);
            }
            (Some((ringing, _)), false) if *ringing == channel => self.ringing = None,
            _ => {}
        }
    }

    /// While a call rings: the ring again every few seconds.
    pub fn keep_ringing(&mut self) {
        if let Some((channel, last)) = self.ringing
            && last.elapsed() >= Duration::from_millis(2_600)
        {
            self.ringing = Some((channel, Instant::now()));
            self.sound(crate::sounds::Sound::Ring);
        }
    }

    pub fn leave_voice(&mut self) {
        let Some(call) = self.call.take() else {
            return;
        };
        if call.room.is_some() {
            self.backend().send(crate::backend::Command::LeaveRoom);
        }
        self.sound(crate::sounds::Sound::Leave);
        self.call_view = false;
        self.backend().send(crate::backend::Command::VoiceState {
            guild: call.guild,
            channel: None,
            mute: self.settings.muted,
            deaf: self.settings.deafened,
        });
        self.backend().send(crate::backend::Command::LeaveVoice);
        let me = self.model.me;
        self.model.voice.remove(&(call.guild, me));
    }

    /// The gateway hears of mute and deafen; the driver stops sending.
    pub fn send_voice_state(&self) {
        let Some(call) = &self.call else {
            return;
        };
        let (mute, deaf) = (self.settings.muted, self.settings.deafened);
        call.shared.deaf.store(deaf, Ordering::Relaxed);
        self.backend().send(crate::backend::Command::VoiceState {
            guild: call.guild,
            channel: Some(call.channel),
            mute,
            deaf,
        });
        self.backend()
            .send(crate::backend::Command::VoiceMute(mute || deaf || self.in_audience()));
    }

    /// In a stage's audience: the microphone stays off whatever the
    /// mute button says.
    pub fn in_audience(&self) -> bool {
        let Some(call) = &self.call else { return false };
        self.model
            .channel(call.channel)
            .is_some_and(|c| c.kind == crate::model::ChannelKind::Stage)
            && self
                .model
                .voice
                .get(&(call.guild, self.model.me))
                .is_none_or(|v| v.suppress)
    }

    /// VOICE_STATE_UPDATE: someone moved; for the user, the session the
    /// voice server needs, or the call ending elsewhere.
    pub fn voice_state_event(&mut self, data: &serde_json::Value) {
        let before = crate::discord::parse::id(&data["user_id"]).and_then(|user| {
            let guild = crate::discord::parse::id(&data["guild_id"]);
            self.model.voice.get(&(guild, user)).map(|v| v.channel)
        });
        let Some(user) = crate::discord::parse::voice_state(data, None, &mut self.model) else {
            return;
        };
        if user != self.model.me {
            // Someone coming into or leaving the user's channel.
            if let Some(mine) = self.call.as_ref().map(|c| c.channel) {
                let after = crate::discord::parse::id(&data["channel_id"]);
                if after == Some(mine) && before != Some(mine) {
                    self.sound(crate::sounds::Sound::Arrive);
                } else if before == Some(mine) && after != Some(mine) {
                    self.sound(crate::sounds::Sound::Depart);
                }
            }
            return;
        }
        let channel = crate::discord::parse::id(&data["channel_id"]);
        // Brought up to or down from a stage: the microphone follows.
        if self.call.as_ref().is_some_and(|c| Some(c.channel) == channel) {
            let audience = self.in_audience();
            self.backend().send(crate::backend::Command::VoiceMute(
                self.settings.muted || self.settings.deafened || audience,
            ));
        }
        let Some(call) = &mut self.call else {
            return;
        };
        // The same account on another device: joining a call there takes
        // this one's place, as in Discord; leaving there changes nothing.
        let session = data["session_id"].as_str().unwrap_or_default();
        if !self.model.session.is_empty() && session != self.model.session {
            if channel.is_some() {
                self.voice_taken = Some(call.channel);
                self.call = None;
                self.call_view = false;
                self.backend().send(crate::backend::Command::LeaveVoice);
            }
            return;
        }
        match channel {
            None => {
                // Disconnected from elsewhere, or by a moderator.
                self.call = None;
                self.call_view = false;
                self.backend().send(crate::backend::Command::LeaveVoice);
            }
            Some(channel) => {
                call.channel = channel;
                call.session_id = data["session_id"].as_str().map(str::to_owned);
                self.connect_voice();
            }
        }
    }

    /// VOICE_SERVER_UPDATE: where the call's sound goes.
    pub fn voice_server_event(&mut self, data: &serde_json::Value) {
        let Some(call) = &mut self.call else {
            return;
        };
        let (Some(endpoint), Some(token)) = (data["endpoint"].as_str(), data["token"].as_str())
        else {
            return;
        };
        call.server = Some((endpoint.to_owned(), token.to_owned()));
        // A new server (a region change) means connecting again.
        call.sent = false;
        self.connect_voice();
    }

    fn connect_voice(&mut self) {
        let me = self.model.me;
        let Some(call) = &mut self.call else {
            return;
        };
        let (Some(session_id), Some((endpoint, token))) = (&call.session_id, &call.server) else {
            return;
        };
        if call.sent {
            return;
        }
        call.sent = true;
        call.state = State::Connecting;
        let server = crate::discord::call::Server {
            place: call.guild.unwrap_or(call.channel),
            channel: call.channel,
            user: me,
            session_id: session_id.clone(),
            endpoint: endpoint.clone(),
            token: token.clone(),
        };
        let shared = Arc::clone(&call.shared);
        self.backend()
            .send(crate::backend::Command::ConnectVoice { server, shared });
    }

    pub fn voice_connected(&mut self, result: Result<(), String>) {
        let Some(call) = &mut self.call else {
            return;
        };
        match result {
            Ok(()) => {
                call.state = State::Connected;
                self.send_voice_state();
                self.sound(crate::sounds::Sound::Join);
                self.join_room();
            }
            Err(error) => {
                log::warn!("voice: {error}");
                call.state = State::Failed(error);
                self.sound(crate::sounds::Sound::Lost);
            }
        }
    }

    /// Starts watching someone's screen share: the gateway is asked, and
    /// answers with where the share's own voice server is.
    pub fn watch_stream(&mut self, user: Id) {
        if !CAN_WATCH {
            return;
        }
        let Some(call) = &mut self.call else {
            return;
        };
        let key = stream_key(call.guild, call.channel, user);
        self.call_view = true;
        if call.watching.as_ref().is_some_and(|w| w.key == key) {
            return;
        }
        let old = call.watching.replace(Watch {
            user,
            key: key.clone(),
            state: State::Joining,
            rtc_server_id: None,
            server: None,
            sent: false,
            screen: Arc::default(),
            texture: None,
        });
        if let Some(old) = old {
            self.backend()
                .send(crate::backend::Command::StopStream { key: old.key });
        }
        self.backend()
            .send(crate::backend::Command::WatchStream { key });
    }

    pub fn stop_watching(&mut self) {
        let Some(watch) = self.call.as_mut().and_then(|c| c.watching.take()) else {
            return;
        };
        self.backend()
            .send(crate::backend::Command::StopStream { key: watch.key });
    }

    /// The watched share, if `data` is about it.
    fn watched(&mut self, data: &serde_json::Value) -> Option<&mut Watch> {
        let key = data["stream_key"].as_str()?;
        self.call
            .as_mut()?
            .watching
            .as_mut()
            .filter(|w| w.key == key)
    }

    /// STREAM_CREATE and STREAM_UPDATE: the share's voice server id.
    pub fn stream_create_event(&mut self, data: &serde_json::Value) {
        let Some(watch) = self.watched(data) else {
            return;
        };
        let Some(id) = crate::discord::parse::id(&data["rtc_server_id"]) else {
            return;
        };
        if watch.rtc_server_id != Some(id) {
            watch.rtc_server_id = Some(id);
            watch.sent = false;
        }
        self.connect_stream();
    }

    /// STREAM_SERVER_UPDATE: where the share's voice server is.
    pub fn stream_server_event(&mut self, data: &serde_json::Value) {
        let Some(watch) = self.watched(data) else {
            return;
        };
        let (Some(endpoint), Some(token)) = (data["endpoint"].as_str(), data["token"].as_str())
        else {
            return;
        };
        watch.server = Some((endpoint.to_owned(), token.to_owned()));
        watch.sent = false;
        self.connect_stream();
    }

    /// STREAM_DELETE: the share ended, or the user may not watch it.
    pub fn stream_delete_event(&mut self, data: &serde_json::Value) {
        if self.watched(data).is_some() {
            self.stop_watching();
        }
    }

    fn connect_stream(&mut self) {
        let me = self.model.me;
        let Some(call) = &mut self.call else {
            return;
        };
        let Some(session_id) = call.session_id.clone() else {
            return;
        };
        let Some(watch) = &mut call.watching else {
            return;
        };
        let (Some(rtc_server_id), Some((endpoint, token))) = (watch.rtc_server_id, &watch.server)
        else {
            return;
        };
        if watch.sent {
            return;
        }
        watch.sent = true;
        watch.state = State::Connecting;
        let server = crate::discord::stream::Server {
            rtc_server_id,
            user: me,
            streamer: watch.user,
            session_id,
            endpoint: endpoint.clone(),
            token: token.clone(),
        };
        let screen = Arc::clone(&watch.screen);
        self.backend()
            .send(crate::backend::Command::ConnectStream { server, screen });
    }

    pub fn stream_connected(&mut self, result: Result<(), String>) {
        let Some(watch) = self.call.as_mut().and_then(|c| c.watching.as_mut()) else {
            return;
        };
        match result {
            Ok(()) => watch.state = State::Connected,
            Err(error) => {
                log::warn!("screen share: {error}");
                watch.state = State::Failed(error);
            }
        }
    }

    /// Whether a video server of the user's own is set, and this build can
    /// use it.
    pub fn has_video_server(&self) -> bool {
        crate::room::AVAILABLE
            && !self.settings.video_server.trim().is_empty()
            && !self.settings.video_key.trim().is_empty()
    }

    /// Joins the call's room on the user's own video server, once the
    /// voice is connected.
    pub fn join_room(&mut self) {
        if !self.has_video_server() {
            return;
        }
        let me = self.model.me;
        let name = self
            .model
            .user(me)
            .map(|u| u.name.clone())
            .unwrap_or_default();
        let server = crate::room::Server {
            url: self.settings.video_server.trim().to_owned(),
            key: self.settings.video_key.trim().to_owned(),
        };
        let Some(call) = &mut self.call else {
            return;
        };
        if call.room.is_some() {
            return;
        }
        let videos = Arc::new(crate::room::Videos::default());
        call.room = Some(OwnRoom {
            state: State::Connecting,
            videos: Arc::clone(&videos),
            camera: false,
            screen: false,
            focus: None,
            textures: HashMap::new(),
        });
        let room = crate::room::room_name(call.channel);
        self.backend().send(crate::backend::Command::JoinRoom {
            server,
            room,
            me,
            name,
            videos,
        });
    }

    pub fn room_connected(&mut self, result: Result<(), String>) {
        let Some(room) = self.call.as_mut().and_then(|c| c.room.as_mut()) else {
            return;
        };
        match result {
            Ok(()) => room.state = State::Connected,
            Err(error) => {
                log::warn!("video server: {error}");
                room.state = State::Failed(error);
                room.camera = false;
                room.screen = false;
            }
        }
    }

    /// The camera or screen on or off in the room.
    pub fn toggle_video(&mut self, kind: crate::room::Kind) {
        let Some(room) = self.call.as_mut().and_then(|c| c.room.as_mut()) else {
            return;
        };
        if room.state != State::Connected {
            return;
        }
        let control = match kind {
            crate::room::Kind::Camera => {
                room.camera = !room.camera;
                crate::room::Control::Camera(room.camera)
            }
            crate::room::Kind::Screen => {
                room.screen = !room.screen;
                crate::room::Control::Screen(room.screen)
            }
        };
        self.backend()
            .send(crate::backend::Command::RoomControl(control));
    }

    /// Whether someone in the call is talking now; the user by their
    /// microphone, unless muted.
    pub fn talking(&self, user: Id) -> bool {
        let Some(call) = &self.call else {
            return false;
        };
        if user == self.model.me {
            return !self.settings.muted
                && !self.settings.deafened
                && call.state == State::Connected
                && call.shared.talking.load(Ordering::Relaxed);
        }
        call.shared
            .speaking
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&user)
    }
}
