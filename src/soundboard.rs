//! Discord's soundboard. The sounds come from Discord (its own and every
//! server's), and playing one mixes it straight into the voice the call
//! sends, after the noise filter, so everyone hears it, whatever server
//! it is from and with no Nitro. The user hears it too, through the
//! call's own output, where the echo canceller knows it. Sounds that
//! others play the way Discord's apps do (VOICE_CHANNEL_EFFECT_SEND) are
//! played here as those apps play them.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use reqwest::Method;
use serde_json::Value;

use crate::app::App;
use crate::backend::Command;
use crate::discord::parse;
use crate::model::Id;

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    /// Discord's own sounds.
    Defaults,
    /// A server's sounds.
    Guild(Id),
}

/// One sound of the board.
#[derive(Clone, Debug)]
pub struct Sound {
    pub id: Id,
    pub name: String,
    /// 0 to 1, as the sound's owner set it.
    pub volume: f32,
    pub emoji: Option<String>,
    pub emoji_id: Option<Id>,
}

impl Sound {
    fn parse(value: &Value) -> Option<Self> {
        Some(Self {
            id: parse::id(&value["sound_id"])?,
            name: value["name"].as_str().unwrap_or_default().to_owned(),
            volume: value["volume"].as_f64().unwrap_or(1.0) as f32,
            emoji: value["emoji_name"].as_str().filter(|e| !e.is_empty()).map(str::to_owned),
            emoji_id: parse::id(&value["emoji_id"]),
        })
    }
}

/// What to do with a sound once it has been downloaded and decoded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Use {
    /// Into the call, for everyone.
    Send,
    /// For the user alone.
    Preview,
    /// Someone else played it the way Discord's apps do.
    Heard(f32),
}

#[derive(Default)]
pub struct Soundboard {
    pub open: bool,
    pub search: String,
    /// Discord's sounds, then each server's, in the rail's order.
    pub defaults: Option<Vec<Sound>>,
    pub guilds: HashMap<Id, Vec<Sound>>,
    asked: HashSet<Option<Id>>,
    /// Decoded sounds, 48 kHz mono, by id.
    decoded: HashMap<Id, Arc<Vec<f32>>>,
    /// Sounds being fetched, and what they are for.
    waiting: HashMap<Id, Vec<(Use, f32)>>,
    /// The sound last sent and when, for the button's light.
    pub playing: Option<(Id, std::time::Instant)>,
}

#[derive(Clone, Debug)]
pub enum SoundboardAction {
    Toggle,
    Close,
    Play(Id, f32),
    Preview(Id, f32),
}

impl App {
    fn soundboard_api(&self, tag: Tag, path: String) {
        self.backend().send(Command::Api {
            tag: crate::backend::Tag::Soundboard(tag),
            method: Method::GET,
            path,
            body: None,
        });
    }

    pub fn apply_soundboard(&mut self, action: SoundboardAction) {
        match action {
            SoundboardAction::Toggle => {
                self.soundboard.open = !self.soundboard.open;
                if self.soundboard.open {
                    self.load_sounds();
                }
            }
            SoundboardAction::Close => self.soundboard.open = false,
            SoundboardAction::Play(id, volume) => {
                self.soundboard.playing = Some((id, std::time::Instant::now()));
                self.use_sound(id, Use::Send, volume);
            }
            SoundboardAction::Preview(id, volume) => self.use_sound(id, Use::Preview, volume),
        }
    }

    /// Discord's sounds and every server's, asked for once.
    fn load_sounds(&mut self) {
        if self.soundboard.asked.insert(None) {
            self.soundboard_api(Tag::Defaults, "/soundboard-default-sounds".into());
        }
        let guilds: Vec<Id> = self.model.guilds.iter().map(|g| g.id).collect();
        for guild in guilds {
            if self.soundboard.asked.insert(Some(guild)) {
                self.soundboard_api(Tag::Guild(guild), format!("/guilds/{guild}/soundboard-sounds"));
            }
        }
    }

    fn use_sound(&mut self, id: Id, how: Use, volume: f32) {
        match self.soundboard.decoded.get(&id).cloned() {
            Some(samples) => self.play_sound(&samples, how, volume),
            None => {
                let waiting = self.soundboard.waiting.entry(id).or_default();
                let first = waiting.is_empty();
                waiting.push((how, volume));
                if first {
                    self.backend().send(Command::FetchSound(id));
                }
            }
        }
    }

    /// A sound into the call (and the user's ears), or for the user alone.
    fn play_sound(&mut self, samples: &[f32], how: Use, volume: f32) {
        // Discord's sounds are loud; its board plays them a little down.
        let gain = volume.clamp(0.0, 1.0) * 0.7;
        let scaled: Vec<f32> = samples.iter().map(|s| (s * gain).clamp(-1.0, 1.0)).collect();
        match (&self.call, how) {
            (Some(call), Use::Send) => {
                call.shared.effect_out.lock().unwrap_or_else(|e| e.into_inner()).extend(scaled.iter().copied());
                call.shared.effect_in.lock().unwrap_or_else(|e| e.into_inner()).extend(scaled);
            }
            (Some(call), Use::Preview | Use::Heard(_)) => {
                let extra = if let Use::Heard(v) = how { v } else { 1.0 };
                call.shared
                    .effect_in
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .extend(scaled.iter().map(|s| s * extra));
            }
            (None, Use::Preview) => self.play_samples(scaled),
            (None, _) => {}
        }
    }

