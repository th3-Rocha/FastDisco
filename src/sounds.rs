//! The app's little sounds, as Discord has: joining and leaving a call,
//! someone coming and going, muting, deafening, a new message, a dropped
//! call. Made here from soft sine tones (Discord's own are its own), played
//! through rodio on the default output.

use std::time::Duration;

use rodio::Source;

const RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// The user joined a call.
    Join,
    /// The user left it.
    Leave,
    /// Someone else came into the user's channel.
    Arrive,
    /// Someone else left it.
    Depart,
    Mute,
    Unmute,
    Deafen,
    Undeafen,
    /// A direct message or a mention.
    Message,
    /// The call could not connect, or dropped.
    Lost,
}

/// Notes as (frequency in Hz, start in seconds, length in seconds).
fn notes(sound: Sound) -> (&'static [(f32, f32, f32)], f32) {
    // Loudness: the call's own events softer than a message.
    match sound {
        Sound::Join => (&[(523.25, 0.0, 0.16), (783.99, 0.09, 0.26)], 0.22),
        Sound::Leave => (&[(783.99, 0.0, 0.16), (523.25, 0.09, 0.26)], 0.22),
        Sound::Arrive => (&[(880.0, 0.0, 0.10), (1174.66, 0.06, 0.16)], 0.16),
        Sound::Depart => (&[(1174.66, 0.0, 0.10), (880.0, 0.06, 0.16)], 0.16),
        Sound::Mute => (&[(392.0, 0.0, 0.07), (293.66, 0.05, 0.10)], 0.18),
        Sound::Unmute => (&[(293.66, 0.0, 0.07), (392.0, 0.05, 0.10)], 0.18),
        Sound::Deafen => (
            &[
                (523.25, 0.0, 0.07),
                (392.0, 0.05, 0.07),
                (293.66, 0.10, 0.12),
            ],
            0.18,
        ),
        Sound::Undeafen => (
            &[
                (293.66, 0.0, 0.07),
                (392.0, 0.05, 0.07),
                (523.25, 0.10, 0.12),
            ],
            0.18,
        ),
        Sound::Message => (&[(1318.51, 0.0, 0.12), (1760.0, 0.07, 0.22)], 0.20),
        Sound::Lost => (&[(220.0, 0.0, 0.18), (164.81, 0.14, 0.30)], 0.22),
    }
}

/// The sound as mono samples: each note a sine with its octave, a quick
/// rise and a smooth fall, so nothing clicks.
pub fn render(sound: Sound) -> Vec<f32> {
    let (notes, gain) = notes(sound);
    let end = notes
        .iter()
        .map(|(_, start, len)| start + len)
        .fold(0f32, f32::max);
    let total = (end * RATE as f32) as usize + RATE as usize / 50;
    let mut out = vec![0f32; total];
    for &(frequency, start, length) in notes {
        let first = (start * RATE as f32) as usize;
        let count = (length * RATE as f32) as usize;
        for i in 0..count {
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.008).min(1.0);
            let release = (1.0 - i as f32 / count as f32).powf(2.0);
            let tau = std::f32::consts::TAU;
            let tone = (tau * frequency * t).sin() + 0.25 * (tau * 2.0 * frequency * t).sin();
            if let Some(slot) = out.get_mut(first + i) {
                *slot += tone * attack * release * gain;
            }
        }
    }
    for sample in &mut out {
        *sample = sample.clamp(-1.0, 1.0);
    }
    out
}

/// Plays the sounds; the output opens with the first one.
#[derive(Default)]
pub struct Sounds {
    stream: Option<rodio::OutputStream>,
}

impl Sounds {
    pub fn play(&mut self, sound: Sound) {
        if self.stream.is_none() {
            match rodio::OutputStreamBuilder::open_default_stream() {
                Ok(mut stream) => {
                    stream.log_on_drop(false);
                    self.stream = Some(stream);
                }
                Err(error) => {
                    log::warn!("sounds: {error}");
                    return;
                }
            }
        }
        if let Some(stream) = &self.stream {
            let samples = rodio::buffer::SamplesBuffer::new(1, RATE, render(sound));
            stream
                .mixer()
                .add(samples.fade_in(Duration::from_millis(2)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_short_soft_and_ends_in_silence() {
        for sound in [
            Sound::Join,
            Sound::Leave,
            Sound::Arrive,
            Sound::Depart,
            Sound::Mute,
            Sound::Unmute,
            Sound::Deafen,
            Sound::Undeafen,
            Sound::Message,
            Sound::Lost,
        ] {
            let samples = render(sound);
            let seconds = samples.len() as f32 / RATE as f32;
            assert!(seconds > 0.1 && seconds < 0.6, "{sound:?}: {seconds}");
            let peak = samples.iter().fold(0f32, |p, s| p.max(s.abs()));
            assert!(peak > 0.05 && peak < 0.5, "{sound:?}: {peak}");
            assert!(samples.last().unwrap().abs() < 0.01, "{sound:?}");
        }
    }
}
