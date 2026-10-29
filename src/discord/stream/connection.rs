//! The share's voice connection: the voice gateway (version 4, as
//! songbird speaks it), IP discovery and the UDP packets, with DAVE kept in
//! step the way songbird's driver keeps it for the call.
//!
//! Not yet tried against Discord. The parts most likely to need a change
//! are marked UNVERIFIED.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::num::NonZeroU16;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::UdpSocket;
use tokio_tungstenite::tungstenite::Message;

use super::decode::Decoder;
use super::rtp::{Assembler, Cipher, Codec, MODES, Pushed};
use super::{Screen, Server};

/// The payload types we offer, and Discord's retransmissions of each.
const H264: u8 = 101;
const H264_RTX: u8 = 102;
const VP8: u8 = 103;
const VP8_RTX: u8 = 104;
/// DAVE's frames end in this.
const MAGIC: [u8; 2] = [0xfa, 0xfa];
/// The least time between two requests for a keyframe.
const PLI_GAP: Duration = Duration::from_secs(1);

pub async fn run(
    server: Server,
    screen: Arc<Screen>,
    wake: impl Fn() + Send + Sync + 'static,
    connected: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    let url = format!("wss://{}/?v=4", server.endpoint);
    let (socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|e| e.to_string())?;
    let (mut sink, mut stream) = socket.split();
    send(
        &mut sink,
        json!({
            "op": 0,
            "d": {
                "server_id": server.rtc_server_id.to_string(),
                "user_id": server.user.to_string(),
                "session_id": server.session_id,
                "token": server.token,
                "video": true,
                "streams": [{ "type": "screen", "rid": "100", "quality": 100 }],
                "max_dave_protocol_version": davey::DAVE_PROTOCOL_VERSION,
            }
        }),
    )
    .await?;

    // Hello and ready, in either order; anything else is kept for later.
    let mut early = VecDeque::new();
    let (mut interval, mut ready) = (None, None);
    while interval.is_none() || ready.is_none() {
        let value = next_json(&mut stream, &mut early).await?;
        match value["op"].as_u64() {
            Some(8) => interval = value["d"]["heartbeat_interval"].as_f64(),
            Some(2) => ready = Some(value["d"].clone()),
            _ => early.push_back(Incoming::Json(value)),
        }
    }
    let ready = ready.unwrap_or_default();
    let ssrc = ready["ssrc"].as_u64().unwrap_or_default() as u32;
    let ip = ready["ip"].as_str().ok_or("voice ready sem ip")?;
    let port = ready["port"].as_u64().ok_or("voice ready sem porta")? as u16;
    let offered: Vec<&str> = ready["modes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mode = MODES
        .into_iter()
        .find(|m| offered.contains(m))
        .ok_or("nenhum modo de criptografia em comum")?;

    let udp = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| e.to_string())?;
    udp.connect((ip, port)).await.map_err(|e| e.to_string())?;
    let (address, external_port) = discover(&udp, ssrc).await?;

    // UNVERIFIED: the codec list as Discord's own client sends it.
    send(
        &mut sink,
        json!({
            "op": 1,
            "d": {
                "protocol": "udp",
                "address": address.to_string(),
                "port": external_port,
                "mode": mode,
                "data": { "address": address.to_string(), "port": external_port, "mode": mode },
                "codecs": [
                    { "name": "opus", "type": "audio", "priority": 1000, "payload_type": 120 },
                    { "name": "H264", "type": "video", "priority": 1000, "payload_type": H264,
                      "rtx_payload_type": H264_RTX, "encode": false, "decode": true },
                    { "name": "VP8", "type": "video", "priority": 2000, "payload_type": VP8,
                      "rtx_payload_type": VP8_RTX, "encode": false, "decode": true },
                ],
            }
        }),
    )
    .await?;

    let mut dave = Dave::new(server.user, server.rtc_server_id);
    let mut video_ssrc: Option<u32> = None;
    let cipher = loop {
        let value = next_json(&mut stream, &mut early).await?;
        if value["op"].as_u64() != Some(4) {
            early.push_back(Incoming::Json(value));
            continue;
        }
        let d = &value["d"];
        let key: Vec<u8> = d["secret_key"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| b.as_u64().map(|b| b as u8))
            .collect();
        let chosen = d["mode"].as_str().unwrap_or(mode);
        let cipher = Cipher::new(chosen, &key).ok_or("chave de transporte inválida")?;
        let version = d["dave_protocol_version"].as_u64().unwrap_or(0) as u16;
        for message in dave.start(version)? {
            sink.send(message).await.map_err(|e| e.to_string())?;
        }
        break cipher;
    };
    connected();
    // UNVERIFIED: asks for the share at full quality, as Discord's client
    // does when it opens a stream.
    send(&mut sink, json!({ "op": 15, "d": { "any": 100 } })).await?;

    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
    let mut video: Option<(Codec, Assembler, Decoder)> = None;
    let mut last_pli: Option<Instant> = None;
    let mut rtcp_counter = 0u32;
    let mut heartbeat =
        tokio::time::interval(Duration::from_millis(interval.unwrap_or(13_750.0) as u64));
    let mut buffer = vec![0u8; 2048];
    loop {
        let incoming = if let Some(kept) = early.pop_front() {
            kept
        } else {
            tokio::select! {
                _ = heartbeat.tick() => {
                    let nonce = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis() as u64);
                    send(&mut sink, json!({ "op": 3, "d": nonce })).await?;
                    continue;
                }
                frame = stream.next() => match frame {
                    Some(Ok(Message::Text(text))) => Incoming::Json(
                        serde_json::from_str(&text).map_err(|e| e.to_string())?,
                    ),
                    Some(Ok(Message::Binary(data))) => Incoming::Binary(data.to_vec()),
                    Some(Ok(Message::Close(frame))) => {
                        return Err(format!(
                            "o servidor da transmissão fechou ({})",
                            frame.map_or(1000, |f| u16::from(f.code))
                        ));
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return Err(e.to_string()),
                    None => return Ok(()),
                },
                received = udp.recv(&mut buffer) => {
                    let size = received.map_err(|e| e.to_string())?;
                    Incoming::Udp(buffer[..size].to_vec())
                }
            }
        };
        match incoming {
            Incoming::Json(value) => {
                let d = &value["d"];
                match value["op"].as_u64() {
                    // Video: who sends which stream. UNVERIFIED shape.
                    Some(12) => {
                        if parse_id(&d["user_id"]) == Some(server.streamer) {
                            let ssrc = d["streams"][0]["ssrc"]
                                .as_u64()
                                .or_else(|| d["video_ssrc"].as_u64())
                                .filter(|s| *s != 0);
                            video_ssrc = ssrc.map(|s| s as u32);
                            log::info!("screen share: video ssrc {video_ssrc:?}");
                        }
                    }
                    Some(11) => {
                        let users = d["user_ids"].as_array().into_iter().flatten();
                        dave.known.extend(users.filter_map(parse_id));
                    }
                    Some(13) => {
                        if let Some(user) = parse_id(&d["user_id"]) {
                            dave.known.remove(&user);
                        }
                    }
                    Some(op) => {
                        for message in dave.json(op, d)? {
                            sink.send(message).await.map_err(|e| e.to_string())?;
                        }
                    }
                    None => {}
                }
            }
            Incoming::Binary(data) => {
                for message in dave.binary(&data)? {
                    sink.send(message).await.map_err(|e| e.to_string())?;
                }
            }
            Incoming::Udp(packet) => {
                let Some(packet) = cipher.open(&packet) else {
                    continue;
                };
                let codec = match packet.payload_type {
                    H264 => Codec::H264,
                    VP8 => Codec::Vp8,
                    _ => continue,
                };
                if video_ssrc.is_some_and(|s| s != packet.ssrc) {
                    continue;
                }
                let media = *video_ssrc.get_or_insert(packet.ssrc);
                if video.as_ref().is_none_or(|(c, _, _)| *c != codec) {
                    let decoder = Decoder::start(codec, Arc::clone(&screen), Arc::clone(&wake))?;
                    video = Some((codec, Assembler::new(codec), decoder));
                    last_pli = None;
                }
                let Some((_, assembler, decoder)) = &mut video else {
                    continue;
                };
                let mut ask = last_pli.is_none();
                match assembler.push(
                    packet.sequence,
                    packet.timestamp,
                    packet.marker,
                    &packet.payload,
                ) {
                    Pushed::Frame(frame) => {
                        if let Some(frame) = dave.open(server.streamer, frame)
                            && !decoder.feed(frame)
                        {
                            return Err("o ffmpeg parou".to_owned());
                        }
                    }
                    Pushed::Lost => ask = true,
                    Pushed::Nothing => {}
                }
                if ask && last_pli.is_none_or(|at| at.elapsed() >= PLI_GAP) {
                    rtcp_counter = rtcp_counter.wrapping_add(1);
                    if let Some(pli) = cipher.picture_loss(ssrc, media, rtcp_counter) {
                        let _ = udp.send(&pli).await;
                    }
                    last_pli = Some(Instant::now());
                }
            }
        }
    }
}

