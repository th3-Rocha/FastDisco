//! The share's packets: RTP opened with the call's transport key, and the
//! video put back into whole frames, H264 in Annex B (as DAVE and ffmpeg
//! read it) or VP8.

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::XChaCha20Poly1305;

/// The AEAD tag after the payload.
const TAG: usize = 16;
/// The nonce counter at the very end of each packet.
const NONCE: usize = 4;
/// What Annex B puts before each unit. Four bytes, as WebRTC writes them,
/// because DAVE's clear ranges are counted on the sender's frame.
const START: [u8; 4] = [0, 0, 0, 1];

/// The transport encryption the voice server chose ("rtpsize" modes).
pub enum Cipher {
    Aes(Box<Aes256Gcm>),
    XChaCha(XChaCha20Poly1305),
}

/// The modes we offer, best first.
pub const MODES: [&str; 2] = ["aead_aes256_gcm_rtpsize", "aead_xchacha20_poly1305_rtpsize"];

/// One opened RTP packet.
#[derive(Debug, PartialEq, Eq)]
pub struct Packet {
    pub payload_type: u8,
    pub marker: bool,
    pub sequence: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub payload: Vec<u8>,
}

impl Cipher {
    pub fn new(mode: &str, key: &[u8]) -> Option<Self> {
        match mode {
            "aead_aes256_gcm_rtpsize" => Aes256Gcm::new_from_slice(key)
                .ok()
                .map(|c| Self::Aes(Box::new(c))),
            "aead_xchacha20_poly1305_rtpsize" => XChaCha20Poly1305::new_from_slice(key)
                .ok()
                .map(Self::XChaCha),
            _ => None,
        }
    }

    fn open_in_place(&self, nonce: &[u8], aad: &[u8], body: &mut [u8], tag: &[u8]) -> bool {
        match self {
            Self::Aes(c) => {
                let mut n = aes_gcm::Nonce::default();
                n[..NONCE].copy_from_slice(nonce);
                c.decrypt_in_place_detached(&n, aad, body, tag.into())
                    .is_ok()
            }
            Self::XChaCha(c) => {
                let mut n = chacha20poly1305::XNonce::default();
                n[..NONCE].copy_from_slice(nonce);
                c.decrypt_in_place_detached(&n, aad, body, tag.into())
                    .is_ok()
            }
        }
    }

    fn seal_in_place(&self, nonce: &[u8], aad: &[u8], body: &mut [u8]) -> Option<Vec<u8>> {
        match self {
            Self::Aes(c) => {
                let mut n = aes_gcm::Nonce::default();
                n[..NONCE].copy_from_slice(nonce);
                c.encrypt_in_place_detached(&n, aad, body)
                    .ok()
                    .map(|t| t.to_vec())
            }
            Self::XChaCha(c) => {
                let mut n = chacha20poly1305::XNonce::default();
                n[..NONCE].copy_from_slice(nonce);
                c.encrypt_in_place_detached(&n, aad, body)
                    .ok()
                    .map(|t| t.to_vec())
            }
        }
    }

    /// Opens an RTP packet. Discord encrypts from after the extension
    /// preamble, so the extension values come out of the plaintext.
    /// RTCP and anything that does not open is `None`.
    pub fn open(&self, packet: &[u8]) -> Option<Packet> {
        if packet.len() < 12 || packet[0] >> 6 != 2 || is_rtcp(packet) {
            return None;
        }
        let csrcs = usize::from(packet[0] & 0x0f);
        let extension = packet[0] & 0x10 != 0;
        let padding = packet[0] & 0x20 != 0;
        let clear = 12 + 4 * csrcs + if extension { 4 } else { 0 };
        if packet.len() < clear + TAG + NONCE {
            return None;
        }
        let (aad, rest) = packet.split_at(clear);
        let (sealed, nonce) = rest.split_at(rest.len() - NONCE);
        let (body, tag) = sealed.split_at(sealed.len() - TAG);
        let mut body = body.to_vec();
        if !self.open_in_place(nonce, aad, &mut body, tag) {
            return None;
        }
        let mut start = 0;
        if extension {
            let words = usize::from(u16::from_be_bytes([aad[clear - 2], aad[clear - 1]]));
            start = words * 4;
        }
        let mut end = body.len();
        if padding {
            end = end.checked_sub(usize::from(*body.last()?))?;
        }
        if start > end {
            return None;
        }
        Some(Packet {
            payload_type: packet[1] & 0x7f,
            marker: packet[1] & 0x80 != 0,
            sequence: u16::from_be_bytes([packet[2], packet[3]]),
            timestamp: u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]),
            ssrc: u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]),
            payload: body[start..end].to_vec(),
        })
    }

    /// A Picture Loss Indication (RFC 4585) for `media`, sealed as Discord
    /// seals RTCP: the eight header bytes clear, then tag and nonce.
    pub fn picture_loss(&self, ours: u32, media: u32, counter: u32) -> Option<Vec<u8>> {
        let mut packet = vec![0x81, 206, 0, 2];
        packet.extend_from_slice(&ours.to_be_bytes());
        let mut body = media.to_be_bytes().to_vec();
        let nonce = counter.to_be_bytes();
        let tag = self.seal_in_place(&nonce, &packet, &mut body)?;
        packet.extend_from_slice(&body);
        packet.extend_from_slice(&tag);
        packet.extend_from_slice(&nonce);
        Some(packet)
    }
}

