//! A voice call as Discord's web client makes it: its RTCControlSocket
//! (the voice gateway, version 9) and its UnifiedConnection (a WebRTC
//! peer connection to the voice server, protocol "webrtc"), with DAVE on
//! the encoded frames. Ported from the client's own code; the names in
//! backticks are its.
//!
//! The way through: the microphone (platform device) → WebRTC's audio
//! processing → Opus → DAVE → SRTP → Discord; Discord → SRTP → DAVE →
//! NetEq (jitter buffer, loss concealment) → Opus → the engine's mixer →
//! the speakers, whose sound is the echo canceller's reference.

pub mod dave;
pub mod engine;
pub mod sdp;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use libwebrtc::MediaType;
use libwebrtc::audio_stream::native::NativeAudioStream;
use libwebrtc::audio_track::RtcAudioTrack;
use libwebrtc::media_stream_track::MediaStreamTrack;
use libwebrtc::peer_connection::{OfferOptions, PeerConnection, PeerConnectionState, TrackEvent};
use libwebrtc::peer_connection_factory::RtcConfiguration;
use libwebrtc::rtp_parameters::RtpEncodingParameters;
use libwebrtc::rtp_transceiver::{RtpTransceiver, RtpTransceiverDirection, RtpTransceiverInit};
use libwebrtc::session_description::{SdpType, SessionDescription};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::call::Shared;
use dave::{Dave, Reply};

/// The voice gateway's version, as the web client asks for it.
const GATEWAY: u8 = 9;
/// The microphone's bitrate: Discord's default voice bitrate.
const BITRATE: u64 = 64_000;
/// The longest between two heartbeats (`eK`).
const MAX_HEARTBEAT: Duration = Duration::from_secs(5);
/// How long the call may take to connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Between two lines of call statistics in the log.
const REPORT: Duration = Duration::from_secs(10);

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

enum Control {
    Mute(bool),
    Leave,
}

/// A connected call; dropping it leaves.
pub struct Handle {
    control: mpsc::UnboundedSender<Control>,
}

impl Handle {
    /// Stops or starts sending the microphone.
    pub fn mute(&self, mute: bool) {
        let _ = self.control.send(Control::Mute(mute));
    }

    pub fn leave(&self) {
        let _ = self.control.send(Control::Leave);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.leave();
    }
}

/// Each call's frames find its DAVE by the number its transformers carry.
static DAVES: RwLock<Option<HashMap<u64, Arc<Dave>>>> = RwLock::new(None);
static CALLS: AtomicU64 = AtomicU64::new(1);

fn install_transform() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        libwebrtc::native::discofast::set_transform(Some(Arc::new(
            |context, encrypt, ssrc, frame: &[u8], out: &mut Vec<u8>| {
                let dave = DAVES
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .and_then(|d| d.get(&context).cloned());
                match dave {
                    Some(dave) if encrypt => dave.encrypt(frame, out),
                    Some(dave) => dave.decrypt(ssrc, frame, out),
                    None => {
                        out.extend_from_slice(frame);
                        true
                    }
                }
            },
        )));
    });
}

/// Connects and starts sending the microphone. `wake` redraws the window
/// when who is talking changes; `gone` tells that the call dropped after
/// it had connected.
pub async fn connect(
    server: Server,
    shared: Arc<Shared>,
    wake: impl Fn() + Send + Sync + 'static,
    gone: impl Fn(String) + Send + Sync + 'static,
) -> Result<Handle, String> {
    let (control, controls) = mpsc::unbounded_channel();
    let (ready, connected) = oneshot::channel();
    let ready = Arc::new(Mutex::new(Some(ready)));
    let first = Arc::clone(&ready);
    tokio::spawn(async move {
        let result = run(server, shared, controls, Arc::new(wake), first).await;
        let unanswered = ready.lock().unwrap_or_else(|e| e.into_inner()).take();
        match (result, unanswered) {
            (Err(error), Some(ready)) => {
                let _ = ready.send(Err(error));
            }
            (Err(error), None) => gone(error),
            (Ok(()), _) => {}
        }
    });
    match connected.await {
        Ok(Ok(())) => Ok(Handle { control }),
        Ok(Err(error)) => Err(error),
        Err(_) => Err("A chamada de voz não conectou.".to_owned()),
    }
}

type Ready = Arc<Mutex<Option<oneshot::Sender<Result<(), String>>>>>;

/// What libwebrtc reports from its own threads.
enum Peer {
    Track(TrackEvent),
    State(PeerConnectionState),
}

async fn run(
    server: Server,
    shared: Arc<Shared>,
    mut controls: mpsc::UnboundedReceiver<Control>,
    wake: Arc<dyn Fn() + Send + Sync>,
    ready: Ready,
) -> Result<(), String> {
    install_transform();
    engine::engine();
    engine::attach(&shared);
    // `RTCConnection`: a call that drops after it connected is made again
    // from the start (`_scheduleReconnect`), with a new identify, peer
    // connection and DAVE group.
    let mut backoff = Backoff::new(1_000, 10_000);
    let result = loop {
        let context = CALLS.fetch_add(1, Ordering::Relaxed);
        let dave = Dave::new(server.user, server.channel);
        DAVES
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_default()
            .insert(context, Arc::clone(&dave));
        let result = call(&server, &shared, &mut controls, &wake, &ready, context, &dave).await;
        DAVES
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_default()
            .remove(&context);
        shared.talking.store(false, Ordering::Relaxed);
        shared.speaking.lock().unwrap_or_else(|e| e.into_inner()).clear();
        wake();
        let error = match result {
            Ok(()) => break Ok(()),
            Err(error) => error,
        };
        // Never connected: the call failed, nothing to go back to.
        if ready.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            break Err(error);
        }
        let wait = backoff.fail();
        log::warn!("voice: {error}; reconnecting in {:.1} s", wait.as_secs_f32());
        crate::call::report(&format!("disconnected: {error}; reconnecting"));
        let sleep = tokio::time::sleep(wait);
        tokio::pin!(sleep);
        let left = loop {
            tokio::select! {
                () = &mut sleep => break false,
                control = controls.recv() => match control {
                    Some(Control::Mute(mute)) => shared.muted.store(mute, Ordering::Relaxed),
                    Some(Control::Leave) | None => break true,
                },
            }
        };
        if left {
            break Ok(());
        }
    };
    engine::detach(&shared);
    result
}