enum Incoming {
    Json(Value),
    Binary(Vec<u8>),
    Udp(Vec<u8>),
}

async fn send<S>(sink: &mut S, value: Value) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    sink.send(Message::Text(value.to_string().into()))
        .await
        .map_err(|e| e.to_string())
}

/// The next JSON message during the handshake; binary DAVE messages are
/// kept for the main loop.
async fn next_json<S>(stream: &mut S, early: &mut VecDeque<Incoming>) -> Result<Value, String>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(&text).map_err(|e| e.to_string());
            }
            Some(Ok(Message::Binary(data))) => early.push_back(Incoming::Binary(data.to_vec())),
            Some(Ok(Message::Close(frame))) => {
                return Err(format!(
                    "o servidor da transmissão recusou ({})",
                    frame.map_or(1000, |f| u16::from(f.code))
                ));
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e.to_string()),
            None => return Err("o servidor da transmissão fechou".to_owned()),
        }
    }
}

fn parse_id(value: &Value) -> Option<u64> {
    crate::discord::parse::id(value)
}

/// Discord's IP discovery: our address and port as the server sees them.
async fn discover(udp: &UdpSocket, ssrc: u32) -> Result<(IpAddr, u16), String> {
    let mut request = [0u8; 74];
    request[..2].copy_from_slice(&1u16.to_be_bytes());
    request[2..4].copy_from_slice(&70u16.to_be_bytes());
    request[4..8].copy_from_slice(&ssrc.to_be_bytes());
    udp.send(&request).await.map_err(|e| e.to_string())?;
    let mut response = [0u8; 74];
    let size = tokio::time::timeout(Duration::from_secs(5), udp.recv(&mut response))
        .await
        .map_err(|_| "a descoberta de IP não respondeu".to_owned())?
        .map_err(|e| e.to_string())?;
    parse_discovery(&response[..size]).ok_or_else(|| "resposta de IP inválida".to_owned())
}

