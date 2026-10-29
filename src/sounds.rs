//! The app's little sounds, as Discord has: joining and leaving a call,
//! someone coming and going, muting, deafening, a new message, a dropped
//! call. Made here from soft, low mallet tones (Discord's own are its own), played
//! through rodio on the default output.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// Someone calls: repeated while it rings.
    Ring,
}

/// Notes as (frequency in Hz, start in seconds, length in seconds), and
/// the loudness. Low and in major or pentatonic steps, ringing into each
/// other, so they come out as a little melody rather than beeps.
fn notes(sound: Sound) -> (&'static [(f32, f32, f32)], f32) {
    const D3: f32 = 146.83;
    const E3: f32 = 164.81;
    const F3: f32 = 174.61;
    const G3: f32 = 196.0;
    const A3: f32 = 220.0;
    const C4: f32 = 261.63;
    const E4: f32 = 329.63;
    const G4: f32 = 392.0;
    const A4: f32 = 440.0;
    match sound {
        Sound::Join => (&[(G3, 0.0, 0.32), (C4, 0.07, 0.32), (E4, 0.14, 0.34)], 0.20),
        Sound::Leave => (&[(E4, 0.0, 0.32), (C4, 0.07, 0.32), (G3, 0.14, 0.34)], 0.20),
        Sound::Arrive => (&[(C4, 0.0, 0.26), (G4, 0.08, 0.30)], 0.15),
        Sound::Depart => (&[(G4, 0.0, 0.26), (C4, 0.08, 0.30)], 0.15),
        Sound::Mute => (&[(A3, 0.0, 0.18), (E3, 0.06, 0.24)], 0.20),
        Sound::Unmute => (&[(E3, 0.0, 0.18), (A3, 0.06, 0.24)], 0.20),
        Sound::Deafen => (&[(C4, 0.0, 0.20), (G3, 0.06, 0.20), (E3, 0.12, 0.26)], 0.18),
        Sound::Undeafen => (&[(E3, 0.0, 0.20), (G3, 0.06, 0.20), (C4, 0.12, 0.26)], 0.18),
        Sound::Message => (&[(E4, 0.0, 0.28), (A4, 0.09, 0.36)], 0.18),
        Sound::Lost => (&[(A3, 0.0, 0.26), (F3, 0.12, 0.26), (D3, 0.24, 0.32)], 0.20),
        Sound::Ring => (
            &[
                (C4, 0.0, 0.30),
                (E4, 0.12, 0.30),
                (G4, 0.24, 0.40),
                (C4, 0.60, 0.30),
                (E4, 0.72, 0.30),
                (G4, 0.84, 0.50),
            ],
            0.18,
        ),
    }
}

/// The sound as mono samples. Each note is a soft mallet: a sine whose
/// first moments carry a little FM shimmer that fades out, a rounded
/// attack and a long fall to silence, so nothing clicks or beeps.
pub fn render(sound: Sound) -> Vec<f32> {
    let (notes, gain) = notes(sound);
    let end = notes
        .iter()
        .map(|(_, start, len)| start + len)
        .fold(0f32, f32::max);
    let total = (end * RATE as f32) as usize + RATE as usize / 50;
    let mut out = vec![0f32; total];
    let tau = std::f32::consts::TAU;
    for &(frequency, start, length) in notes {
        let first = (start * RATE as f32) as usize;
        let count = (length * RATE as f32) as usize;
        for i in 0..count {
            let t = i as f32 / RATE as f32;
            let x = i as f32 / count as f32;
            let attack = (t / 0.012).min(1.0);
            let attack = (attack * std::f32::consts::FRAC_PI_2).sin();
            let decay = (-t / (length * 0.45)).exp();
            let release = (1.0 - x).powf(1.5);
            let shimmer = 0.9 * (-t / 0.05).exp() * (tau * 2.0 * frequency * t).sin();
            let tone = (tau * frequency * t + shimmer).sin()
                + 0.08 * (tau * 3.0 * frequency * t).sin() * (-t / 0.03).exp();
            if let Some(slot) = out.get_mut(first + i) {
                *slot += tone * attack * decay * release * gain;
            }
        }
    }
    for sample in &mut out {
        *sample = sample.clamp(-1.0, 1.0);
    }
    out
}

/// Plays the sounds; the output opens with the first one. A device that
/// goes away (leaving a call can take the output with it) is closed and
/// opened again with the next sound, as the dead one reports its error
/// over and over until dropped.
#[derive(Default)]
pub struct Sounds {
    stream: Option<rodio::OutputStream>,
    lost: Arc<AtomicBool>,
}

impl Sounds {
    pub fn play(&mut self, ctx: &egui::Context, sound: Sound) {
        self.close_if_lost();
        if self.stream.is_none() {
            let lost = Arc::clone(&self.lost);
            let ctx = ctx.clone();
            let opened = rodio::OutputStreamBuilder::from_default_device()
                .map(|b| {
                    b.with_error_callback(move |error| {
                        if !lost.swap(true, Ordering::Relaxed) {
                            log::warn!("sounds: {error}");
                            ctx.request_repaint();
                        }
                    })
                })
                .and_then(|b| b.open_stream_or_fallback());
            match opened {
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

    /// Closes the output once it has failed.
    pub fn close_if_lost(&mut self) {
        if self.lost.load(Ordering::Relaxed) {
            // Dropped first: its thread stops, so it cannot mark the next
            // one as lost.
            self.stream = None;
            self.lost.store(false, Ordering::Relaxed);
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
