//! The session descriptions, as Discord's web client writes them: the
//! offer libwebrtc makes is cut down to what the voice server reads, and
//! the server's short answer is grown into a whole one, with an m-line
//! per person heard. Ported from the client's own code (its "SDP" module:
//! `eD`, `ey`, `eb`, `eC`, `eR` and `eO`).

use std::collections::HashSet;

/// Discord's media is always this.
const PROTOCOL: &str = "UDP/TLS/RTP/SAVPF";
const AUDIO_LEVEL: &str = "urn:ietf:params:rtp-hdrext:ssrc-audio-level";
const TRANSPORT_CC: &str = "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    SendRecv,
    SendOnly,
    RecvOnly,
    Inactive,
}

impl Direction {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "sendrecv" => Self::SendRecv,
            "sendonly" => Self::SendOnly,
            "recvonly" => Self::RecvOnly,
            "inactive" => Self::Inactive,
            _ => return None,
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::SendRecv => "sendrecv",
            Self::SendOnly => "sendonly",
            Self::RecvOnly => "recvonly",
            Self::Inactive => "inactive",
        }
    }

    /// The other side's direction (`eN`).
    pub fn reversed(self) -> Self {
        match self {
            Self::RecvOnly => Self::SendOnly,
            Self::SendOnly => Self::RecvOnly,
            Self::SendRecv => Self::SendRecv,
            Self::Inactive => Self::Inactive,
        }
    }
}

/// One m-line of the local offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outbound {
    pub kind: String,
    pub mid: String,
    pub direction: Direction,
}

/// What the client takes from its own offer (`ey` and `eb`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Local {
    pub outbound: Vec<Outbound>,
    /// Opus's payload type.
    pub opus: Option<u8>,
    /// The ssrc the microphone is sent on.
    pub audio_ssrc: u32,
    /// Every header extension: its id and uri.
    pub extensions: Vec<(u8, String)>,
}

pub fn parse_local(sdp: &str) -> Local {
    let mut local = Local::default();
    let mut seen = HashSet::new();
    let mut current: Option<Outbound> = None;
    let mut cname: Option<u32> = None;
    let finish = |current: &mut Option<Outbound>, cname: &mut Option<u32>, local: &mut Local| {
        if let Some(section) = current.take() {
            if section.kind == "audio"
                && section.direction == Direction::SendRecv
                && let Some(ssrc) = cname.take()
            {
                local.audio_ssrc = ssrc;
            }
            local.outbound.push(section);
        }
        *cname = None;
    };
    for line in sdp.split("\r\n").flat_map(|l| l.split('\n')) {
        if let Some(rest) = line.strip_prefix("m=") {
            finish(&mut current, &mut cname, &mut local);
            current = Some(Outbound {
                kind: rest.split(' ').next().unwrap_or_default().to_owned(),
                mid: String::new(),
                direction: Direction::SendRecv,
            });
        } else if let Some(rest) = line.strip_prefix("a=extmap:") {
            if seen.insert(line.to_owned()) {
                let mut parts = rest.split(' ');
                let id = parts.next().and_then(|i| i.split('/').next()?.parse().ok());
                if let (Some(id), Some(uri)) = (id, parts.next()) {
                    local.extensions.push((id, uri.to_owned()));
                }
            }
        } else if let Some(section) = &mut current {
            if let Some(mid) = line.strip_prefix("a=mid:") {
                section.mid = mid.to_owned();
            } else if let Some(direction) = line.strip_prefix("a=").and_then(Direction::parse) {
                section.direction = direction;
            } else if let Some(rest) = line.strip_prefix("a=rtpmap:") {
                let (pt, codec) = rest.split_once(' ').unwrap_or_default();
                if section.kind == "audio"
                    && codec.to_ascii_lowercase().starts_with("opus/")
                    && local.opus.is_none()
                {
                    local.opus = pt.parse().ok();
                }
            } else if let Some(rest) = line.strip_prefix("a=ssrc:") {
                let (ssrc, attribute) = rest.split_once(' ').unwrap_or_default();
                if attribute.starts_with("cname:") && cname.is_none() {
                    cname = ssrc.parse().ok();
                }
            }
        }
    }
    finish(&mut current, &mut cname, &mut local);
    local
}

