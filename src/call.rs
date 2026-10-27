//! A voice call's sound on this machine: the microphone into a ring the
//! voice driver reads, and what the others say from a ring into the
//! speakers. Both at Discord's 48 kHz stereo. The streams stay on the
//! interface thread (they cannot move between threads on every platform);
//! the rings are shared with the driver in the backend.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
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
/// The most a ring keeps: a fifth of a second. Older sound is dropped so
/// the delay never grows.
const RING_SECONDS: f32 = 0.2;

/// Samples handed from one thread to another. Reading starts only once
/// `cushion` samples are in, and again after it runs dry, so the small
/// differences in when each side runs do not become gaps that click.
pub struct Ring {
    state: Mutex<RingState>,
    cushion: usize,
    max: usize,
}

struct RingState {
    samples: VecDeque<f32>,
    primed: bool,
}

impl Ring {
    /// A ring of `channels` interleaved, holding `cushion` seconds before
    /// it plays.
    pub fn new(channels: usize, cushion: f32) -> Self {
        let per_second = RATE as f32 * channels as f32;
        Self {
            state: Mutex::new(RingState {
                samples: VecDeque::new(),
                primed: false,
            }),
            cushion: (per_second * cushion) as usize,
            max: (per_second * RING_SECONDS) as usize,
        }
    }

    pub fn push(&self, data: &[f32]) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.samples.extend(data.iter().copied());
        let over = state.samples.len().saturating_sub(self.max);
        state.samples.drain(..over);
    }

    pub fn len(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .samples
            .len()
    }

    /// Fills `out`, with silence while the cushion builds.
    pub fn pop(&self, out: &mut [f32]) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.primed {
            if state.samples.len() < self.cushion.max(out.len()) {
                out.fill(0.0);
                return;
            }
            state.primed = true;
        }
        let have = state.samples.len().min(out.len());
        for (slot, sample) in out.iter_mut().zip(state.samples.drain(..have)) {
            *slot = sample;
        }
        out[have..].fill(0.0);
        if have < out.len() {
            // Ran dry: build the cushion again.
            state.primed = false;
        }
    }
}

/// The voice filter's settings, which can change during a call.
pub struct Filter {
    /// 0 off, 1 moderate, 2 high, 3 very high.
    pub noise: AtomicU8,
    pub echo: AtomicBool,
    pub gain: AtomicBool,
    /// Sends nothing while the user is not talking, as Discord's
    /// automatic input sensitivity.
    pub gate: AtomicBool,
    /// Bumped on a change, for the filter to be built again.
    pub version: AtomicU32,
}

impl Filter {
    pub fn set(&self, settings: &crate::settings::Settings) {
        self.noise
            .store(settings.noise_suppression, Ordering::Relaxed);
        self.echo
            .store(settings.echo_cancellation, Ordering::Relaxed);
        self.gain.store(settings.auto_gain, Ordering::Relaxed);
        self.gate
            .store(settings.auto_sensitivity, Ordering::Relaxed);
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
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            mic: Ring::new(1, 0.04),
            played: Ring::new(1, 0.0),
            speakers: Ring::new(CHANNELS, 0.06),
            deaf: AtomicBool::new(false),
            filter: Filter {
                noise: AtomicU8::new(2),
                echo: AtomicBool::new(true),
                gain: AtomicBool::new(true),
                gate: AtomicBool::new(true),
                version: AtomicU32::new(0),
            },
            talking: AtomicBool::new(false),
            speaking: Mutex::new(HashSet::new()),
            volumes: Mutex::new(HashMap::new()),
        }
    }
}

impl Shared {
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
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("call::Shared")
    }
}

