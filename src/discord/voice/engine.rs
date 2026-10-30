//! The voice engine: libwebrtc's, the one Discord's web client runs in the
//! browser. The platform audio device records and plays (PulseAudio or
//! PipeWire on Linux), WebRTC's audio processing cancels the echo with
//! what was really played, NetEq evens out the network, and Opus sends
//! with forward error correction and DTX.
//!
//! Discofast's own part sits on the device's sound on its way through
//! (see vendor/webrtc-sys/src/discofast.cpp): the microphone's volume and
//! the soundboard are added before the engine takes it, and the speakers'
//! volume and deafen after the engine mixes everyone.

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use libwebrtc::audio_track::RtcAudioTrack;
use libwebrtc::peer_connection_factory::PeerConnectionFactory;
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;

use crate::call::Shared;

/// The rate the soundboard's samples are at.
const EFFECT_RATE: u32 = crate::call::RATE;

pub struct Engine {
    pub factory: PeerConnectionFactory,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
/// The call's or the microphone test's shared state, for the device taps.
static CURRENT: RwLock<Option<Arc<Shared>>> = RwLock::new(None);

/// The engine, made the first time: the platform audio device is taken
/// for good, as a browser keeps its own.
pub fn engine() -> &'static Engine {
    ENGINE.get_or_init(|| {
        let factory = PeerConnectionFactory::default();
        factory.set_adm_recording_enabled(true);
        factory.set_adm_playout_enabled(true);
        if !factory.acquire_platform_adm() {
            log::warn!("voice: the platform audio device did not start");
        }
        libwebrtc::native::discofast::set_recorded(Some(Arc::new(recorded)));
        libwebrtc::native::discofast::set_playout(Some(Arc::new(playout)));
        log::info!(
            "voice engine: {} microphones, {} outputs",
            factory.recording_devices(),
            factory.playout_devices()
        );
        Engine { factory }
    })
}

/// Hands the device taps a call's (or test's) shared state.
pub fn attach(shared: &Arc<Shared>) {
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::clone(shared));
}

/// Takes it back, if it is still that one.
pub fn detach(shared: &Arc<Shared>) {
    let mut current = CURRENT.write().unwrap_or_else(|e| e.into_inner());
    if current.as_ref().is_some_and(|c| Arc::ptr_eq(c, shared)) {
        *current = None;
    }
}

fn current() -> Option<Arc<Shared>> {
    CURRENT.read().unwrap_or_else(|e| e.into_inner()).clone()
}

impl Engine {
    /// The microphone as a track, with the processing the settings ask for:
    /// echo cancellation and automatic gain as the web client's
    /// getUserMedia constraints. Noise suppression stays off.
    pub fn microphone(&self, shared: &Shared) -> RtcAudioTrack {
        let echo = shared.filter.echo.load(Ordering::Relaxed);
        let gain = shared.filter.gain.load(Ordering::Relaxed);
        self.factory
            .create_device_audio_track_with("discofast-microphone", echo, gain, false)
    }
}

/// Interleaved 16-bit sound from the microphone, before the engine.
fn recorded(samples: &mut [i16], channels: usize, rate: u32) {
    let Some(shared) = current() else {
        return;
    };
    let channels = channels.max(1);
    let gain = f32::from_bits(shared.mic_gain.load(Ordering::Relaxed));
    if gain != 1.0 {
        for sample in samples.iter_mut() {
            *sample = (f32::from(*sample) * gain).clamp(-32_768.0, 32_767.0) as i16;
        }
    }
    if let Ok(mut analyser) = shared.analyser.try_lock() {
        analyser.push(samples, channels, rate);
    }
    mix(&shared.effect_out, samples, channels, rate, 1.0);
}

/// Everyone mixed, on the way to the speakers.
fn playout(samples: &mut [i16], channels: usize, rate: u32) {
    let Some(shared) = current() else {
        return;
    };
    let channels = channels.max(1);
    mix(&shared.effect_in, samples, channels, rate, 1.0);
    let gain = if shared.deaf.load(Ordering::Relaxed) {
        0.0
    } else {
        f32::from_bits(shared.speaker_gain.load(Ordering::Relaxed))
    };
    if gain != 1.0 {
        for sample in samples.iter_mut() {
            *sample = (f32::from(*sample) * gain).clamp(-32_768.0, 32_767.0) as i16;
        }
    }
}