/// What goes to the voice server in SELECT_PROTOCOL (`eD`): the offer's
/// ICE, fingerprint, header extensions and codec lines, each once.
pub fn for_server(sdp: &str) -> String {
    let mut seen = HashSet::new();
    sdp.split("\r\n")
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.starts_with("a=ice")
                || lower.contains("a=extmap")
                || lower.contains("a=fingerprint")
                || lower.contains("opus")
                || lower.contains("vp8")
                || lower.contains("0 rtx")
        })
        .filter(|line| seen.insert(*line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The codecs SELECT_PROTOCOL lists (`ey` then `e$`): Opus alone.
pub fn codecs(local: &Local) -> serde_json::Value {
    match local.opus {
        Some(pt) => serde_json::json!([{
            "name": "opus",
            "type": "audio",
            "priority": 1000,
            "payload_type": pt,
            "rtx_payload_type": null,
        }]),
        None => serde_json::json!([]),
    }
}

/// The parts of the server's SDP every m-line of the answer repeats.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remote {
    pub port: String,
    pub connection: String,
    pub rtcp: Option<String>,
    pub ufrag: String,
    pub pwd: String,
    pub fingerprint: String,
    pub candidates: Vec<String>,
}

/// The server's SDP, checked as the client checks it (`ev`).
pub fn parse_remote(sdp: &str) -> Result<Remote, String> {
    let mut remote = Remote::default();
    for line in sdp.split("\r\n").flat_map(|l| l.split('\n')) {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("m=") {
            remote.port = rest.split(' ').nth(1).unwrap_or_default().to_owned();
        } else if let Some(rest) = line.strip_prefix("c=") {
            remote.connection = rest.to_owned();
        } else if let Some(rest) = line.strip_prefix("a=rtcp:") {
            remote.rtcp = Some(rest.to_owned());
        } else if let Some(rest) = line.strip_prefix("a=ice-ufrag:") {
            remote.ufrag = rest.to_owned();
        } else if let Some(rest) = line.strip_prefix("a=ice-pwd:") {
            remote.pwd = rest.to_owned();
        } else if let Some(rest) = line.strip_prefix("a=fingerprint:") {
            remote.fingerprint = rest.to_owned();
        } else if let Some(rest) = line.strip_prefix("a=candidate:") {
            remote.candidates.push(rest.to_owned());
        }
    }
    if remote.fingerprint.is_empty() {
        return Err("Remote SDP does not include fingerprint!".into());
    }
    if remote.ufrag.is_empty() {
        return Err("Remote SDP does not include ICE user name!".into());
    }
    if remote.pwd.is_empty() {
        return Err("Remote SDP does not include ICE password!".into());
    }
    if remote.candidates.is_empty() {
        return Err("Remote SDP does not include ICE candidate!".into());
    }
    if remote.connection.split(' ').count() < 3 {
        return Err(format!("Incorrect c-line: c={}", remote.connection));
    }
    if remote.port.is_empty() {
        remote.port = remote
            .candidates
            .first()
            .and_then(|c| c.split(' ').nth(5))
            .unwrap_or("9")
            .to_owned();
    }
    Ok(remote)
}

/// One m-line of the answer: whose sound it carries, if anyone's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub mid: String,
    pub direction: Direction,
    /// The person and the ssrc they send on.
    pub stream: Option<(u64, u32)>,
}