/// WebRTC's processing as the settings ask: echo cancelling against what
/// the speakers play, noise suppression and level control.
fn processor(filter: &Filter) -> Option<sonora::AudioProcessing> {
    use sonora::config::{
        AdaptiveDigital, EchoCanceller, GainController2, HighPassFilter, NoiseSuppression,
        NoiseSuppressionLevel,
    };
    let noise = match filter.noise.load(Ordering::Relaxed) {
        0 => None,
        1 => Some(NoiseSuppressionLevel::Moderate),
        2 => Some(NoiseSuppressionLevel::High),
        _ => Some(NoiseSuppressionLevel::VeryHigh),
    };
    let echo = filter.echo.load(Ordering::Relaxed);
    let gain = filter.gain.load(Ordering::Relaxed);
    if noise.is_none() && !echo && !gain {
        return None;
    }
    let stream = sonora::StreamConfig::new(RATE, 1);
    Some(
        sonora::AudioProcessing::builder()
            .config(sonora::Config {
                high_pass_filter: Some(HighPassFilter::default()),
                echo_canceller: echo.then(EchoCanceller::default),
                noise_suppression: noise.map(|level| NoiseSuppression {
                    level,
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

    /// One 10 ms chunk; whether the user is talking.
    fn feed(&mut self, chunk: &[f32]) -> bool {
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        let db = 20.0 * rms.max(1e-6).log10();
        // The floor follows quiet quickly, and a steady sound (a fan the
        // level control raised) within about half a second; speech rises
        // and falls too often to be taken for it.
        self.floor = if db < self.floor {
            self.floor + (db - self.floor) * 0.2
        } else {
            self.floor + (db - self.floor) * 0.01
        };
        let loud = db > (self.floor + 10.0).max(-45.0);
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
/// quiet, as raw little-endian f32 stereo; never ending.
pub struct MicSource {
    shared: Arc<Shared>,
    pending: Vec<u8>,
    filter: Mutex<Filtering>,
}

struct Filtering {
    processor: Option<sonora::AudioProcessing>,
    version: u32,
    voice: VoiceDetector,
    /// The gate's gain now, moving smoothly so it never clicks.
    open: f32,
}

impl MicSource {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            filter: Mutex::new(Filtering {
                processor: processor(&shared.filter),
                version: shared.filter.version.load(Ordering::Relaxed),
                voice: VoiceDetector::new(),
                open: 0.0,
            }),
            shared,
            pending: Vec::new(),
        }
    }

    /// Twenty milliseconds of the microphone, filtered, as stereo.
    fn next_block(&mut self) -> Vec<f32> {
        let shared = &self.shared;
        // Wait briefly for the microphone to catch up instead of sending a
        // gap; then send what there is.
        let wanted = CHUNK * 2;
        let start = Instant::now();
        while shared.mic.len() < wanted && start.elapsed() < Duration::from_millis(12) {
            std::thread::sleep(Duration::from_millis(2));
        }
        let mut mono = vec![0f32; wanted];
        shared.mic.pop(&mut mono);
        let mut filtering = self.filter.lock().unwrap_or_else(|e| e.into_inner());
        let f = &mut *filtering;
        let version = shared.filter.version.load(Ordering::Relaxed);
        if version != f.version {
            f.version = version;
            f.processor = processor(&shared.filter);
        }
        let gate = shared.filter.gate.load(Ordering::Relaxed);
        let mut talking = false;
        for chunk in mono.chunks_mut(CHUNK) {
            if let Some(p) = &mut f.processor {
                let mut played = vec![0f32; CHUNK];
                if shared.played.len() >= CHUNK {
                    shared.played.pop(&mut played);
                }
                let mut sink = vec![0f32; CHUNK];
                let _ = p.process_render_f32(&[&played], &mut [&mut sink]);
                let mut out = vec![0f32; CHUNK];
                if p.process_capture_f32(&[&*chunk], &mut [&mut out]).is_ok() {
                    chunk.copy_from_slice(&out);
                }
            }
            talking = f.voice.feed(chunk);
            let target = if !gate || talking { 1.0 } else { 0.0 };
            for sample in chunk.iter_mut() {
                f.open += (target - f.open) * 0.01;
                *sample *= f.open;
            }
        }
        shared.talking.store(talking, Ordering::Relaxed);
        mono.iter().flat_map(|s| [*s, *s]).collect()
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

/// Turns a device's rate and channels into 48 kHz stereo, a callback at a
/// time.
struct Resampler {
    from: u32,
    channels: usize,
    position: f64,
    last: [f32; 2],
}

impl Resampler {
    fn new(from: u32, channels: usize) -> Self {
        Self {
            from,
            channels: channels.max(1),
            position: 0.0,
            last: [0.0; 2],
        }
    }

    fn feed(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let frames: Vec<[f32; 2]> = input
            .chunks_exact(self.channels)
            .map(|f| match f.len() {
                1 => [f[0], f[0]],
                _ => [f[0], f[1]],
            })
            .collect();
        if self.from == RATE {
            out.extend(frames.iter().flatten());
            return;
        }
        let step = f64::from(self.from) / f64::from(RATE);
        while (self.position as usize) < frames.len() {
            let i = self.position as usize;
            let t = (self.position - i as f64) as f32;
            let a = if i == 0 { self.last } else { frames[i - 1] };
            let b = frames[i];
            out.push(a[0] + (b[0] - a[0]) * t);
            out.push(a[1] + (b[1] - a[1]) * t);
            self.position += step;
        }
        self.position -= frames.len() as f64;
        if let Some(last) = frames.last() {
            self.last = *last;
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
    pub fn start(shared: Arc<Shared>) -> Result<Self, String> {
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
        Ok(Self {
            _mic: mic,
            _speakers: speakers,
        })
    }
}

fn open_mic(device: &cpal::Device, shared: &Arc<Shared>) -> Result<cpal::Stream, String> {
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
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
    let mut converted = Vec::new();
    let mut out = Vec::new();
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            converted.clear();
            converted.extend(
                data.iter()
                    .map(|s| <f32 as FromSample<T>>::from_sample_(*s)),
            );
            out.clear();
            resampler.feed(&converted, &mut out);
            // The filter works on one channel.
            let mono: Vec<f32> = out
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[l, r]| (l + r) * 0.5)
                .collect();
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
    let config: cpal::StreamConfig = config.into();
    let shared = Arc::clone(shared);
    // 48 kHz frames waiting to be played, and how far into them.
    let mut pending: VecDeque<[f32; 2]> = VecDeque::new();
    let mut position = 0f64;
    let mut pulled = Vec::new();
    let step = f64::from(RATE) / f64::from(rate);
    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels.max(1);
                // Exactly the frames this callback covers, no more, so the
                // ring is not drained faster than it fills.
                let needed = (position + frames as f64 * step).ceil() as usize + 1;
                if pending.len() < needed {
                    pulled.resize((needed - pending.len()) * CHANNELS, 0.0);
                    shared.speakers.pop(&mut pulled);
                    pending.extend(pulled.as_chunks::<2>().0.iter().copied());
                }
                let deaf = shared.deaf.load(Ordering::Relaxed);
                for (i, frame) in data.chunks_mut(channels.max(1)).enumerate() {
                    let at = position + i as f64 * step;
                    let k = at as usize;
                    let t = (at - k as f64) as f32;
                    let a = pending.get(k).copied().unwrap_or([0.0; 2]);
                    let b = pending.get(k + 1).copied().unwrap_or(a);
                    let (l, r) = if deaf {
                        (0.0, 0.0)
                    } else {
                        (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t)
                    };
                    for (c, sample) in frame.iter_mut().enumerate() {
                        *sample = if c % 2 == 0 { l } else { r };
                    }
                }
                let advanced = position + frames as f64 * step;
                let used = (advanced as usize).min(pending.len());
                pending.drain(..used);
                position = advanced - used as f64;
            },
            |error| log::warn!("call speakers: {error}"),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_rate_comes_out_at_48k_stereo() {
        let mut r = Resampler::new(44_100, 1);
        let mut out = Vec::new();
        // One second fed in pieces, as callbacks come.
        for _ in 0..10 {
            r.feed(&vec![0.5; 4_410], &mut out);
        }
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() < 10, "{frames}");
        assert!(out[10..].iter().all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn the_ring_waits_for_its_cushion_and_drops_old_sound() {
        let ring = Ring::new(1, 0.01);
        let mut out = vec![9.0; 100];
        ring.push(&[1.0; 200]);
        // 480 samples of cushion: not yet.
        ring.pop(&mut out);
        assert!(out.iter().all(|s| *s == 0.0));
        ring.push(&[1.0; 400]);
        ring.pop(&mut out);
        assert!(out.iter().all(|s| *s == 1.0));
        // Too much: only the newest fifth of a second stays.
        ring.push(&vec![2.0; 20_000]);
        assert_eq!(ring.len(), 9_600);
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
            assert_eq!(out.len(), CHUNK * 4);
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
    fn the_voice_detector_holds_between_words_and_ignores_a_click() {
        let mut v = VoiceDetector::new();
        let quiet = vec![0.0005f32; CHUNK];
        let voice: Vec<f32> = (0..CHUNK).map(|i| (i as f32 * 0.3).sin() * 0.2).collect();
        for _ in 0..50 {
            assert!(!v.feed(&quiet));
        }
        // One loud chunk alone is a click.
        assert!(!v.feed(&voice));
        assert!(!v.feed(&quiet));
        assert!(!v.feed(&voice));
        assert!(v.feed(&voice));
        // A pause between words keeps it on, a long silence turns it off.
        for _ in 0..20 {
            assert!(v.feed(&quiet));
        }
        for _ in 0..20 {
            v.feed(&quiet);
        }
        assert!(!v.feed(&quiet));
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
        let audio = match Audio::start(Arc::clone(&shared)) {
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
        });
        self.send_voice_state();
    }

    pub fn leave_voice(&mut self) {
        let Some(call) = self.call.take() else {
            return;
        };
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
            .send(crate::backend::Command::VoiceMute(mute || deaf));
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
        let Some(call) = &mut self.call else {
            return;
        };
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
            }
            Err(error) => {
                log::warn!("voice: {error}");
                call.state = State::Failed(error);
                self.sound(crate::sounds::Sound::Lost);
            }
        }
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
