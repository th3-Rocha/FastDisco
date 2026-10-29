//! The voice driver: songbird, given the voice server our own gateway was
//! told about, sends the microphone and hands back what everyone says,
//! mixed for the speakers, and who is talking.
//!
//! songbird only decrypts what it receives; the Opus is decoded here.
//! songbird 0.6.1 hands its own decoder a DAVE frame with the frame's
//! trailer (tag, nonce, size and magic marker) still on its end. Opus in
//! voice mode, as Discord's apps send, mostly shrugs that off; the music
//! mode another songbird client sends reads bits from the end of the
//! frame, and comes out low and robotic. Cut here, the trailer never
//! reaches the decoder.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use songbird::driver::{Bitrate, DecodeMode, MixMode};
use songbird::id::{ChannelId, GuildId, UserId};
use songbird::input::RawAdapter;
use songbird::packet::Packet as _;
use songbird::{Config, ConnectionInfo, CoreEvent, Driver, Event, EventContext, EventHandler};

use crate::call::{MicSource, RATE, Shared, Tap};

/// What the microphone is sent at: Opus at 64 kbps, mono, as a voice
/// needs. songbird's default is 128 kbps stereo, made for music.
const BITRATE: i32 = 64_000;
/// How many 20 ms packets songbird holds per person to put late and
/// out-of-order ones back in place before decoding: 100 ms.
const PLAYOUT: u8 = 5;
/// Ticks between two lines of call statistics in the log: ten seconds.
const REPORT_TICKS: u32 = 500;

/// Where to connect, from VOICE_STATE_UPDATE and VOICE_SERVER_UPDATE.
#[derive(Clone, Debug)]
pub struct Server {
    /// The server, or the direct message channel for a call there.
    pub place: u64,
    pub channel: u64,
    pub user: u64,
    pub session_id: String,
    pub endpoint: String,
    pub token: String,
}

fn id(value: u64) -> Result<NonZeroU64, String> {
    NonZeroU64::new(value).ok_or_else(|| "id vazio".to_owned())
}

/// Connects and starts sending the microphone. `wake` redraws the window
/// when who is talking changes; `gone` tells that the call dropped.
pub async fn connect(
    server: Server,
    shared: Arc<Shared>,
    wake: impl Fn() + Send + Sync + 'static,
    gone: impl Fn(String) + Send + Sync + 'static,
) -> Result<Driver, String> {
    let config = Config::default()
        .decode_mode(DecodeMode::Decrypt)
        .mix_mode(MixMode::Mono)
        .playout_buffer_length(std::num::NonZeroU8::new(PLAYOUT).unwrap_or(std::num::NonZeroU8::MIN));
    let mut driver = Driver::new(config);
    driver.set_bitrate(Bitrate::Bits(BITRATE));
    log::info!(
        "call: Opus {} kbps mono, playout buffer {} packets ({} ms)",
        BITRATE / 1000,
        PLAYOUT,
        u32::from(PLAYOUT) * 20,
    );
    let ssrcs: Arc<Mutex<HashMap<u32, u64>>> = Arc::default();
    driver.add_global_event(
        CoreEvent::SpeakingStateUpdate.into(),
        Speaking {
            ssrcs: Arc::clone(&ssrcs),
        },
    );
    driver.add_global_event(
        CoreEvent::VoiceTick.into(),
        Tick {
            shared: Arc::clone(&shared),
            ssrcs,
            decoders: Mutex::default(),
            heard: Mutex::default(),
            wake: Box::new(wake),
            stats: Mutex::default(),
            tap: Tap::open("3-recebido", RATE, 2),
        },
    );
    driver.add_global_event(
        CoreEvent::DriverDisconnect.into(),
        Gone {
            report: Box::new(gone),
        },
    );
    driver.play_input(RawAdapter::new(MicSource::new(shared), RATE, 1).into());
    let info = ConnectionInfo {
        channel_id: ChannelId(id(server.channel)?),
        endpoint: server.endpoint,
        guild_id: GuildId(id(server.place)?),
        session_id: server.session_id,
        token: server.token,
        user_id: UserId(id(server.user)?),
    };
    driver.connect(info).await.map_err(|e| e.to_string())?;
    Ok(driver)
}

/// Learns whose voice each RTP stream carries.
struct Speaking {
    ssrcs: Arc<Mutex<HashMap<u32, u64>>>,
}