fn parse_discovery(response: &[u8]) -> Option<(IpAddr, u16)> {
    if response.len() < 74 || response[..2] != 2u16.to_be_bytes() {
        return None;
    }
    let raw = &response[8..72];
    let end = raw.iter().position(|b| *b == 0)?;
    let address = std::str::from_utf8(&raw[..end]).ok()?.parse().ok()?;
    Some((address, u16::from_be_bytes([response[72], response[73]])))
}

/// DAVE, kept in step with the voice server as songbird's driver does.
struct Dave {
    session: Option<davey::DaveSession>,
    version: u16,
    pending: HashMap<u16, u16>,
    /// Who the server said is connected, to check proposals against.
    known: HashSet<u64>,
    user: u64,
    /// UNVERIFIED: the MLS group of a share is assumed to be named by its
    /// voice server id; the call's channel id is the other candidate.
    group: u64,
}

impl Dave {
    fn new(user: u64, group: u64) -> Self {
        Self {
            session: None,
            version: 0,
            pending: HashMap::new(),
            known: HashSet::new(),
            user,
            group,
        }
    }

    /// From the session description: a session and its key package, when
    /// the share is end-to-end encrypted.
    fn start(&mut self, version: u16) -> Result<Vec<Message>, String> {
        self.version = version;
        let Some(version) = NonZeroU16::new(version) else {
            return Ok(Vec::new());
        };
        let mut session = davey::DaveSession::new(version, self.user, self.group, None)
            .map_err(|e| format!("DAVE: {e:?}"))?;
        let package = session
            .create_key_package()
            .map_err(|e| format!("DAVE: {e:?}"))?;
        self.session = Some(session);
        Ok(vec![binary(26, &package)])
    }