/// RTCP shares the port; its second byte is a packet type of 192 to 223.
fn is_rtcp(packet: &[u8]) -> bool {
    (192..=223).contains(&packet[1])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264,
    Vp8,
}

/// What a packet did to the frame being put together.
#[derive(Debug, PartialEq, Eq)]
pub enum Pushed {
    /// The frame is whole.
    Frame(Vec<u8>),
    /// A packet went missing: this frame is lost, ask for a keyframe.
    Lost,
    Nothing,
}

/// Puts one sender's video packets back into frames. Frames end on the
/// marker bit; a gap in the sequence spoils the frame it falls in.
pub struct Assembler {
    codec: Codec,
    frame: Vec<u8>,
    timestamp: Option<u32>,
    next: Option<u16>,
    broken: bool,
}

impl Assembler {
    pub fn new(codec: Codec) -> Self {
        Self {
            codec,
            frame: Vec::new(),
            timestamp: None,
            next: None,
            broken: false,
        }
    }

    pub fn push(&mut self, sequence: u16, timestamp: u32, marker: bool, payload: &[u8]) -> Pushed {
        let gap = match self.next {
            // Late or repeated packets are dropped.
            Some(next) if sequence.wrapping_sub(next) > u16::MAX / 2 => return Pushed::Nothing,
            Some(next) => sequence != next,
            None => false,
        };
        self.next = Some(sequence.wrapping_add(1));
        let mut lost = false;
        if self.timestamp != Some(timestamp) {
            // The last frame never got its marker.
            lost = !self.frame.is_empty() && !self.broken;
            self.frame.clear();
            self.timestamp = Some(timestamp);
            self.broken = false;
        }
        let before = self.broken;
        self.broken |= gap;
        if !self.broken {
            self.broken = !match self.codec {
                Codec::H264 => h264(payload, &mut self.frame),
                Codec::Vp8 => vp8(payload, &mut self.frame),
            };
        }
        lost |= self.broken && !before;
        if marker {
            let whole = !self.broken && !self.frame.is_empty();
            let frame = std::mem::take(&mut self.frame);
            self.timestamp = None;
            self.broken = false;
            if whole {
                return Pushed::Frame(frame);
            }
        }
        if lost { Pushed::Lost } else { Pushed::Nothing }
    }
}

/// RFC 6184: single units, STAP-A aggregates and FU-A fragments.
fn h264(payload: &[u8], frame: &mut Vec<u8>) -> bool {
    let Some(&first) = payload.first() else {
        return false;
    };
    match first & 0x1f {
        1..=23 => {
            frame.extend_from_slice(&START);
            frame.extend_from_slice(payload);
            true
        }
        24 => {
            let mut rest = &payload[1..];
            while rest.len() >= 2 {
                let size = usize::from(u16::from_be_bytes([rest[0], rest[1]]));
                let Some(unit) = rest.get(2..2 + size) else {
                    return false;
                };
                frame.extend_from_slice(&START);
                frame.extend_from_slice(unit);
                rest = &rest[2 + size..];
            }
            rest.is_empty()
        }
        28 => {
            let Some(&header) = payload.get(1) else {
                return false;
            };
            if header & 0x80 != 0 {
                frame.extend_from_slice(&START);
                frame.push((first & 0xe0) | (header & 0x1f));
            } else if frame.is_empty() {
                // The start of this unit was lost.
                return false;
            }
            frame.extend_from_slice(&payload[2..]);
            true
        }
        _ => false,
    }
}