#[async_trait::async_trait]
impl EventHandler for Speaking {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::SpeakingStateUpdate(speaking) = ctx
            && let Some(user) = speaking.user_id
        {
            self.ssrcs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(speaking.ssrc, user.0);
        }
        None
    }
}

/// Every twenty milliseconds: mixes what was said for the speakers and
/// the echo canceller, and notes who said it.
struct Tick {
    shared: Arc<Shared>,
    ssrcs: Arc<Mutex<HashMap<u32, u64>>>,
    /// An Opus decoder for each stream, 48 kHz stereo.
    decoders: Mutex<HashMap<u32, opus2::Decoder>>,
    /// When each person was last heard, to keep their ring lit between
    /// words.
    heard: Mutex<HashMap<u64, Instant>>,
    wake: Box<dyn Fn() + Send + Sync>,
    stats: Mutex<Stats>,
    /// What was received after decoding, for `DISCOFAST_AUDIO_DEBUG`.
    tap: Option<Tap>,
}

/// Counted between two reports in the log.
#[derive(Default)]
struct Stats {
    ticks: u32,
    packets: u32,
    /// Missing when their turn came: lost or later than the playout
    /// buffer; the decoder made up the sound.
    lost: u32,
    errors: u32,
    /// The most people heard in one tick.
    streams: usize,
}

/// How long a voice stays lit after it was last heard.
const HOLD: Duration = Duration::from_millis(350);

#[async_trait::async_trait]
impl EventHandler for Tick {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        let EventContext::VoiceTick(tick) = ctx else {
            return None;
        };
        let now = Instant::now();
        let mut mixed = vec![0f32; RATE as usize * 2 / 50];
        let ssrcs = self.ssrcs.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let volumes = self
            .shared
            .volumes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut heard = self.heard.lock().unwrap_or_else(|e| e.into_inner());
        let mut decoders = self.decoders.lock().unwrap_or_else(|e| e.into_inner());
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        stats.ticks += 1;
        stats.streams = stats.streams.max(tick.speaking.len());
        decoders.retain(|ssrc, _| tick.speaking.contains_key(ssrc) || tick.silent.contains(ssrc));
        for (ssrc, data) in &tick.speaking {
            let decoder = match decoders.entry(*ssrc) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    match opus2::Decoder::new(RATE, opus2::Channels::Stereo) {
                        Ok(decoder) => entry.insert(decoder),
                        Err(error) => {
                            log::warn!("voice decoder: {error}");
                            continue;
                        }
                    }
                }
            };
            if data.packet.is_some() {
                stats.packets += 1;
            } else {
                stats.lost += 1;
            }
            let Some(pcm) = decode(decoder, data.packet.as_ref()) else {
                stats.errors += 1;
                continue;
            };
            let energy = pcm.iter().map(|s| s * s).sum::<f32>() / pcm.len().max(1) as f32;
            // About -45 dBFS: a voice, not a breath or comfort noise.
            if energy.sqrt() > 0.0055
                && let Some(user) = ssrcs.get(ssrc)
            {
                heard.insert(*user, now);
            }
            let gain = ssrcs
                .get(ssrc)
                .and_then(|user| volumes.get(user))
                .copied()
                .unwrap_or(1.0);
            if gain == 0.0 {
                continue;
            }
            for (out, sample) in mixed.iter_mut().zip(&pcm) {
                *out += sample * gain;
            }
        }
        drop(decoders);
        for sample in &mut mixed {
            *sample = sample.clamp(-1.0, 1.0);
        }
        // The echo canceller's copy is taken where the speakers play it,
        // in time with what the microphone hears.
        self.shared.speakers.push(&mixed);
        if let Some(tap) = &self.tap {
            tap.push(&mixed);
        }
        if stats.ticks >= REPORT_TICKS {
            log::info!(
                "call: received {} packets, {} lost or late ({:.1}%), {} undecodable, up to {} voices; {}",
                stats.packets,
                stats.lost,
                f64::from(stats.lost) * 100.0 / f64::from((stats.packets + stats.lost).max(1)),
                stats.errors,
                stats.streams,
                self.shared.report(),
            );
            *stats = Stats::default();
        }
        drop(stats);
        heard.retain(|_, at| now.duration_since(*at) < HOLD);
        let talking: HashSet<u64> = heard.keys().copied().collect();
        drop(heard);
        let mut speaking = self
            .shared
            .speaking
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *speaking != talking {
            *speaking = talking;
            drop(speaking);
            (self.wake)();
        }
        None
    }
}

