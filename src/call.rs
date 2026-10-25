//! A voice call's sound on this machine: the microphone into a ring the
//! voice driver reads, and what the others say from a ring into the
//! speakers. Both at Discord's 48 kHz stereo. The streams stay on the
//! interface thread (they cannot move between threads on every platform);
//! the rings are shared with the driver in the backend.

use std::collections::{HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

use crate::model::Id;

/// Discord's voice rate.
pub const RATE: u32 = 48_000;
/// Interleaved stereo.
const CHANNELS: usize = 2;
/// The most a ring keeps, in samples: a fifth of a second. Older sound is
/// dropped so the delay never grows.
const RING: usize = RATE as usize * CHANNELS / 5;

/// Samples handed from one thread to another.
#[derive(Default)]
pub struct Ring {
    samples: Mutex<VecDeque<f32>>,
}

impl Ring {
    pub fn push(&self, data: &[f32]) {
        let mut ring = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        ring.extend(data.iter().copied());
        let over = ring.len().saturating_sub(RING);
        ring.drain(..over);
    }

    /// Fills `out`, with silence where there is nothing yet.
    pub fn pop(&self, out: &mut [f32]) {
        let mut ring = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        for slot in out.iter_mut() {
            *slot = ring.pop_front().unwrap_or(0.0);
        }
    }
}

/// What the interface and the voice driver share.
#[derive(Default)]
pub struct Shared {
    /// The microphone, 48 kHz stereo, for the driver to send.
    pub mic: Ring,
    /// Everyone else mixed, 48 kHz stereo, for the speakers.
    pub speakers: Ring,
    /// Deafened: what others say is not played.
    pub deaf: AtomicBool,
    /// The microphone's loudness, 0 to 1000, for the user's own ring.
    pub level: AtomicU32,
    /// Who is talking right now.
    pub speaking: Mutex<HashSet<Id>>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("call::Shared")
    }
}

/// The microphone as songbird reads it: raw little-endian f32 samples,
/// never ending, silence when the microphone has nothing new.
pub struct MicSource {
    pub shared: Arc<Shared>,
    pending: Vec<u8>,
}

impl MicSource {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            pending: Vec::new(),
        }
    }
}

impl Read for MicSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            // Twenty milliseconds at a time, as the driver sends.
            let mut samples = vec![0f32; RATE as usize * CHANNELS / 50];
            self.shared.mic.pop(&mut samples);
            self.pending = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
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
            let peak = converted.iter().fold(0f32, |p, s: &f32| p.max(s.abs()));
            shared
                .level
                .store((peak.min(1.0) * 1000.0) as u32, Ordering::Relaxed);
            out.clear();
            resampler.feed(&converted, &mut out);
            shared.mic.push(&out);
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
    let mut source = Vec::new();
    let mut position = 0f64;
    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels.max(1);
                // How many 48 kHz frames this callback covers.
                let step = f64::from(RATE) / f64::from(rate);
                let needed = ((frames as f64) * step).ceil() as usize + 1;
                source.resize(needed * CHANNELS, 0.0);
                shared.speakers.pop(&mut source);
                let deaf = shared.deaf.load(Ordering::Relaxed);
                for (i, frame) in data.chunks_mut(channels.max(1)).enumerate() {
                    let at = ((i as f64 * step + position) as usize).min(needed - 1);
                    let (l, r) = if deaf {
                        (0.0, 0.0)
                    } else {
                        (source[at * 2], source[at * 2 + 1])
                    };
                    for (c, sample) in frame.iter_mut().enumerate() {
                        *sample = if c % 2 == 0 { l } else { r };
                    }
                }
                position = (frames as f64 * step + position).fract();
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
    fn the_ring_drops_old_sound_and_pads_with_silence() {
        let ring = Ring::default();
        ring.push(&vec![1.0; RING + 100]);
        let mut out = vec![0.0; RING + 10];
        ring.pop(&mut out);
        assert!(out[..RING].iter().all(|s| *s == 1.0));
        assert!(out[RING..].iter().all(|s| *s == 0.0));
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
        });
        self.send_voice_state();
    }

    pub fn leave_voice(&mut self) {
        let Some(call) = self.call.take() else {
            return;
        };
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
        let Some(user) = crate::discord::parse::voice_state(data, None, &mut self.model) else {
            return;
        };
        if user != self.model.me {
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
            }
            Err(error) => {
                log::warn!("voice: {error}");
                call.state = State::Failed(error);
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
                && call.shared.level.load(Ordering::Relaxed) > 60;
        }
        call.shared
            .speaking
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&user)
    }
}
