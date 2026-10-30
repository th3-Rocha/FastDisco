//! A voice call's state on this machine, shared between the interface
//! and the voice engine (see `discord::voice`): mute and deafen, the
//! voice filter, who is talking, everyone's volume, and the soundboard's
//! sound on its way in and out.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::model::Id;

/// Discord's voice rate, and the soundboard's.
pub const RATE: u32 = 48_000;

/// The voice filter's settings, which can change during a call.
pub struct Filter {
    pub echo: AtomicBool,
    pub gain: AtomicBool,
    /// Automatic input sensitivity; otherwise `threshold`.
    pub auto: AtomicBool,
    /// The manual sensitivity in dB, as f32 bits.
    pub threshold: AtomicU32,
    /// Bumped when echo cancellation or automatic gain changes, for the
    /// microphone to be opened again with them.
    pub version: AtomicU32,
}

impl Filter {
    pub fn set(&self, settings: &crate::settings::Settings) {
        let filter = settings.voice_filter();
        let reopen = self.echo.swap(filter.echo, Ordering::Relaxed) != filter.echo
            | (self.gain.swap(filter.gain, Ordering::Relaxed) != filter.gain);
        self.auto.store(filter.auto, Ordering::Relaxed);
        self.threshold
            .store(filter.threshold.to_bits(), Ordering::Relaxed);
        if reopen {
            self.version.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// What the interface and the voice engine share.
pub struct Shared {
    /// Deafened: what others say is not played.
    pub deaf: AtomicBool,
    /// The microphone is off.
    pub muted: AtomicBool,
    pub filter: Filter,
    /// The user is talking, by Discord's voice activity.
    pub talking: AtomicBool,
    /// Who else is talking right now.
    pub speaking: Mutex<HashSet<Id>>,
    /// The gain for each person who is not at 100%: 0 for one silenced.
    pub volumes: Mutex<HashMap<Id, f32>>,
    /// Bumped when `volumes` changes.
    pub volumes_version: AtomicU64,
    /// The microphone's and the speakers' volume, as f32 bits.
    pub mic_gain: AtomicU32,
    pub speaker_gain: AtomicU32,
    /// The microphone's level in dB, as f32 bits, for the meters.
    pub level: AtomicU32,
    /// The microphone's last samples, for the voice activity.
    pub analyser: Mutex<crate::discord::voice::engine::Analyser>,
    /// The chosen devices, for routing the engine's streams to them.
    pub input_device: Mutex<Option<String>>,
    pub output_device: Mutex<Option<String>>,
    /// Soundboard sounds to send, 48 kHz mono, mixed into the microphone.
    pub effect_out: Mutex<VecDeque<f32>>,
    /// Soundboard sounds for the user to hear, mixed into what is played.
    pub effect_in: Mutex<VecDeque<f32>>,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            deaf: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            filter: Filter {
                echo: AtomicBool::new(true),
                gain: AtomicBool::new(true),
                auto: AtomicBool::new(true),
                threshold: AtomicU32::new((-60f32).to_bits()),
                version: AtomicU32::new(0),
            },
            talking: AtomicBool::new(false),
            speaking: Mutex::new(HashSet::new()),
            volumes: Mutex::new(HashMap::new()),
            volumes_version: AtomicU64::new(0),
            mic_gain: AtomicU32::new(1f32.to_bits()),
            speaker_gain: AtomicU32::new(1f32.to_bits()),
            level: AtomicU32::new((-100f32).to_bits()),
            analyser: Mutex::default(),
            input_device: Mutex::new(None),
            output_device: Mutex::new(None),
            effect_out: Mutex::new(VecDeque::new()),
            effect_in: Mutex::new(VecDeque::new()),
        }
    }
}

impl Shared {
    /// A call's or test's state, from the settings.
    pub fn from_settings(settings: &crate::settings::Settings) -> Arc<Self> {
        let shared = Arc::new(Self::default());
        shared.filter.set(settings);
        shared.set_gains(settings);
        shared.set_volumes(settings);
        shared
    }

    /// The microphone's and the speakers' volume, and the devices, from the
    /// settings.
    pub fn set_gains(&self, settings: &crate::settings::Settings) {
        let gain = |percent: u16| (f32::from(percent) / 100.0).to_bits();
        self.mic_gain
            .store(gain(settings.input_volume), Ordering::Relaxed);
        self.speaker_gain
            .store(gain(settings.output_volume), Ordering::Relaxed);
        *self.input_device.lock().unwrap_or_else(|e| e.into_inner()) =
            settings.input_device.clone();
        *self.output_device.lock().unwrap_or_else(|e| e.into_inner()) =
            settings.output_device.clone();
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
        self.volumes_version.fetch_add(1, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("call::Shared")
    }
}

/// Appends a line of call statistics to `~/.cache/discofast/call.log`,
/// so a call that sounded choppy can be looked at afterwards. The file is
/// started over when it passes a megabyte.
pub fn report(line: &str) {
    use std::io::Write;
    let Some(dir) = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .map(|d| d.join("discofast"))
    else {
        return;
    };
    let path = dir.join("call.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::remove_file(&path);
    }
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{} {line}", jiff::Timestamp::now().strftime("%Y-%m-%d %H:%M:%S"));
    }
}

/// "Teste do microfone", as Discord's web client does it: the microphone
/// sent through the voice engine to itself and played, so it is heard as
/// the others would hear it; and its level for the meter, until dropped.
pub struct MicTest {
    pub shared: Arc<Shared>,
    stop: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MicTest {
    pub fn start(settings: &crate::settings::Settings) -> Result<Self, String> {
        let shared = Shared::from_settings(settings);
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        let test = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("mic-test".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(e) => return log::warn!("mic test: {e}"),
                };
                let loopback = runtime.block_on(crate::discord::voice::Loopback::start(test));
                match loopback {
                    Ok(loopback) => {
                        while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                            stopped.recv_timeout(Duration::from_millis(50))
                        {
                            runtime.block_on(loopback.exchange());
                        }
                        drop(loopback);
                    }
                    Err(e) => log::warn!("mic test: {e}"),
                }
            })
            .map_err(|e| e.to_string())?;
        crate::devices::route(settings.input_device.clone(), settings.output_device.clone());
        Ok(Self {
            shared,
            stop: Some(stop),
            thread: Some(thread),
        })
    }
}

impl Drop for MicTest {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
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
        let shared = Shared::from_settings(&self.settings);
        shared.deaf.store(self.settings.deafened, Ordering::Relaxed);
        shared.muted.store(
            self.settings.muted || self.settings.deafened,
            Ordering::Relaxed,
        );
        // The microphone test gives the microphone to the call.
        self.mic_test = None;
        self.call = Some(Session {
            guild,
            channel,
            state: State::Joining,
            session_id: None,
            server: None,
            shared,
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
        let server = crate::discord::voice::Server {
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