    fn reinit(&mut self) -> Vec<Message> {
        let Some(version) = NonZeroU16::new(self.version) else {
            if let Some(session) = &mut self.session {
                let _ = session.reset();
                session.set_passthrough_mode(true, Some(10));
            }
            return Vec::new();
        };
        let (user, group) = (self.user, self.group);
        let session = match self.session.take() {
            Some(mut session) => session
                .reinit(version, user, group, None)
                .map(|()| session)
                .map_err(|e| format!("{e:?}")),
            None => {
                davey::DaveSession::new(version, user, group, None).map_err(|e| format!("{e:?}"))
            }
        };
        let package = session.and_then(|mut session| {
            let package = session.create_key_package().map_err(|e| format!("{e:?}"));
            self.session = Some(session);
            package
        });
        match package {
            Ok(package) => vec![binary(26, &package)],
            Err(e) => {
                log::warn!("screen share DAVE reinit: {e}");
                Vec::new()
            }
        }
    }

    fn execute(&mut self, transition: u16) {
        let Some(new) = self.pending.remove(&transition) else {
            return;
        };
        let old = std::mem::replace(&mut self.version, new);
        if transition > 0
            && old == 0
            && new != 0
            && let Some(session) = &mut self.session
        {
            session.set_passthrough_mode(true, Some(10));
        }
    }

    fn ready(transition: u16, version: u16) -> Message {
        let value = json!({
            "op": 23,
            "d": { "transition_id": transition, "protocol_version": version }
        });
        Message::Text(value.to_string().into())
    }

    /// The JSON DAVE operations: 21, 22 and 24.
    fn json(&mut self, op: u64, d: &Value) -> Result<Vec<Message>, String> {
        let number = |key: &str| d[key].as_u64().unwrap_or(0) as u16;
        match op {
            21 => {
                let (transition, version) = (number("transition_id"), number("protocol_version"));
                self.pending.insert(transition, version);
                if transition == 0 {
                    self.execute(0);
                } else if version == 0 {
                    if let Some(session) = &mut self.session {
                        session.set_passthrough_mode(true, Some(120));
                    }
                    return Ok(vec![Self::ready(transition, version)]);
                }
            }
            22 => self.execute(number("transition_id")),
            24 if d["epoch"].as_u64() == Some(1) => {
                self.version = number("protocol_version");
                return Ok(self.reinit());
            }
            _ => {}
        }
        Ok(Vec::new())
    }