/// One tick of a stream, 48 kHz stereo: the packet's Opus, or what the
/// decoder makes up for a packet that was lost.
fn decode(
    decoder: &mut opus2::Decoder,
    packet: Option<&songbird::events::context_data::RtpData>,
) -> Option<Vec<f32>> {
    // Room for the longest Opus frame, 120 ms; a lost packet is made up
    // for exactly one 20 ms tick, as libopus conceals as much as it is
    // given room for.
    let length = if packet.is_some() { 120 } else { 20 };
    let mut pcm = vec![0f32; RATE as usize * 2 * length / 1000];
    let samples = match packet {
        Some(data) => {
            let rtp = data.rtp();
            let payload = rtp.payload();
            // Despite its name, songbird's payload_end_pad is where the body
            // ends, not how long the padding after it is.
            let body = payload.get(data.payload_offset..data.payload_end_pad)?;
            // A header extension sits before the Opus: its 4-byte header,
            // then as many 4-byte words as it says.
            let start = if rtp.get_extension() != 0 {
                4 + 4 * usize::from(u16::from_be_bytes([*body.get(2)?, *body.get(3)?]))
            } else {
                0
            };
            decoder.decode_float(without_dave_trailer(body.get(start..)?), &mut pcm, false)
        }
        None => decoder.decode_float(&[], &mut pcm, false),
    };
    match samples {
        Ok(samples) => {
            pcm.truncate(samples * 2);
            Some(pcm)
        }
        Err(error) => {
            log::debug!("voice decode: {error}");
            None
        }
    }
}

/// The DAVE magic marker that ends an end-to-end encrypted frame.
const DAVE_MARKER: [u8; 2] = [0xfa, 0xfa];

/// A decrypted DAVE frame still ends with its trailer: everything from
/// the tag on, whose length is the byte before the marker. A frame
/// without the marker is passed through.
fn without_dave_trailer(frame: &[u8]) -> &[u8] {
    let n = frame.len();
    if n < 11 || frame[n - 2..] != DAVE_MARKER {
        return frame;
    }
    let trailer = usize::from(frame[n - 3]);
    if trailer < 11 || trailer > n {
        return frame;
    }
    &frame[..n - trailer]
}

/// The connection ended without being asked to.
struct Gone {
    report: Box<dyn Fn(String) + Send + Sync>,
}

#[async_trait::async_trait]
impl EventHandler for Gone {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::DriverDisconnect(data) = ctx
            && let Some(reason) = &data.reason
        {
            (self.report)(format!("{reason:?}"));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lost_packet_is_made_up_for_one_tick_only() {
        let mut encoder =
            opus2::Encoder::new(RATE, opus2::Channels::Mono, opus2::Application::Voip).unwrap();
        let mut decoder = opus2::Decoder::new(RATE, opus2::Channels::Stereo).unwrap();
        let tone: Vec<f32> = (0..960).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let mut packet = vec![0u8; 4000];
        let n = encoder.encode_float(&tone, &mut packet).unwrap();
        let mut pcm = vec![0f32; 5760 * 2];
        let decoded = decoder.decode_float(&packet[..n], &mut pcm, false).unwrap();
        assert_eq!(decoded, 960);
        // Before, the concealment filled the whole 120 ms buffer and the
        // decoder's state ran 100 ms ahead of the call.
        let concealed = decode(&mut decoder, None).unwrap();
        assert_eq!(concealed.len(), 960 * 2);
    }

    #[test]
    fn the_dave_trailer_is_cut_and_plain_opus_kept() {
        let opus = [0xfc, 1, 2, 3, 4];
        // Tag (8), a 1-byte nonce, no ranges, the size byte and the marker.
        let mut frame = opus.to_vec();
        frame.extend([9; 8]);
        frame.push(7);
        frame.push(12);
        frame.extend(DAVE_MARKER);
        assert_eq!(without_dave_trailer(&frame), opus);
        assert_eq!(without_dave_trailer(&opus), opus);
        let mut odd = vec![0; 20];
        odd.extend([3, 0xfa, 0xfa]);
        assert_eq!(without_dave_trailer(&odd).len(), odd.len());
    }
}