/// Adds queued 48 kHz mono sound, at the device's rate.
fn mix(queue: &Mutex<VecDeque<f32>>, samples: &mut [i16], channels: usize, rate: u32, gain: f32) {
    let Ok(mut queue) = queue.try_lock() else {
        return;
    };
    if queue.is_empty() || rate == 0 {
        return;
    }
    let frames = samples.len() / channels;
    let step = f64::from(EFFECT_RATE) / f64::from(rate);
    let mut position = 0.0f64;
    for frame in samples.chunks_mut(channels).take(frames) {
        let Some(value) = queue.get(position as usize).copied() else {
            break;
        };
        let add = value * gain * 32_767.0;
        for sample in frame {
            *sample = (f32::from(*sample) + add).clamp(-32_768.0, 32_767.0) as i16;
        }
        position += step;
    }
    let used = (position as usize).min(queue.len());
    queue.drain(..used);
}

/// The microphone's level as Discord's web client measures it: an
/// AnalyserNode (FFT of 512, smoothing 0.1) read every 20 ms, the loudest
/// frequency in dB.
pub struct Analyser {
    ring: VecDeque<f32>,
    smoothed: Vec<f32>,
    fft: Arc<dyn realfft::RealToComplex<f32>>,
    window: Vec<f32>,
}

const FFT: usize = 512;
const SMOOTHING: f32 = 0.1;

impl Default for Analyser {
    fn default() -> Self {
        let fft = realfft::RealFftPlanner::<f32>::new().plan_fft_forward(FFT);
        // Web Audio's Blackman window.
        let window = (0..FFT)
            .map(|n| {
                let x = std::f32::consts::TAU * n as f32 / FFT as f32;
                0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos()
            })
            .collect();
        Self {
            ring: VecDeque::with_capacity(FFT * 2),
            smoothed: vec![0.0; FFT / 2 + 1],
            fft,
            window,
        }
    }
}

impl Analyser {
    fn push(&mut self, samples: &[i16], channels: usize, _rate: u32) {
        for frame in samples.chunks(channels) {
            let sum: f32 = frame.iter().map(|s| f32::from(*s)).sum();
            self.ring.push_back(sum / channels as f32 / 32_768.0);
        }
        while self.ring.len() > FFT {
            self.ring.pop_front();
        }
    }

    /// The loudest frequency now, in dB (`getFloatFrequencyData`, from the
    /// fifth bin on, below 0).
    pub fn volume(&mut self) -> f32 {
        if self.ring.len() < FFT {
            return f32::NEG_INFINITY;
        }
        let mut input: Vec<f32> = self
            .ring
            .iter()
            .zip(&self.window)
            .map(|(s, w)| s * w)
            .collect();
        let mut output = self.fft.make_output_vec();
        if self.fft.process(&mut input, &mut output).is_err() {
            return f32::NEG_INFINITY;
        }
        let mut loudest = f32::NEG_INFINITY;
        for (k, (bin, smooth)) in output.iter().zip(self.smoothed.iter_mut()).enumerate() {
            let magnitude = bin.norm() / FFT as f32;
            *smooth = SMOOTHING * *smooth + (1.0 - SMOOTHING) * magnitude;
            if k < 4 {
                continue;
            }
            let db = 20.0 * smooth.log10();
            if db > loudest && db < 0.0 {
                loudest = db;
            }
        }
        loudest
    }
}

/// Discord's voice activity (its class `V`): talking while any of the last
/// ten 20 ms readings was over the threshold.
pub struct Activity {
    history: [bool; 10],
    index: usize,
    counter: usize,
    silent: usize,
}

impl Default for Activity {
    fn default() -> Self {
        Self {
            history: [false; 10],
            index: 0,
            counter: 0,
            silent: 10,
        }
    }
}