/// Discord's backoff: each failure waits up to three times longer, at
/// random, up to a ceiling; a success starts over.
struct Backoff {
    min: f64,
    max: f64,
    current: f64,
    fails: u32,
}

impl Backoff {
    fn new(min_ms: u64, max_ms: u64) -> Self {
        Self {
            min: min_ms as f64,
            max: max_ms as f64,
            current: min_ms as f64,
            fails: 0,
        }
    }

    fn fail(&mut self) -> Duration {
        self.fails += 1;
        let grow = 2.0 * self.current * rand::random::<f64>();
        self.current = (self.current + grow).min(self.max);
        Duration::from_millis(self.current as u64)
    }

    fn succeed(&mut self) {
        self.fails = 0;
        self.current = self.min;
    }
}

#[allow(clippy::too_many_arguments)]
async fn call(
    server: &Server,
    shared: &Arc<Shared>,
    controls: &mut mpsc::UnboundedReceiver<Control>,
    wake: &Arc<dyn Fn() + Send + Sync>,
    ready: &Ready,
    context: u64,
    dave: &Arc<Dave>,
) -> Result<(), String> {
    let mut socket = Socket::open(&server.endpoint).await?;
    // `identify`
    socket
        .send(
            0,
            json!({
                "server_id": server.place.to_string(),
                "channel_id": server.channel.to_string(),
                "user_id": server.user.to_string(),
                "session_id": server.session_id,
                "token": server.token,
                "max_dave_protocol_version": davey::DAVE_PROTOCOL_VERSION,
                "video": false,
                "streams": [],
            }),
        )
        .await?;

    let (peers, mut peer_events) = mpsc::unbounded_channel();
    let pc = engine::engine()
        .factory
        .create_peer_connection(RtcConfiguration::default())
        .map_err(|e| format!("peer connection: {e}"))?;
    {
        let tracks = peers.clone();
        pc.on_track(Some(Box::new(move |event| {
            let _ = tracks.send(Peer::Track(event));
        })));
        pc.on_connection_state_change(Some(Box::new(move |state| {
            let _ = peers.send(Peer::State(state));
        })));
    }
    let mic = engine::engine().microphone(shared);
    let muted = shared.muted.load(Ordering::Relaxed);
    mic.set_enabled(!muted);
    // The microphone's transceiver: recvonly with the bitrate, then the
    // track on it, sendrecv (`addTransceiver` and `e0`).
    let audio = pc
        .add_transceiver(
            MediaStreamTrack::Audio(mic.clone()),
            RtpTransceiverInit {
                direction: RtpTransceiverDirection::SendRecv,
                stream_ids: vec![],
                send_encodings: vec![RtpEncodingParameters {
                    active: true,
                    max_bitrate: Some(BITRATE),
                    ..Default::default()
                }],
            },
        )
        .map_err(|e| format!("transceiver: {e}"))?;
    libwebrtc::native::discofast::set_sender_transformer(&audio.sender(), context);
    libwebrtc::native::discofast::set_receiver_transformer(&audio.receiver(), context);

    let mut voice = Connection {
        pc,
        context,
        dave: Arc::clone(dave),
        remote: None,
        local: sdp::Local::default(),
        users: HashMap::new(),
        owners: HashMap::new(),
        unassigned: Vec::new(),
        inactive: Vec::new(),
        assigned: HashMap::new(),
        transceivers: 1,
        nack: false,
        needed: false,
        answered: false,
    };
    // The first offer, cut down for SELECT_PROTOCOL once READY is in.
    let offer = voice.offer().await?;

    let mut heartbeat: Option<tokio::time::Interval> = None;
    let mut audio_ssrc = 0u32;
    let mut selected = false;
    let mut connected = false;
    let mut speaking_sent: Option<bool> = None;
    let mut tracks: HashMap<u64, Remote> = HashMap::new();
    let mut volumes_seen = u64::MAX;
    let mut filter_seen = shared.filter.version.load(Ordering::Relaxed);
    let mut mic = mic;
    let (talk, mut talking) = mpsc::unbounded_channel::<bool>();
    let stop = Arc::new(AtomicBool::new(false));
    let activity = {
        let wake = Arc::clone(wake);
        engine::watch_activity(Arc::clone(shared), Arc::clone(&stop), move |speaking| {
            let _ = talk.send(speaking);
            wake();
        })
    };
    let _activity = Joined(Some(activity), stop);
    let mut report = tokio::time::interval(REPORT);
    let mut settings = tokio::time::interval(Duration::from_millis(250));
    let mut last = Stats::default();
    let deadline = tokio::time::sleep(CONNECT_TIMEOUT);
    tokio::pin!(deadline);

    loop {
        let beat = async {
            match &mut heartbeat {
                Some(interval) => interval.tick().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = &mut deadline, if !connected => {
                return Err("A chamada de voz não conectou a tempo.".to_owned());
            }
            _ = beat => {
                // No answer to the last heartbeat: the socket is dead
                // (`handleHeartbeatTimeout`).
                if socket.awaiting_ack {
                    socket.resume(4000, "Heartbeat timeout.", server).await?;
                    heartbeat = None;
                    continue;
                }
                socket.heartbeat().await?;
            }
            incoming = socket.next() => {
                let incoming = incoming.unwrap_or_else(|e| Incoming::Closed(1006, e));
                let (op, d) = match incoming {
                    Incoming::Json(op, d) => (op, Some(d)),
                    Incoming::Binary(op, body) => {
                        for reply in dave.binary(op, &body) {
                            socket.reply(reply).await?;
                        }
                        // `disconnectForRepeatedMLSInvalidMessages`
                        if dave.failing() {
                            socket.close_with(4803).await;
                            return Err("Mensagens MLS inválidas em sequência.".to_owned());
                        }
                        (u64::from(op), None)
                    }
                    // `handleClose`: resumed when it can be, else the call
                    // starts over.
                    Incoming::Closed(code, reason) => {
                        socket.resume(code, &reason, server).await?;
                        heartbeat = None;
                        continue;
                    }
                };
                let Some(d) = d else { continue };
                match op {
                    // `handleHello`
                    8 => {
                        let interval = d["heartbeat_interval"].as_f64().unwrap_or(5_000.0);
                        socket.version = d["v"].as_u64().unwrap_or(3);
                        let period = Duration::from_millis(interval as u64).min(MAX_HEARTBEAT);
                        let mut timer = tokio::time::interval(period);
                        timer.tick().await;
                        heartbeat = Some(timer);
                    }
                    // `handleReady`: SELECT_PROTOCOL with the offer.
                    2 => {
                        socket.backoff.succeed();
                        if socket.version >= 6 {
                            socket.send(16, json!({})).await?;
                        }
                        if !selected {
                            selected = true;
                            audio_ssrc = voice.local.audio_ssrc;
                            let data = sdp::for_server(&offer);
                            socket
                                .send(
                                    1,
                                    json!({
                                        "protocol": "webrtc",
                                        "data": data,
                                        "sdp": data,
                                        "codecs": sdp::codecs(&voice.local),
                                        "rtc_connection_id": uuid(),
                                    }),
                                )
                                .await?;
                        }
                    }
                    // SELECT_PROTOCOL_ACK: codecs, the server's SDP and DAVE.
                    4 => {
                        if let Some(text) = d["sdp"].as_str() {
                            voice.remote = Some(sdp::parse_remote(text)?);
                            voice.needed = true;
                        }
                        if let Some(id) = d["media_session_id"].as_str() {
                            log::info!("voice: media session {id}");
                        }
                        socket.resumable = true;
                        let version = d["dave_protocol_version"].as_u64().unwrap_or(0) as u16;
                        log::info!("voice: DAVE protocol {version}");
                        for reply in dave.init(version) {
                            socket.reply(reply).await?;
                        }
                    }
                    // Someone speaking, and the ssrc they send on.
                    5 => {
                        let user = crate::discord::parse::id(&d["user_id"]);
                        let ssrc = d["ssrc"].as_u64().unwrap_or(0) as u32;
                        let flags = match &d["speaking"] {
                            Value::Bool(b) => u64::from(*b),
                            v => v.as_u64().unwrap_or(0),
                        };
                        if let Some(user) = user
                            && user != server.user
                            && flags != 0
                        {
                            voice.create_user(user, ssrc);
                        }
                    }
                    // Someone's audio and video ssrcs.
                    12 => {
                        let user = crate::discord::parse::id(&d["user_id"]);
                        let ssrc = d["audio_ssrc"].as_u64().unwrap_or(0) as u32;
                        if let Some(user) = user
                            && user != server.user
                            && ssrc != 0
                        {
                            voice.create_user(user, ssrc);
                        }
                    }
                    // Clients connected: known to DAVE.
                    11 => {
                        let users = d["user_ids"].as_array().into_iter().flatten();
                        let mut known = dave.known.lock().unwrap_or_else(|e| e.into_inner());
                        known.extend(
                            users
                                .filter_map(crate::discord::parse::id)
                                .filter(|u| *u != server.user),
                        );
                    }
                    13 => {
                        if let Some(user) = crate::discord::parse::id(&d["user_id"]) {
                            dave.known.lock().unwrap_or_else(|e| e.into_inner()).remove(&user);
                            voice.destroy_user(user);
                            tracks.remove(&user);
                            shared
                                .speaking
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .remove(&user);
                            wake();
                        }
                    }
                    // The heartbeat's answer.
                    6 => {
                        socket.awaiting_ack = false;
                        socket.last_ack = Some(std::time::Instant::now());
                    }
                    9 => {
                        log::info!("voice: resumed");
                        socket.backoff.succeed();
                    }
                    op @ (21 | 22 | 24) => {
                        for reply in dave.json(op, &d) {
                            socket.reply(reply).await?;
                        }
                    }
                    _ => {}
                }
            }
            Some(event) = peer_events.recv() => match event {
                Peer::State(state) => {
                    log::info!("voice: peer connection {state:?}");
                    match state {
                        PeerConnectionState::Connected if !connected => {
                            connected = true;
                            if let Some(ready) =
                                ready.lock().unwrap_or_else(|e| e.into_inner()).take()
                            {
                                let _ = ready.send(Ok(()));
                            }
                            crate::devices::route(
                                shared.input_device.lock().unwrap_or_else(|e| e.into_inner()).clone(),
                                shared.output_device.lock().unwrap_or_else(|e| e.into_inner()).clone(),
                            );
                        }
                        PeerConnectionState::Failed => {
                            return Err("A conexão de voz caiu (sem rota).".to_owned());
                        }
                        PeerConnectionState::Closed => {
                            return Err("A conexão de voz fechou.".to_owned());
                        }
                        _ => {}
                    }
                }
                Peer::Track(event) => {
                    if let Some((user, remote)) = voice.track(event, shared, wake) {
                        tracks.insert(user, remote);
                        volumes_seen = u64::MAX;
                        crate::devices::route(
                            shared.input_device.lock().unwrap_or_else(|e| e.into_inner()).clone(),
                            shared.output_device.lock().unwrap_or_else(|e| e.into_inner()).clone(),
                        );
                    }
                }
            },
            Some(speaking) = talking.recv() => {
                if audio_ssrc != 0 && speaking_sent != Some(speaking) {
                    speaking_sent = Some(speaking);
                    socket
                        .send(5, json!({ "speaking": u8::from(speaking), "delay": 0, "ssrc": audio_ssrc }))
                        .await?;
                }
            }
            control = controls.recv() => match control {
                Some(Control::Mute(mute)) => {
                    shared.muted.store(mute, Ordering::Relaxed);
                    mic.set_enabled(!mute);
                    if mute && speaking_sent == Some(true) && audio_ssrc != 0 {
                        speaking_sent = Some(false);
                        socket
                            .send(5, json!({ "speaking": 0, "delay": 0, "ssrc": audio_ssrc }))
                            .await?;
                    }
                }
                Some(Control::Leave) | None => {
                    socket.close().await;
                    voice.pc.close();
                    return Ok(());
                }
            },
            _ = settings.tick() => {
                let version = shared.volumes_version.load(Ordering::Relaxed);
                if version != volumes_seen {
                    volumes_seen = version;
                    let volumes = shared.volumes.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    for (user, remote) in &tracks {
                        let gain = volumes.get(&crate::model::Id::from(*user)).copied().unwrap_or(1.0);
                        libwebrtc::native::discofast::set_audio_track_volume(&remote.track, f64::from(gain));
                    }
                }
                // Echo cancellation or automatic gain changed: a new
                // microphone track with them, on the same sender.
                let filter = shared.filter.version.load(Ordering::Relaxed);
                if filter != filter_seen {
                    filter_seen = filter;
                    let track = engine::engine().microphone(shared);
                    track.set_enabled(!shared.muted.load(Ordering::Relaxed));
                    if audio.sender().set_track(Some(MediaStreamTrack::Audio(track.clone()))).is_ok() {
                        mic = track;
                    }
                }
            }
            _ = report.tick() => {
                if connected {
                    let stats = Stats::read(&voice.pc).await;
                    let line = stats.since(&last, tracks.len());
                    log::info!("voice: {line}");
                    crate::call::report(&line);
                    last = stats;
                }
            }
        }
        if voice.needed && voice.remote.is_some() {
            voice.negotiate().await?;
        }
    }
}

/// libwebrtc's offer options say "do not receive" for a false; the web
/// client leaves them unset, which is what true keeps here.
fn offer_options() -> OfferOptions {
    OfferOptions {
        ice_restart: false,
        offer_to_receive_audio: true,
        offer_to_receive_video: false,
    }
}

/// What the closing code means, in words.
fn closed(code: u16, reason: &str) -> String {
    match code {
        4014 => "Você foi desconectado da chamada.".to_owned(),
        4006 => "A sessão de voz expirou.".to_owned(),
        4004 => "O servidor de voz recusou a entrada.".to_owned(),
        4015 => "O servidor de voz caiu.".to_owned(),
        _ if reason.is_empty() => format!("O servidor de voz fechou ({code})."),
        _ => format!("O servidor de voz fechou ({code}: {reason})."),
    }
}

fn uuid() -> String {
    let bytes: [u8; 16] = rand::random();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-4{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[13..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Joins the activity thread when the call ends.
struct Joined(Option<std::thread::JoinHandle<()>>, Arc<AtomicBool>);

impl Drop for Joined {
    fn drop(&mut self) {
        self.1.store(true, Ordering::Relaxed);
        if let Some(thread) = self.0.take() {
            let _ = thread.join();
        }
    }
}

/// Someone heard in the call: their track, for the volume, and the task
/// that tells when they talk.
struct Remote {
    track: RtcAudioTrack,
    level: tokio::task::JoinHandle<()>,
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.level.abort();
    }
}

/// The peer connection and who is heard on which m-line
/// (`UnifiedConnection`).
struct Connection {
    pc: PeerConnection,
    context: u64,
    dave: Arc<Dave>,
    /// The server's SDP, once SELECT_PROTOCOL_ACK brings it.
    remote: Option<sdp::Remote>,
    local: sdp::Local,
    /// Each person's audio ssrc.
    users: HashMap<u64, u32>,
    owners: HashMap<u32, u64>,
    /// People heard of but not yet on an m-line.
    unassigned: Vec<(u64, u32)>,
    /// Receive-only m-lines nobody is on.
    inactive: Vec<String>,
    /// Who is on each m-line.
    assigned: HashMap<String, (u64, u32)>,
    transceivers: usize,
    nack: bool,
    needed: bool,
    /// An answer was taken; the next round needs an offer of its own.
    answered: bool,
}

impl Connection {
    async fn offer(&mut self) -> Result<String, String> {
        let offer = self
            .pc
            .create_offer(offer_options())
            .await
            .map_err(|e| format!("offer: {e}"))?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .map_err(|e| format!("local description: {e}"))?;
        // The pending offer itself: libwebrtc's "current" description is
        // still the last one answered.
        let text = offer.to_string();
        self.local = sdp::parse_local(&text);
        Ok(text)
    }

    /// `createUser`
    fn create_user(&mut self, user: u64, ssrc: u32) {
        if ssrc == 0 {
            return;
        }
        if self.owners.get(&ssrc).is_some_and(|owner| *owner != user) {
            log::warn!("voice: ssrc {ssrc} is someone else's");
            return;
        }
        if self.users.get(&user) == Some(&ssrc) {
            return;
        }
        if let Some(old) = self.users.insert(user, ssrc) {
            self.owners.remove(&old);
        }
        self.owners.insert(ssrc, user);
        self.dave.set_owner(ssrc, user);
        self.unassigned.push((user, ssrc));
        log::info!("voice: user {user} on ssrc {ssrc}");
        self.needed = true;
    }

    /// `destroyUser`
    fn destroy_user(&mut self, user: u64) {
        self.dave.forget_user(user);
        if let Some(ssrc) = self.users.remove(&user) {
            self.owners.remove(&ssrc);
            self.needed = true;
        }
        self.unassigned.retain(|(u, _)| *u != user);
    }

    /// `handleNegotiationNeeded` then `setRemoteAnswer`.
    async fn negotiate(&mut self) -> Result<(), String> {
        self.needed = false;
        let Some(remote) = self.remote.clone() else {
            return Ok(());
        };
        let mut waiting = std::mem::take(&mut self.unassigned);
        // Receive-only m-lines for everyone not yet on one.
        let missing = waiting.len().saturating_sub(self.inactive.len());
        for _ in 0..missing {
            let transceiver = self
                .pc
                .add_transceiver_for_media(
                    MediaType::Audio,
                    RtpTransceiverInit {
                        direction: RtpTransceiverDirection::RecvOnly,
                        stream_ids: vec![],
                        send_encodings: vec![],
                    },
                )
                .map_err(|e| format!("transceiver: {e}"))?;
            set_transformers(&transceiver, self.context);
            self.inactive.push(self.transceivers.to_string());
            self.transceivers += 1;
        }
        // The server's first SDP answers the offer already made (`setSDP`);
        // after that, each round is a new offer.
        if self.answered || missing > 0 {
            self.offer().await?;
        }
        self.answered = true;
        let Some(opus) = self.local.opus else {
            return Err("O WebRTC não ofereceu Opus.".to_owned());
        };
        let mut sections = Vec::new();
        for outbound in &self.local.outbound {
            let mid = outbound.mid.clone();
            if let Some((user, ssrc)) = self.assigned.get(&mid).copied()
                && self.users.get(&user) != Some(&ssrc)
            {
                self.assigned.remove(&mid);
            }
            if !self.assigned.contains_key(&mid)
                && let Some(at) = self.inactive.iter().position(|m| *m == mid)
                && let Some(stream) = waiting.pop()
            {
                self.inactive.remove(at);
                self.assigned.insert(mid.clone(), stream);
            }
            sections.push(sdp::Section {
                mid: mid.clone(),
                direction: outbound.direction.reversed(),
                stream: self.assigned.get(&mid).copied(),
            });
        }
        let answer = sdp::answer(&remote, &sections, opus, &self.local.extensions, self.nack);
        let description = SessionDescription::parse(&answer, SdpType::Answer)
            .map_err(|e| format!("answer: {e}"))?;
        if let Err(e) = self.pc.set_remote_description(description).await {
            log::warn!("voice: failed to set remote answer: {e}");
            log::debug!("voice: answer was\n{answer}");
        }
        // Whoever is left waits for the next round.
        if !waiting.is_empty() {
            waiting.extend(std::mem::take(&mut self.unassigned));
            self.unassigned = waiting;
            self.needed = true;
        }
        Ok(())
    }

    /// `handleTrack`: someone's sound arrived, on a stream named
    /// "<user>-<ssrc>".
    fn track(
        &self,
        event: TrackEvent,
        shared: &Arc<Shared>,
        wake: &Arc<dyn Fn() + Send + Sync>,
    ) -> Option<(u64, Remote)> {
        let MediaStreamTrack::Audio(track) = event.track else {
            return None;
        };
        let stream = event.streams.first()?.id();
        let (user, ssrc) = stream.split_once('-')?;
        let (user, ssrc): (u64, u32) = (user.parse().ok()?, ssrc.parse().ok()?);
        if self.users.get(&user) != Some(&ssrc) {
            log::info!("voice: stale ssrc {ssrc} for user {user}");
            return None;
        }
        let gain = shared
            .volumes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&crate::model::Id::from(user))
            .copied()
            .unwrap_or(1.0);
        libwebrtc::native::discofast::set_audio_track_volume(&track, f64::from(gain));
        let level = {
            let shared = Arc::clone(shared);
            let wake = Arc::clone(wake);
            let mut frames = NativeAudioStream::new(track.clone(), 48_000, 1);
            tokio::spawn(async move {
                let mut level = engine::Level::default();
                let mut talking = false;
                let id = crate::model::Id::from(user);
                while let Some(frame) = frames.next().await {
                    let now = level.feed(&frame.data);
                    if now != talking {
                        talking = now;
                        let mut speaking =
                            shared.speaking.lock().unwrap_or_else(|e| e.into_inner());
                        if now {
                            speaking.insert(id);
                        } else {
                            speaking.remove(&id);
                        }
                        drop(speaking);
                        wake();
                    }
                }
            })
        };
        Some((user, Remote { track, level }))
    }
}

fn set_transformers(transceiver: &RtpTransceiver, context: u64) {
    libwebrtc::native::discofast::set_sender_transformer(&transceiver.sender(), context);
    libwebrtc::native::discofast::set_receiver_transformer(&transceiver.receiver(), context);
}

/// The voice gateway (`RTCControlSocket`).
struct Socket {
    sink: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    stream: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    /// The server's version, from HELLO.
    version: u64,
    /// The last sequence number received, for heartbeats.
    seq: i64,
    endpoint: String,
    /// SELECT_PROTOCOL_ACK came: the session can be resumed.
    resumable: bool,
    awaiting_ack: bool,
    last_ack: Option<std::time::Instant>,
    backoff: Backoff,
}

enum Incoming {
    Json(u64, Value),
    Binary(u8, Vec<u8>),
    Closed(u16, String),
}

impl Socket {
    async fn open(endpoint: &str) -> Result<Self, String> {
        let (sink, stream) = Self::dial(endpoint).await?;
        Ok(Self {
            sink,
            stream,
            version: 3,
            seq: -1,
            endpoint: endpoint.to_owned(),
            resumable: false,
            awaiting_ack: false,
            last_ack: None,
            backoff: Backoff::new(1_000, 5_000),
        })
    }

    #[allow(clippy::type_complexity)]
    async fn dial(
        endpoint: &str,
    ) -> Result<
        (
            futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                Message,
            >,
            futures_util::stream::SplitStream<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
            >,
        ),
        String,
    > {
        let url = format!("wss://{}/?v={GATEWAY}", endpoint.trim_end_matches('/'));
        let (socket, _) = tokio::time::timeout(
            Duration::from_secs(20),
            tokio_tungstenite::connect_async(url),
        )
        .await
        .map_err(|_| "O servidor de voz não respondeu.".to_owned())?
        .map_err(|e| e.to_string())?;
        Ok(socket.split())
    }

    /// `handleClose` then `reconnect` and `doResumeOrClose`: a new socket
    /// that takes the session up where it was, while the peer connection
    /// carries on. An error means the call must start over.
    async fn resume(&mut self, mut code: u16, reason: &str, server: &Server) -> Result<(), String> {
        let mut reason = reason.to_owned();
        loop {
            log::warn!("voice socket closed ({code}: {reason})");
            if matches!(code, 4004 | 4006 | 4011 | 4015) || self.backoff.fails > 3 {
                return Err(closed(code, &reason));
            }
            tokio::time::sleep(self.backoff.fail()).await;
            let fresh = self
                .last_ack
                .is_none_or(|at| at.elapsed() <= Duration::from_secs(60));
            if !self.resumable || !fresh {
                return Err(closed(4801, "Cannot resume connection."));
            }
            match Self::dial(&self.endpoint).await {
                Ok((sink, stream)) => {
                    self.sink = sink;
                    self.stream = stream;
                    self.awaiting_ack = false;
                    self.last_ack = Some(std::time::Instant::now());
                    log::info!("voice: resuming, seq_ack {}", self.seq);
                    let seq = self.seq;
                    self.send(
                        7,
                        json!({
                            "token": server.token,
                            "session_id": server.session_id,
                            "server_id": server.place.to_string(),
                            "channel_id": server.channel.to_string(),
                            "seq_ack": seq,
                        }),
                    )
                    .await?;
                    return Ok(());
                }
                Err(e) => {
                    code = 1006;
                    reason = e;
                }
            }
        }
    }

    async fn send(&mut self, op: u8, d: Value) -> Result<(), String> {
        let text = json!({ "op": op, "d": d }).to_string();
        self.sink
            .send(Message::Text(text.into()))
            .await
            .map_err(|e| e.to_string())
    }

    /// A binary message: the opcode, then the body.
    async fn send_binary(&mut self, op: u8, body: &[u8]) -> Result<(), String> {
        let mut data = Vec::with_capacity(body.len() + 1);
        data.push(op);
        data.extend_from_slice(body);
        self.sink
            .send(Message::Binary(data.into()))
            .await
            .map_err(|e| e.to_string())
    }

    async fn reply(&mut self, reply: Reply) -> Result<(), String> {
        match reply {
            Reply::Json(value) => {
                let op = value["op"].as_u64().unwrap_or(0) as u8;
                self.send(op, value["d"].clone()).await
            }
            Reply::Binary(op, body) => self.send_binary(op, &body).await,
        }
    }

    /// `sendHeartbeat`
    async fn heartbeat(&mut self) -> Result<(), String> {
        self.awaiting_ack = true;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        if self.version >= 8 {
            self.send(3, json!({ "t": now, "seq_ack": self.seq })).await
        } else {
            self.send(3, json!(now)).await
        }
    }

    async fn next(&mut self) -> Result<Incoming, String> {
        loop {
            let Some(message) = self.stream.next().await else {
                return Ok(Incoming::Closed(1006, String::new()));
            };
            match message.map_err(|e| e.to_string())? {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                    if let Some(seq) = value["seq"].as_i64() {
                        self.seq = seq;
                    }
                    let op = value["op"].as_u64().unwrap_or(u64::MAX);
                    return Ok(Incoming::Json(op, value["d"].clone()));
                }
                // `parseBinaryMessage`: from version 8 a sequence number
                // comes first.
                Message::Binary(data) => {
                    let skip = if self.version >= 8 { 2 } else { 0 };
                    if data.len() < skip + 1 {
                        continue;
                    }
                    if skip == 2 {
                        self.seq = i64::from(u16::from_be_bytes([data[0], data[1]]));
                    }
                    return Ok(Incoming::Binary(data[skip], data[skip + 1..].to_vec()));
                }
                Message::Close(frame) => {
                    let (code, reason) = frame
                        .map(|f| (u16::from(f.code), f.reason.to_string()))
                        .unwrap_or((1000, String::new()));
                    return Ok(Incoming::Closed(code, reason));
                }
                _ => {}
            }
        }
    }

    async fn close(&mut self) {
        let _ = self.sink.send(Message::Close(None)).await;
    }

    async fn close_with(&mut self, code: u16) {
        use tokio_tungstenite::tungstenite::protocol::CloseFrame;
        let frame = CloseFrame {
            code: code.into(),
            reason: "".into(),
        };
        let _ = self.sink.send(Message::Close(Some(frame))).await;
    }
}

/// The inbound and outbound audio counters, for the log.
#[derive(Default)]
struct Stats {
    received: u64,
    lost: i64,
    concealed: u64,
    samples: u64,
    jitter_ms: f64,
    buffer_ms: f64,
    sent: u64,
    rtt_ms: f64,
}

impl Stats {
    async fn read(pc: &PeerConnection) -> Self {
        use libwebrtc::stats::RtcStats;
        let mut stats = Stats::default();
        let Ok(all) = pc.get_stats().await else {
            return stats;
        };
        let mut buffers = Vec::new();
        for entry in all {
            match entry {
                RtcStats::InboundRtp(s) if s.stream.kind == "audio" => {
                    stats.received += s.received.packets_received;
                    stats.lost += s.received.packets_lost;
                    stats.concealed += s.inbound.concealed_samples;
                    stats.samples += s.inbound.total_samples_received;
                    stats.jitter_ms = stats.jitter_ms.max(s.received.jitter * 1000.0);
                    if s.inbound.jitter_buffer_emitted_count > 0 {
                        buffers.push(
                            s.inbound.jitter_buffer_delay
                                / s.inbound.jitter_buffer_emitted_count as f64
                                * 1000.0,
                        );
                    }
                }
                RtcStats::OutboundRtp(s) if s.stream.kind == "audio" => {
                    stats.sent += s.sent.packets_sent;
                }
                RtcStats::CandidatePair(s) if s.candidate_pair.nominated => {
                    stats.rtt_ms = s.candidate_pair.current_round_trip_time * 1000.0;
                }
                _ => {}
            }
        }
        stats.buffer_ms = buffers.into_iter().fold(0.0, f64::max);
        stats
    }

    fn since(&self, before: &Stats, people: usize) -> String {
        let received = self.received.saturating_sub(before.received);
        let lost = (self.lost - before.lost).max(0);
        let samples = self.samples.saturating_sub(before.samples).max(1);
        let concealed = self.concealed.saturating_sub(before.concealed);
        format!(
            "{people} heard; received {received} packets, lost {lost}, concealed {:.1}%, \
             jitter {:.0} ms, buffer {:.0} ms; sent {} packets; rtt {:.0} ms",
            concealed as f64 * 100.0 / samples as f64,
            self.jitter_ms,
            self.buffer_ms,
            self.sent.saturating_sub(before.sent),
            self.rtt_ms,
        )
    }
}

/// The microphone test, as the web client's: the microphone sent from one
/// peer connection to another on this machine and played, through the
/// same processing a call uses.
pub struct Loopback {
    sender: PeerConnection,
    receiver: PeerConnection,
    /// ICE candidates each side found, for the other.
    candidates: std::sync::mpsc::Receiver<(bool, libwebrtc::ice_candidate::IceCandidate)>,
    shared: Arc<Shared>,
    _activity: Joined,
}

impl Loopback {
    pub async fn start(shared: Arc<Shared>) -> Result<Self, String> {
        let engine = engine::engine();
        engine::attach(&shared);
        let factory = &engine.factory;
        let sender = factory
            .create_peer_connection(RtcConfiguration::default())
            .map_err(|e| e.to_string())?;
        let receiver = factory
            .create_peer_connection(RtcConfiguration::default())
            .map_err(|e| e.to_string())?;
        let (found, candidates) = std::sync::mpsc::channel();
        for (pc, to_receiver) in [(&sender, true), (&receiver, false)] {
            let found = found.clone();
            pc.on_ice_candidate(Some(Box::new(move |candidate| {
                let _ = found.send((to_receiver, candidate));
            })));
        }
        let mic = engine.microphone(&shared);
        sender
            .add_transceiver(
                MediaStreamTrack::Audio(mic),
                RtpTransceiverInit {
                    direction: RtpTransceiverDirection::SendOnly,
                    stream_ids: vec!["test".into()],
                    send_encodings: vec![],
                },
            )
            .map_err(|e| e.to_string())?;
        let offer = sender
            .create_offer(offer_options())
            .await
            .map_err(|e| e.to_string())?;
        sender
            .set_local_description(offer.clone())
            .await
            .map_err(|e| e.to_string())?;
        receiver
            .set_remote_description(offer)
            .await
            .map_err(|e| e.to_string())?;
        let answer = receiver
            .create_answer(Default::default())
            .await
            .map_err(|e| e.to_string())?;
        receiver
            .set_local_description(answer.clone())
            .await
            .map_err(|e| e.to_string())?;
        sender
            .set_remote_description(answer)
            .await
            .map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let activity = engine::watch_activity(Arc::clone(&shared), Arc::clone(&stop), |_| {});
        Ok(Self {
            sender,
            receiver,
            candidates,
            shared,
            _activity: Joined(Some(activity), stop),
        })
    }

    /// Hands each side the candidates the other found.
    pub async fn exchange(&self) {
        while let Ok((to_receiver, candidate)) = self.candidates.try_recv() {
            let to = if to_receiver { &self.receiver } else { &self.sender };
            if let Err(e) = to.add_ice_candidate(candidate).await {
                log::debug!("mic test candidate: {e}");
            }
        }
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        self.sender.close();
        self.receiver.close();
        engine::detach(&self.shared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: &str = "m=audio 50005 ICE/SDP\na=fingerprint:sha-256 \
4A:79:94:16:44:3F:BD:05:41:5A:C7:20:F3:12:54:70:00:73:5D:33:00:2D:2C:80:9B:39:E1:9F:2D:A7:49:87\n\
c=IN IP4 127.0.0.1\na=rtcp:50005\na=ice-ufrag:PkLE\na=ice-pwd:o9QGn2N6YizFOM/UNojYai\n\
a=candidate:1 1 UDP 4261412862 127.0.0.1 50005 typ host\n";

    /// libwebrtc takes the answer as Discord's web client writes it, and
    /// the person's sound arrives on a stream named after them.
    #[tokio::test]
    #[ignore = "starts the platform audio device"]
    async fn libwebrtc_takes_the_discord_answer() {
        install_transform();
        let shared = Arc::new(Shared::default());
        let pc = engine::engine()
            .factory
            .create_peer_connection(RtcConfiguration::default())
            .unwrap();
        let (peers, mut events) = mpsc::unbounded_channel();
        pc.on_track(Some(Box::new(move |event| {
            let _ = peers.send(Peer::Track(event));
        })));
        let mic = engine::engine().microphone(&shared);
        pc.add_transceiver(
            MediaStreamTrack::Audio(mic),
            RtpTransceiverInit {
                direction: RtpTransceiverDirection::SendRecv,
                stream_ids: vec![],
                send_encodings: vec![RtpEncodingParameters {
                    active: true,
                    max_bitrate: Some(BITRATE),
                    ..Default::default()
                }],
            },
        )
        .unwrap();
        let mut voice = Connection {
            pc,
            context: 0,
            dave: Dave::new(1, 2),
            remote: None,
            local: sdp::Local::default(),
            users: HashMap::new(),
            owners: HashMap::new(),
            unassigned: Vec::new(),
            inactive: Vec::new(),
            assigned: HashMap::new(),
            transceivers: 1,
            nack: false,
            needed: false,
            answered: false,
        };
        let offer = voice.offer().await.unwrap();
        let data = sdp::for_server(&offer);
        println!("SELECT_PROTOCOL data:\n{data}\ncodecs: {}", sdp::codecs(&voice.local));
        assert!(data.contains("a=ice-ufrag"));
        assert!(voice.local.audio_ssrc != 0);
        voice.remote = Some(sdp::parse_remote(SERVER).unwrap());
        voice.create_user(77, 9001);
        voice.negotiate().await.unwrap();
        let description = voice.pc.current_remote_description().expect("answer taken");
        println!("answer:\n{}", description.to_string());

        let Some(Peer::Track(event)) =
            tokio::time::timeout(Duration::from_secs(5), events.recv()).await.unwrap()
        else {
            panic!("no track");
        };
        assert_eq!(event.streams[0].id(), "77-9001");
        // A second person gets their own m-line.
        voice.create_user(78, 9002);
        voice.negotiate().await.unwrap();
        let Some(Peer::Track(event)) =
            tokio::time::timeout(Duration::from_secs(5), events.recv()).await.unwrap()
        else {
            panic!("no second track");
        };
        assert_eq!(event.streams[0].id(), "78-9002");
        voice.pc.close();
    }

    /// The microphone test plays the microphone back and reads its level.
    #[tokio::test]
    #[ignore = "uses the microphone and speakers"]
    async fn the_microphone_test_runs() {
        let shared = Arc::new(Shared::default());
        let test = Loopback::start(Arc::clone(&shared)).await.unwrap();
        for _ in 0..30 {
            test.exchange().await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let state = test.sender.connection_state();
        let level = shared.level();
        println!("loopback {state:?}, microphone at {level:.1} dB");
        assert_eq!(state, PeerConnectionState::Connected);
        assert!(level > -100.0);
    }

    #[test]
    fn backoff_grows_to_its_ceiling_and_starts_over() {
        let mut backoff = Backoff::new(1_000, 5_000);
        let mut last = Duration::ZERO;
        for _ in 0..20 {
            let wait = backoff.fail();
            assert!(wait >= last && wait <= Duration::from_secs(5));
            last = wait;
        }
        assert_eq!(backoff.fails, 20);
        backoff.succeed();
        assert_eq!(backoff.fails, 0);
        assert!(backoff.fail() <= Duration::from_secs(3));
    }
}
