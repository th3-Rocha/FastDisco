//! Voice messages from the desktop: recording the microphone, then the
//! file Discord's phone app sends, mono 48 kHz Opus in OGG, with the
//! waveform its player draws.
//!
//! The encoder and the resampling follow ZapFast's voice notes
//! (crmne/zapfast, MIT).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

/// Opus' own rate; every voice message is mono at it.
pub const RATE: u32 = 48_000;
/// One Opus frame: 20 ms.
const FRAME: usize = 960;
/// Speech at about what the phone app sends.
const BITRATE: i32 = 32_000;
/// Discord draws at most this many bars.
const MAX_BARS: usize = 256;
/// Shorter than this is a slip of the finger, and is not sent.
pub const SHORTEST: Duration = Duration::from_millis(500);

/// What the audio thread hands over: the samples, and the loudness of
/// each tenth of a second for the bars that move while recording.
#[derive(Default)]
struct Shared {
    samples: Vec<f32>,
    levels: Vec<f32>,
    sum: f32,
    count: usize,
}

/// The microphone, recording until dropped or finished.
pub struct Recorder {
    _stream: cpal::Stream,
    shared: Arc<Mutex<Shared>>,
    channels: u16,
    rate: u32,
    started: Instant,
}

/// A finished recording, ready to send.
pub struct Take {
    /// Mono at [`RATE`].
    pub samples: Vec<f32>,
    pub seconds: f32,
    pub waveform: Vec<u8>,
}

impl Recorder {
    /// Opens the default microphone at its own format and starts recording.
    pub fn start() -> Result<Self, String> {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or("Nenhum microfone encontrado.")?;
        let config = device
            .default_input_config()
            .map_err(|e| format!("Não foi possível abrir o microfone: {e}"))?;
        let channels = config.channels();
        let rate = config.sample_rate().0;
        let shared = Arc::new(Mutex::new(Shared::default()));
        // A tenth of a second of interleaved samples per bar.
        let window = (rate as usize * usize::from(channels) / 10).max(1);
        let format = config.sample_format();
        let config: cpal::StreamConfig = config.into();
        let stream = match format {
            cpal::SampleFormat::F32 => open::<f32>(&device, &config, &shared, window),
            cpal::SampleFormat::I16 => open::<i16>(&device, &config, &shared, window),
            cpal::SampleFormat::U16 => open::<u16>(&device, &config, &shared, window),
            cpal::SampleFormat::I32 => open::<i32>(&device, &config, &shared, window),
            other => return Err(format!("Formato de microfone não suportado: {other}")),
        }
        .map_err(|e| format!("Não foi possível abrir o microfone: {e}"))?;
        stream
            .play()
            .map_err(|e| format!("Não foi possível gravar: {e}"))?;
        log::info!("recording a voice message at {rate} Hz, {channels} channels");
        Ok(Self {
            _stream: stream,
            shared,
            channels,
            rate,
            started: Instant::now(),
        })
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// The loudness of the last `count` tenths of a second, 0 to 1, oldest
    /// first; fewer at the start.
    pub fn levels(&self, count: usize) -> Vec<f32> {
        let shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let from = shared.levels.len().saturating_sub(count);
        shared.levels[from..].to_vec()
    }

    /// Stops the microphone and makes the recording mono at [`RATE`].
    pub fn finish(self) -> Take {
        let interleaved = {
            let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut shared.samples)
        };
        drop(self._stream);
        let mut samples = mono_at_rate(&interleaved, self.channels, self.rate);
        normalize(&mut samples);
        let seconds = samples.len() as f32 / RATE as f32;
        let waveform = waveform(&samples);
        Take {
            samples,
            seconds,
            waveform,
        }
    }
}

fn open<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    shared: &Arc<Mutex<Shared>>,
    window: usize,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let shared = Arc::clone(shared);
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let Ok(mut shared) = shared.lock() else {
                return;
            };
            let shared = &mut *shared;
            for &sample in data {
                let value: f32 = FromSample::from_sample_(sample);
                shared.samples.push(value);
                shared.sum += value * value;
                shared.count += 1;
                if shared.count >= window {
                    let level = (shared.sum / shared.count as f32).sqrt();
                    // Speech sits far below full scale; lift it for the bars.
                    shared.levels.push((level * 4.0).min(1.0));
                    shared.sum = 0.0;
                    shared.count = 0;
                }
            }
        },
        |error| log::warn!("microphone: {error}"),
        None,
    )
}

/// An OGG/Opus file from mono samples at [`RATE`].
pub fn encode(samples: &[f32]) -> Result<Vec<u8>, String> {
    use ogg::PacketWriteEndInfo::{EndPage, EndStream, NormalPacket};
    let mut encoder = opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip)
        .map_err(|e| e.to_string())?;
    encoder
        .set_bitrate(opus::Bitrate::Bits(BITRATE))
        .map_err(|e| e.to_string())?;
    let pre_skip = encoder.get_lookahead().map_err(|e| e.to_string())?.max(0) as usize;
    let serial = 0x4644_4953;
    let io = |e: std::io::Error| e.to_string();
    let mut writer = ogg::PacketWriter::new(Vec::new());
    writer
        .write_packet(opus_head(pre_skip as u16), serial, EndPage, 0)
        .map_err(io)?;
    writer
        .write_packet(opus_tags(), serial, EndPage, 0)
        .map_err(io)?;
    // Silence after the end flushes the encoder's lookahead; the last
    // granule position trims it off again.
    let frames = (samples.len() + pre_skip).div_ceil(FRAME).max(1);
    let mut frame = vec![0f32; FRAME];
    let mut packet = vec![0u8; 4000];
    for index in 0..frames {
        frame.fill(0.0);
        let start = index * FRAME;
        if start < samples.len() {
            let end = (start + FRAME).min(samples.len());
            frame[..end - start].copy_from_slice(&samples[start..end]);
        }
        let written = encoder
            .encode_float(&frame, &mut packet)
            .map_err(|e| e.to_string())?;
        let last = index + 1 == frames;
        let granule = if last {
            (pre_skip + samples.len()) as u64
        } else {
            ((index + 1) * FRAME) as u64
        };
        let end = if last {
            EndStream
        } else if (index + 1) % 50 == 0 {
            EndPage
        } else {
            NormalPacket
        };
        writer
            .write_packet(packet[..written].to_vec(), serial, end, granule)
            .map_err(io)?;
    }
    Ok(writer.into_inner())
}