impl Activity {
    pub fn update(&mut self, volume: f32, threshold: f32) -> bool {
        if self.history[self.index] {
            self.counter -= 1;
        }
        let loud = volume > threshold;
        self.history[self.index] = loud;
        if loud {
            self.counter += 1;
        }
        self.index = (self.index + 1) % self.history.len();
        if self.counter > 0 {
            self.silent = 0;
        } else {
            self.silent += 1;
        }
        self.counter > 0 || self.silent < self.history.len()
    }
}

/// The threshold in use: the manual one, or for automatic sensitivity the
/// level Discord's web client starts from.
pub fn threshold(shared: &Shared) -> f32 {
    if shared.filter.auto.load(Ordering::Relaxed) {
        -50.0
    } else {
        f32::from_bits(shared.filter.threshold.load(Ordering::Relaxed))
    }
}

/// Reads the microphone every 20 ms for the meter and who is talking;
/// `changed` hears when the user starts or stops.
pub fn watch_activity(
    shared: Arc<Shared>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    mut changed: impl FnMut(bool) + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("voice-activity".into())
        .spawn(move || {
            let mut activity = Activity::default();
            let mut talking = false;
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(20));
                let volume = shared
                    .analyser
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .volume();
                let level = if volume.is_finite() { volume.max(-100.0) } else { -100.0 };
                shared.level.store(level.to_bits(), Ordering::Relaxed);
                let speaking = activity.update(volume, threshold(&shared))
                    && !shared.muted.load(Ordering::Relaxed);
                shared.talking.store(speaking, Ordering::Relaxed);
                if speaking != talking {
                    talking = speaking;
                    changed(speaking);
                }
            }
        })
        .expect("voice activity thread")
}

/// A remote person's level as the web client's level-processor worklet
/// reads it: the mean of the last twenty 128-sample blocks over -80 dBFS.
pub struct Level {
    blocks: [f32; 20],
    head: usize,
    pending: Vec<f32>,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            blocks: [-100.0; 20],
            head: 0,
            pending: Vec::with_capacity(256),
        }
    }
}

impl Level {
    /// Feeds 48 kHz mono; true while the person is heard talking.
    pub fn feed(&mut self, samples: &[i16]) -> bool {
        for sample in samples {
            self.pending.push(f32::from(*sample) / 32_768.0);
            if self.pending.len() == 128 {
                let sum: f32 = self.pending.iter().map(|s| s * s).sum();
                let db = 20.0 * (sum / 128.0).sqrt().log10() + std::f32::consts::SQRT_2;
                self.head = (self.head + 1) % self.blocks.len();
                self.blocks[self.head] = if db > -100.0 { db } else { -100.0 };
                self.pending.clear();
            }
        }
        self.blocks.iter().sum::<f32>() / self.blocks.len() as f32 > -80.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_holds_after_the_last_loud_reading() {
        let mut activity = Activity::default();
        assert!(!activity.update(-90.0, -60.0));
        assert!(activity.update(-20.0, -60.0));
        // Ten readings until the loud one leaves the history, then nine
        // more of silence.
        for _ in 0..18 {
            assert!(activity.update(-90.0, -60.0));
        }
        assert!(!activity.update(-90.0, -60.0));
    }

    #[test]
    fn a_tone_is_loud_and_silence_is_not() {
        let mut analyser = Analyser::default();
        let silence = vec![0i16; 960];
        analyser.push(&silence, 1, 48_000);
        assert!(analyser.volume() < -100.0);
        let tone: Vec<i16> = (0..960)
            .map(|n| ((n as f32 * 0.1).sin() * 16_000.0) as i16)
            .collect();
        analyser.push(&tone, 1, 48_000);
        let volume = analyser.volume();
        assert!(volume > -30.0, "{volume}");
    }

    #[test]
    fn remote_level_follows_the_worklet() {
        let mut level = Level::default();
        assert!(!level.feed(&[0; 480]));
        let loud: Vec<i16> = (0..4_800).map(|n| if n % 2 == 0 { 8_000 } else { -8_000 }).collect();
        assert!(level.feed(&loud));
    }
}