/// The whole answer (`eR` for each m-line, then `eO`).
pub fn answer(
    remote: &Remote,
    sections: &[Section],
    opus: u8,
    extensions: &[(u8, String)],
    nack: bool,
) -> String {
    let mids: Vec<&str> = sections.iter().map(|s| s.mid.as_str()).collect();
    let mut out = String::new();
    let mut line = |text: String| {
        out.push_str(&text);
        out.push_str("\r\n");
    };
    line("v=0".into());
    line("o=- 1420070400000 0 IN IP4 127.0.0.1".into());
    line("s=-".into());
    line("t=0 0".into());
    line(format!("a=group:BUNDLE {}", mids.join(" ")));
    line("a=msid-semantic: WMS *".into());
    for section in sections {
        // An m-line with nobody's sound: sendonly becomes inactive and
        // sendrecv becomes recvonly.
        let direction = match (section.stream, section.direction) {
            (None, Direction::SendOnly) => Direction::Inactive,
            (None, Direction::SendRecv) => Direction::RecvOnly,
            (_, direction) => direction,
        };
        line(format!("m=audio {} {PROTOCOL} {opus}", remote.port));
        line(format!("c={}", remote.connection));
        line(format!("a=rtpmap:{opus} opus/48000/2"));
        line(format!("a=fmtp:{opus} minptime=10;useinbandfec=1;usedtx=1"));
        if let Some(rtcp) = &remote.rtcp {
            line(format!("a=rtcp:{rtcp}"));
        }
        line(format!("a=rtcp-fb:{opus} transport-cc"));
        if nack {
            line(format!("a=rtcp-fb:{opus} nack"));
        }
        for (id, uri) in extensions {
            if uri == AUDIO_LEVEL || uri == TRANSPORT_CC {
                line(format!("a=extmap:{id} {uri}"));
            }
        }
        line("a=setup:passive".into());
        line(format!("a=mid:{}", section.mid));
        if let Some((user, ssrc)) = section.stream {
            line(format!("a=msid:{user}-{ssrc} a{user}-{ssrc}"));
        }
        line("a=maxptime:60".into());
        line(format!("a={}", direction.as_str()));
        line(format!("a=ice-ufrag:{}", remote.ufrag));
        line(format!("a=ice-pwd:{}", remote.pwd));
        line(format!("a=fingerprint:{}", remote.fingerprint));
        for candidate in &remote.candidates {
            line(format!("a=candidate:{candidate}"));
        }
        if let Some((user, ssrc)) = section.stream {
            line(format!("a=ssrc:{ssrc} cname:{user}-{ssrc}"));
        }
        line("a=rtcp-mux".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const OFFER: &str = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=group:BUNDLE 0 1\r\n\
a=extmap-allow-mixed\r\na=msid-semantic: WMS\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111 63 9\r\n\
c=IN IP4 0.0.0.0\r\na=rtcp:9 IN IP4 0.0.0.0\r\na=ice-ufrag:abcd\r\na=ice-pwd:secretsecretsecret\r\n\
a=ice-options:trickle\r\na=fingerprint:sha-256 AA:BB\r\na=setup:actpass\r\na=mid:0\r\n\
a=extmap:1 urn:ietf:params:rtp-hdrext:ssrc-audio-level\r\n\
a=extmap:3 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01\r\n\
a=sendrecv\r\na=msid:- track\r\na=rtcp-mux\r\na=rtpmap:111 opus/48000/2\r\n\
a=rtcp-fb:111 transport-cc\r\na=fmtp:111 minptime=10;useinbandfec=1\r\na=rtpmap:9 G722/8000\r\n\
a=ssrc:4242 cname:xyz\r\na=ssrc:4242 msid:- track\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\n\
c=IN IP4 0.0.0.0\r\na=mid:1\r\na=extmap:1 urn:ietf:params:rtp-hdrext:ssrc-audio-level\r\n\
a=recvonly\r\na=rtpmap:111 opus/48000/2\r\n";

    const SERVER: &str = "m=audio 50005 ICE/SDP\na=fingerprint:sha-256 4A:79\nc=IN IP4 66.22.231.190\n\
a=rtcp:50005\na=ice-ufrag:PkLE\na=ice-pwd:o9QGn2N6YizFOM\na=fingerprint:sha-256 4A:79\n\
a=candidate:1 1 UDP 4261412862 66.22.231.190 50005 typ host\n";

    #[test]
    fn the_offer_is_read_as_the_client_reads_it() {
        let local = parse_local(OFFER);
        assert_eq!(local.opus, Some(111));
        assert_eq!(local.audio_ssrc, 4242);
        assert_eq!(local.outbound.len(), 2);
        assert_eq!(local.outbound[1].mid, "1");
        assert_eq!(local.outbound[1].direction, Direction::RecvOnly);
        assert_eq!(local.extensions.len(), 2);
    }

    #[test]
    fn the_server_gets_only_ice_fingerprint_extensions_and_opus() {
        let sent = for_server(OFFER);
        assert!(sent.contains("a=ice-ufrag:abcd"));
        assert!(sent.contains("a=fingerprint:sha-256 AA:BB"));
        assert!(sent.contains("a=rtpmap:111 opus/48000/2"));
        assert!(!sent.contains("G722"));
        assert!(!sent.contains("a=mid"));
        // Each line once, joined by bare newlines.
        assert_eq!(sent.matches("ssrc-audio-level").count(), 1);
        assert!(!sent.contains('\r'));
    }

    #[test]
    fn the_answer_carries_each_person_on_their_own_m_line() {
        let remote = parse_remote(SERVER).unwrap();
        assert_eq!(remote.port, "50005");
        let local = parse_local(OFFER);
        let sections = vec![
            Section { mid: "0".into(), direction: Direction::SendRecv, stream: None },
            Section { mid: "1".into(), direction: Direction::SendOnly, stream: Some((77, 9001)) },
        ];
        let sdp = answer(&remote, &sections, 111, &local.extensions, false);
        assert!(sdp.contains("a=group:BUNDLE 0 1"));
        assert!(sdp.contains("a=mid:0\r\na=maxptime:60\r\na=recvonly"));
        assert!(sdp.contains("a=msid:77-9001 a77-9001"));
        assert!(sdp.contains("a=ssrc:9001 cname:77-9001"));
        assert!(sdp.contains("a=sendonly"));
        assert!(sdp.contains("usedtx=1"));
        assert_eq!(sdp.matches("a=candidate:").count(), 2);
    }

    #[test]
    fn a_server_sdp_without_ice_is_refused() {
        assert!(parse_remote("c=IN IP4 1.2.3.4\na=fingerprint:x").is_err());
    }
}