fn opus_head(pre_skip: u16) -> Vec<u8> {
    let mut head = Vec::with_capacity(19);
    head.extend_from_slice(b"OpusHead");
    head.push(1);
    head.push(1);
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&RATE.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    head
}

fn opus_tags() -> Vec<u8> {
    let vendor = b"FastDisco";
    let mut tags = Vec::with_capacity(20 + vendor.len());
    tags.extend_from_slice(b"OpusTags");
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes());
    tags
}

/// Brings a quiet recording up to just below full scale, at most tenfold.
fn normalize(samples: &mut [f32]) {
    let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
    if peak <= 0.0 {
        return;
    }
    let gain = (0.89 / peak).clamp(1.0, 10.0);
    if gain > 1.0 {
        for sample in samples {
            *sample *= gain;
        }
    }
}

/// Discord's waveform: a bar per tenth of a second, from 32 up to 256
/// bars, each 0 to 255 relative to the loudest.
pub fn waveform(samples: &[f32]) -> Vec<u8> {
    if samples.is_empty() {
        return Vec::new();
    }
    let tenths = samples.len() / (RATE as usize / 10);
    let bars = tenths.clamp(32.min(samples.len()), MAX_BARS);
    let slice = samples.len().div_ceil(bars);
    let loudness: Vec<f32> = samples
        .chunks(slice)
        .map(|chunk| (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt())
        .collect();
    let loudest = loudness.iter().copied().fold(0.0f32, f32::max);
    loudness
        .iter()
        .map(|value| {
            if loudest > 0.0 {
                (value / loudest * 255.0).round() as u8
            } else {
                0
            }
        })
        .collect()
}

/// Mixes interleaved channels to mono and resamples to [`RATE`].
fn mono_at_rate(interleaved: &[f32], channels: u16, rate: u32) -> Vec<f32> {
    let channels = usize::from(channels.max(1));
    let mono: Vec<f32> = interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    if rate == RATE || rate == 0 || mono.is_empty() {
        return mono;
    }
    let ratio = f64::from(rate) / f64::from(RATE);
    let count = (mono.len() as f64 / ratio).floor() as usize;
    (0..count)
        .map(|index| {
            let position = index as f64 * ratio;
            let left = position.floor() as usize;
            let t = (position - left as f64) as f32;
            let a = mono[left.min(mono.len() - 1)];
            let b = mono.get(left + 1).copied().unwrap_or(a);
            a + (b - a) * t
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f32) -> Vec<f32> {
        (0..(RATE as f32 * seconds) as usize)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / RATE as f32).sin() * 0.5)
            .collect()
    }

    #[test]
    fn a_recording_becomes_an_ogg_opus_file() {
        let bytes = encode(&tone(1.0)).expect("encodes");
        assert!(bytes.starts_with(b"OggS"));
        assert!(bytes.windows(8).any(|w| w == b"OpusHead"));
        // 32 kbit/s for a second, give or take the headers.
        assert!(
            bytes.len() > 2_000 && bytes.len() < 8_000,
            "{}",
            bytes.len()
        );
    }

    #[test]
    fn the_waveform_has_a_bar_per_tenth_of_a_second() {
        assert_eq!(waveform(&tone(5.0)).len(), 50);
        assert_eq!(waveform(&tone(1.0)).len(), 32);
        assert_eq!(waveform(&tone(60.0)).len(), MAX_BARS);
        let mut rising = tone(4.0);
        let count = rising.len() as f32;
        for (i, s) in rising.iter_mut().enumerate() {
            *s *= i as f32 / count;
        }
        let bars = waveform(&rising);
        assert_eq!(*bars.last().unwrap(), 255);
        assert!(bars[0] < 20, "{}", bars[0]);
        assert!(waveform(&[]).is_empty());
    }

    /// Records a second from the default microphone:
    /// `cargo test voice::tests::the_microphone_records -- --ignored`.
    #[test]
    #[ignore = "needs a microphone"]
    fn the_microphone_records() {
        let recorder = Recorder::start().expect("opens");
        std::thread::sleep(Duration::from_secs(1));
        assert!(!recorder.levels(100).is_empty());
        let take = recorder.finish();
        assert!((take.seconds - 1.0).abs() < 0.3, "{} s", take.seconds);
        assert!(encode(&take.samples).expect("encodes").starts_with(b"OggS"));
    }

    #[test]
    fn any_input_becomes_mono_at_48k() {
        let stereo: Vec<f32> = (0..44_100 * 2)
            .map(|i| if i % 2 == 0 { 1.0 } else { 0.0 })
            .collect();
        let mono = mono_at_rate(&stereo, 2, 44_100);
        assert!((mono.len() as i64 - 48_000).abs() <= 2, "{}", mono.len());
        assert!(mono.iter().all(|v| (v - 0.5).abs() < 1e-6));
    }
}