    /// A sound downloaded and decoded in the background.
    pub fn sound_ready(&mut self, id: Id, samples: Option<Vec<f32>>) {
        let waiting = self.soundboard.waiting.remove(&id).unwrap_or_default();
        let Some(samples) = samples else {
            log::warn!("soundboard: sound {id} could not be read");
            return;
        };
        let samples = Arc::new(samples);
        self.soundboard.decoded.insert(id, Arc::clone(&samples));
        for (how, volume) in waiting {
            self.play_sound(&samples, how, volume);
        }
    }

    /// VOICE_CHANNEL_EFFECT_SEND: someone in the user's call played a
    /// sound from Discord's board; their app does not send it in their
    /// voice, every listener plays it.
    pub fn voice_effect(&mut self, data: &Value) {
        let (Some(channel), Some(sound), Some(user)) = (
            parse::id(&data["channel_id"]),
            parse::id(&data["sound_id"]),
            parse::id(&data["user_id"]),
        ) else {
            return;
        };
        if user == self.model.me
            || self.call.as_ref().is_none_or(|c| c.channel != channel)
            || self.settings.deafened
            || self.settings.local_mutes.contains(&user)
        {
            return;
        }
        let volume = data["sound_volume"].as_f64().unwrap_or(1.0) as f32;
        self.use_sound(sound, Use::Heard(1.0), volume);
    }

    pub fn soundboard_result(&mut self, tag: Tag, result: Result<Value, String>) {
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                log::debug!("soundboard {tag:?}: {error}");
                return;
            }
        };
        let list = value["items"].as_array().or(value.as_array()).cloned().unwrap_or_default();
        let sounds: Vec<Sound> = list
            .iter()
            .filter(|s| s["available"].as_bool() != Some(false))
            .filter_map(Sound::parse)
            .collect();
        match tag {
            Tag::Defaults => self.soundboard.defaults = Some(sounds),
            Tag::Guild(guild) => {
                if !sounds.is_empty() {
                    self.soundboard.guilds.insert(guild, sounds);
                }
            }
        }
    }
}

/// A sound file as 48 kHz mono: rodio's decoders first (MP3, Vorbis,
/// FLAC, WAV), then ffmpeg for the rest (Discord also stores Opus).
pub fn decode(bytes: Vec<u8>) -> Option<Vec<f32>> {
    use rodio::Source;
    if let Ok(decoder) = rodio::Decoder::new(std::io::Cursor::new(bytes.clone())) {
        let rate = decoder.sample_rate();
        let channels = usize::from(decoder.channels()).max(1);
        let raw: Vec<f32> = decoder.collect();
        if !raw.is_empty() {
            let mono: Vec<f32> = raw
                .chunks(channels)
                .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
                .collect();
            return Some(resample(&mono, rate));
        }
    }
    let mut child = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i", "pipe:0", "-f", "f32le", "-ac", "1", "-ar", "48000", "pipe:1"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let writer = std::thread::spawn(move || {
        use std::io::Write;
        let _ = stdin.write_all(&bytes);
    });
    let output = child.wait_with_output().ok()?;
    let _ = writer.join();
    let samples: Vec<f32> = output
        .stdout
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    (!samples.is_empty()).then_some(samples)
}

/// Linear resampling to 48 kHz: plenty for a short sound effect.
fn resample(input: &[f32], rate: u32) -> Vec<f32> {
    if rate == crate::call::RATE || rate == 0 || input.is_empty() {
        return input.to_vec();
    }
    let ratio = f64::from(rate) / f64::from(crate::call::RATE);
    let len = (input.len() as f64 / ratio) as usize;
    (0..len)
        .map(|i| {
            let x = i as f64 * ratio;
            let k = x as usize;
            let a = input[k.min(input.len() - 1)];
            let b = input[(k + 1).min(input.len() - 1)];
            a + (b - a) * (x - k as f64) as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_wav_decodes_to_48k_mono() {
        // One second of 24 kHz stereo silence with a click, as a WAV.
        let rate = 24_000u32;
        let mut wav = Vec::new();
        let data = rate * 2 * 2;
        wav.extend(b"RIFF");
        wav.extend((36 + data).to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(rate.to_le_bytes());
        wav.extend((rate * 4).to_le_bytes());
        wav.extend(4u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(data.to_le_bytes());
        for i in 0..rate {
            let v: i16 = if i == 100 { 16_000 } else { 0 };
            wav.extend(v.to_le_bytes());
            wav.extend(v.to_le_bytes());
        }
        let samples = super::decode(wav).expect("decodes");
        assert!((samples.len() as i64 - 48_000).abs() < 10, "{}", samples.len());
        assert!(samples.iter().any(|s| *s > 0.3));
    }
}