/// RFC 7741: the payload descriptor, then the frame's bytes.
fn vp8(payload: &[u8], frame: &mut Vec<u8>) -> bool {
    let Some(&first) = payload.first() else {
        return false;
    };
    let mut at = 1;
    if first & 0x80 != 0 {
        let Some(&extended) = payload.get(1) else {
            return false;
        };
        at = 2;
        if extended & 0x80 != 0 {
            let Some(&picture) = payload.get(at) else {
                return false;
            };
            at += if picture & 0x80 != 0 { 2 } else { 1 };
        }
        if extended & 0x40 != 0 {
            at += 1;
        }
        if extended & 0x30 != 0 {
            at += 1;
        }
    }
    let start = first & 0x10 != 0 && first & 0x07 == 0;
    if !start && frame.is_empty() {
        return false;
    }
    match payload.get(at..) {
        Some(bytes) => {
            frame.extend_from_slice(bytes);
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];

    /// Seals a packet the way the voice server does.
    fn seal(cipher: &Cipher, header: &[u8], body: &[u8], counter: u32) -> Vec<u8> {
        let mut body = body.to_vec();
        let nonce = counter.to_be_bytes();
        let tag = cipher.seal_in_place(&nonce, header, &mut body).unwrap();
        [header, &body, &tag, &nonce].concat()
    }

    fn header(extension: bool, marker: bool, sequence: u16) -> Vec<u8> {
        let mut h = vec![
            if extension { 0x90 } else { 0x80 },
            101 | if marker { 0x80 } else { 0 },
        ];
        h.extend_from_slice(&sequence.to_be_bytes());
        h.extend_from_slice(&9000u32.to_be_bytes());
        h.extend_from_slice(&0x1234u32.to_be_bytes());
        if extension {
            // The preamble: profile 0xBEDE and one word of values.
            h.extend_from_slice(&[0xbe, 0xde, 0, 1]);
        }
        h
    }

    #[test]
    fn both_modes_open_what_they_sealed_and_skip_the_extension() {
        for mode in MODES {
            let cipher = Cipher::new(mode, &KEY).unwrap();
            let packet = seal(
                &cipher,
                &header(true, true, 5),
                &[1, 2, 3, 4, 0xaa, 0xbb],
                1,
            );
            let opened = cipher.open(&packet).unwrap();
            assert_eq!(opened.payload, vec![0xaa, 0xbb], "{mode}");
            assert_eq!(opened.payload_type, 101);
            assert!(opened.marker);
            assert_eq!(opened.sequence, 5);
            assert_eq!(opened.ssrc, 0x1234);
            let mut spoiled = packet.clone();
            spoiled[14] ^= 1;
            assert!(cipher.open(&spoiled).is_none());
        }
    }

    #[test]
    fn rtcp_is_not_taken_for_video() {
        let cipher = Cipher::new(MODES[0], &KEY).unwrap();
        let pli = cipher.picture_loss(1, 2, 3).unwrap();
        assert_eq!(pli[1], 206);
        assert!(cipher.open(&pli).is_none());
    }

    #[test]
    fn h264_fragments_and_aggregates_become_annex_b() {
        let mut a = Assembler::new(Codec::H264);
        // STAP-A with an SPS (7) and a PPS (8).
        let stap = [24, 0, 2, 0x67, 1, 0, 2, 0x68, 2];
        assert_eq!(a.push(1, 10, false, &stap), Pushed::Nothing);
        // An IDR slice (5) in three fragments.
        assert_eq!(a.push(2, 10, false, &[0x7c, 0x85, 0xa]), Pushed::Nothing);
        assert_eq!(a.push(3, 10, false, &[0x7c, 0x05, 0xb]), Pushed::Nothing);
        let Pushed::Frame(frame) = a.push(4, 10, true, &[0x7c, 0x45, 0xc]) else {
            panic!("the frame is whole");
        };
        assert_eq!(
            frame,
            [
                &START[..],
                &[0x67, 1],
                &START,
                &[0x68, 2],
                &START,
                &[0x65, 0xa, 0xb, 0xc]
            ]
            .concat()
        );
    }

    #[test]
    fn a_gap_loses_the_frame_and_the_next_one_comes_through() {
        let mut a = Assembler::new(Codec::H264);
        assert_eq!(a.push(1, 10, false, &[0x7c, 0x85, 1]), Pushed::Nothing);
        assert_eq!(a.push(3, 10, true, &[0x7c, 0x45, 3]), Pushed::Lost);
        assert!(matches!(a.push(4, 20, true, &[0x65, 4]), Pushed::Frame(_)));
    }

    #[test]
    fn vp8_descriptors_are_stripped() {
        let mut a = Assembler::new(Codec::Vp8);
        // X and S set, then I with a two-byte picture id.
        assert_eq!(
            a.push(1, 10, false, &[0x90, 0x80, 0x81, 0x02, 0xaa]),
            Pushed::Nothing
        );
        let Pushed::Frame(frame) = a.push(2, 10, true, &[0x80, 0x80, 0x81, 0x02, 0xbb]) else {
            panic!("the frame is whole");
        };
        assert_eq!(frame, vec![0xaa, 0xbb]);
    }
}