    /// The binary DAVE operations: 25, 27, 29 and 30.
    fn binary(&mut self, data: &[u8]) -> Result<Vec<Message>, String> {
        let Some((&op, rest)) = data.split_first() else {
            return Ok(Vec::new());
        };
        let transition =
            || (rest.len() >= 2).then(|| (u16::from_be_bytes([rest[0], rest[1]]), &rest[2..]));
        match op {
            25 => {
                if let Some(session) = &mut self.session
                    && let Err(e) = session.set_external_sender(rest)
                {
                    log::warn!("screen share DAVE external sender: {e:?}");
                }
            }
            27 => {
                let Some((&kind, proposals)) = rest.split_first() else {
                    return Ok(Vec::new());
                };
                let kind = if kind == 1 {
                    davey::ProposalsOperationType::REVOKE
                } else {
                    davey::ProposalsOperationType::APPEND
                };
                let known: Vec<u64> = self.known.iter().copied().collect();
                if let Some(session) = &mut self.session {
                    match session.process_proposals(kind, proposals, Some(&known)) {
                        Ok(Some(cw)) => {
                            let mut body = cw.commit;
                            body.extend(cw.welcome.unwrap_or_default());
                            return Ok(vec![binary(28, &body)]);
                        }
                        Ok(None) => {}
                        Err(e) => log::warn!("screen share DAVE proposals: {e:?}"),
                    }
                }
            }
            29 | 30 => {
                let Some((id, body)) = transition() else {
                    return Ok(Vec::new());
                };
                let Some(session) = &mut self.session else {
                    return Ok(Vec::new());
                };
                let result = if op == 29 {
                    session.process_commit(body).map_err(|e| format!("{e:?}"))
                } else {
                    session.process_welcome(body).map_err(|e| format!("{e:?}"))
                };
                match result {
                    Ok(()) if id != 0 => {
                        self.pending.insert(id, self.version);
                        return Ok(vec![Self::ready(id, self.version)]);
                    }
                    Ok(()) => {}
                    Err(e) => {
                        log::warn!("screen share DAVE {op}: {e}");
                        let invalid = json!({ "op": 31, "d": { "transition_id": id } });
                        let mut out = vec![Message::Text(invalid.to_string().into())];
                        out.extend(self.reinit());
                        return Ok(out);
                    }
                }
            }
            _ => {}
        }
        Ok(Vec::new())
    }

    /// A whole frame without its end-to-end layer. `None` drops it: it is
    /// encrypted and there is no key for it yet.
    fn open(&mut self, user: u64, frame: Vec<u8>) -> Option<Vec<u8>> {
        if self.version == 0 || frame.len() < 11 || !frame.ends_with(&MAGIC) {
            return Some(frame);
        }
        let session = self.session.as_mut().filter(|s| s.is_ready())?;
        match session.decrypt(user, davey::MediaType::VIDEO, &frame) {
            Ok(frame) => Some(frame),
            Err(e) => {
                log::debug!("screen share DAVE decrypt: {e:?}");
                None
            }
        }
    }
}

fn binary(op: u8, body: &[u8]) -> Message {
    let mut data = Vec::with_capacity(1 + body.len());
    data.push(op);
    data.extend_from_slice(body);
    Message::Binary(data.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ip_discovery_answers_are_read() {
        let mut response = [0u8; 74];
        response[..2].copy_from_slice(&2u16.to_be_bytes());
        response[8..8 + 11].copy_from_slice(b"203.0.113.9");
        response[72..].copy_from_slice(&50_000u16.to_be_bytes());
        let (address, port) = parse_discovery(&response).unwrap();
        assert_eq!(address.to_string(), "203.0.113.9");
        assert_eq!(port, 50_000);
        response[1] = 1;
        assert!(parse_discovery(&response).is_none());
    }

    #[test]
    fn frames_without_the_dave_marker_pass_through() {
        let mut dave = Dave::new(1, 2);
        assert_eq!(dave.open(3, vec![1, 2, 3]), Some(vec![1, 2, 3]));
        dave.version = 1;
        let marked = [vec![0; 12], MAGIC.to_vec()].concat();
        // Encrypted, and no session yet: dropped.
        assert_eq!(dave.open(3, marked), None);
    }

    #[test]
    fn a_dave_session_starts_with_a_key_package() {
        let mut dave = Dave::new(1, 2);
        let messages = dave.start(1).unwrap();
        assert!(matches!(&messages[..], [Message::Binary(b)] if b[0] == 26));
        assert!(dave.start(0).unwrap().is_empty());
    }
}
