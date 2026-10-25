//! The voice driver: songbird, given the voice server our own gateway was
//! told about, sends the microphone and hands back what everyone says,
//! mixed for the speakers, and who is talking.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use songbird::driver::{Channels, DecodeConfig, DecodeMode, SampleRate};
use songbird::id::{ChannelId, GuildId, UserId};
use songbird::input::RawAdapter;
use songbird::{Config, ConnectionInfo, CoreEvent, Driver, Event, EventContext, EventHandler};

use crate::call::{MicSource, RATE, Shared};

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
    let config = Config::default().decode_mode(DecodeMode::Decode(DecodeConfig::new(
        Channels::Stereo,
        SampleRate::Hz48000,
    )));
    let mut driver = Driver::new(config);
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
            heard: Mutex::default(),
            wake: Box::new(wake),
        },
    );
    driver.add_global_event(
        CoreEvent::DriverDisconnect.into(),
        Gone {
            report: Box::new(gone),
        },
    );
    driver.play_input(RawAdapter::new(MicSource::new(shared), RATE, 2).into());
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
    /// When each person was last heard, to keep their ring lit between
    /// words.
    heard: Mutex<HashMap<u64, Instant>>,
    wake: Box<dyn Fn() + Send + Sync>,
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
        let mut heard = self.heard.lock().unwrap_or_else(|e| e.into_inner());
        for (ssrc, data) in &tick.speaking {
            let Some(pcm) = &data.decoded_voice else {
                continue;
            };
            let energy =
                pcm.iter().map(|s| f32::from(*s).powi(2)).sum::<f32>() / pcm.len().max(1) as f32;
            // About -45 dBFS: a voice, not a breath or comfort noise.
            if energy.sqrt() > 180.0
                && let Some(user) = ssrcs.get(ssrc)
            {
                heard.insert(*user, now);
            }
            for (out, sample) in mixed.iter_mut().zip(pcm) {
                *out += f32::from(*sample) / 32_768.0;
            }
        }
        for sample in &mut mixed {
            *sample = sample.clamp(-1.0, 1.0);
        }
        self.shared.speakers.push(&mixed);
        if !self.shared.deaf.load(std::sync::atomic::Ordering::Relaxed) {
            let mono: Vec<f32> = mixed
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[l, r]| (l + r) * 0.5)
                .collect();
            self.shared.played.push(&mono);
        }
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
