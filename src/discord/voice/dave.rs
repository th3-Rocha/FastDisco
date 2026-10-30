//! DAVE, Discord's end-to-end encryption: the MLS group kept in step with
//! the voice server, and each Opus frame encrypted before libwebrtc sends
//! it and decrypted before it is decoded, where Discord's web client does
//! it with encoded insertable streams (its DaveSessionManager and
//! encryption worker).

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU16;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// DAVE's frames end in this.
const MAGIC: [u8; 2] = [0xfa, 0xfa];

/// What the voice gateway gets back.
pub enum Reply {
    Json(Value),
    Binary(u8, Vec<u8>),
}

#[derive(Default)]
struct Inner {
    session: Option<davey::DaveSession>,
    /// The protocol version in effect; 0 is transport encryption alone.
    version: u16,
    /// Whose sound each ssrc is.
    owners: HashMap<u32, u64>,
}

/// One call's DAVE. The frame transformers reach it from libwebrtc's
/// threads; the gateway's messages change it from the call's task.
pub struct Dave {
    inner: Mutex<Inner>,
    user: u64,
    channel: u64,
    pending: Mutex<HashMap<u16, u16>>,
    /// Who the server said is in the call, to check proposals against.
    pub known: Mutex<HashSet<u64>>,
    /// Commits and welcomes that failed in a row (`_consecutiveMLSInvalidMessages`).
    invalid: std::sync::atomic::AtomicU32,
}

impl Dave {
    pub fn new(user: u64, channel: u64) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::default(),
            user,
            channel,
            pending: Mutex::default(),
            known: Mutex::default(),
            invalid: std::sync::atomic::AtomicU32::new(0),
        })
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_owner(&self, ssrc: u32, user: u64) {
        self.inner().owners.insert(ssrc, user);
    }

    pub fn forget_user(&self, user: u64) {
        self.inner().owners.retain(|_, u| *u != user);
    }

    /// The frame libwebrtc is about to send, end-to-end encrypted once the
    /// group is ready.
    pub fn encrypt(&self, frame: &[u8], out: &mut Vec<u8>) -> bool {
        let mut inner = self.inner();
        if inner.version != 0
            && let Some(session) = inner.session.as_mut().filter(|s| s.is_ready())
        {
            match session.encrypt_opus(frame) {
                Ok(encrypted) => out.extend_from_slice(&encrypted),
                Err(e) => {
                    log::debug!("voice DAVE encrypt: {e:?}");
                    return false;
                }
            }
            return true;
        }
        out.extend_from_slice(frame);
        true
    }

    /// A received frame, without its end-to-end layer. False drops it:
    /// it is encrypted and there is no key for it yet.
    pub fn decrypt(&self, ssrc: u32, frame: &[u8], out: &mut Vec<u8>) -> bool {
        let mut inner = self.inner();
        let marked = frame.len() >= 11 && frame.ends_with(&MAGIC);
        if inner.version == 0 || !marked {
            out.extend_from_slice(frame);
            return true;
        }
        let Some(user) = inner.owners.get(&ssrc).copied() else {
            return false;
        };
        let Some(session) = inner.session.as_mut().filter(|s| s.is_ready()) else {
            return false;
        };
        match session.decrypt(user, davey::MediaType::AUDIO, frame) {
            Ok(plain) => {
                out.extend_from_slice(&plain);
                true
            }
            Err(e) => {
                log::trace!("voice DAVE decrypt: {e:?}");
                false
            }
        }
    }

    /// Five invalid commits or welcomes in a row: Discord's client gives
    /// up on the group and connects again.
    pub fn failing(&self) -> bool {
        self.invalid.load(std::sync::atomic::Ordering::Relaxed) >= 5
    }

    /// SELECT_PROTOCOL_ACK's dave_protocol_version (`_handleSecureFramesInit`):
    /// a session and its key package, or transport encryption alone.
    pub fn init(&self, version: u16) -> Vec<Reply> {
        if version == 0 {
            self.pending().insert(0, 0);
            self.execute(0);
            return Vec::new();
        }
        self.inner().version = version;
        self.reinit()
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<u16, u16>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A new MLS session and its key package for the server.
    fn reinit(&self) -> Vec<Reply> {
        let mut inner = self.inner();
        let Some(version) = NonZeroU16::new(inner.version) else {
            if let Some(session) = &mut inner.session {
                let _ = session.reset();
                session.set_passthrough_mode(true, Some(10));
            }
            return Vec::new();
        };
        let package = match inner.session.as_mut() {
            Some(session) => session
                .reinit(version, self.user, self.channel, None)
                .map_err(|e| format!("{e:?}"))
                .and_then(|()| session.create_key_package().map_err(|e| format!("{e:?}"))),
            None => davey::DaveSession::new(version, self.user, self.channel, None)
                .map_err(|e| format!("{e:?}"))
                .and_then(|mut session| {
                    let package = session.create_key_package().map_err(|e| format!("{e:?}"));
                    inner.session = Some(session);
                    package
                }),
        };
        match package {
            Ok(package) => vec![Reply::Binary(26, package)],
            Err(e) => {
                log::warn!("voice DAVE: {e}");
                Vec::new()
            }
        }
    }

    fn execute(&self, transition: u16) {
        let Some(new) = self.pending().remove(&transition) else {
            log::debug!("voice DAVE: unknown transition {transition}");
            return;
        };
        let mut inner = self.inner();
        let old = std::mem::replace(&mut inner.version, new);
        // Up from transport encryption alone: others' frames may still come
        // unencrypted for a moment.
        if transition > 0
            && old == 0
            && new != 0
            && let Some(session) = &mut inner.session
        {
            session.set_passthrough_mode(true, Some(10));
        }
    }

    fn ready(transition: u16) -> Reply {
        Reply::Json(json!({ "op": 23, "d": { "transition_id": transition } }))
    }

    /// The JSON DAVE operations: 21, 22 and 24.
    pub fn json(&self, op: u64, d: &Value) -> Vec<Reply> {
        let number = |key: &str| d[key].as_u64().unwrap_or(0) as u16;
        match op {
            // Prepare transition.
            21 => {
                let (transition, version) = (number("transition_id"), number("protocol_version"));
                self.pending().insert(transition, version);
                if transition == 0 {
                    self.execute(0);
                } else {
                    if version == 0
                        && let Some(session) = &mut self.inner().session
                    {
                        session.set_passthrough_mode(true, Some(120));
                    }
                    return vec![Self::ready(transition)];
                }
            }
            // Execute transition.
            22 => self.execute(number("transition_id")),
            // Prepare epoch: epoch 1 is a new group.
            24 if d["epoch"].as_u64() == Some(1) => {
                self.inner().version = number("protocol_version");
                return self.reinit();
            }
            _ => {}
        }
        Vec::new()
    }

    /// The binary DAVE operations: 25, 27, 29 and 30.
    pub fn binary(&self, op: u8, rest: &[u8]) -> Vec<Reply> {
        let transition =
            || (rest.len() >= 2).then(|| (u16::from_be_bytes([rest[0], rest[1]]), &rest[2..]));
        match op {
            // External sender.
            25 => {
                if let Some(session) = &mut self.inner().session
                    && let Err(e) = session.set_external_sender(rest)
                {
                    log::warn!("voice DAVE external sender: {e:?}");
                }
            }
            // Proposals: a commit (and welcome) back.
            27 => {
                let Some((&kind, proposals)) = rest.split_first() else {
                    return Vec::new();
                };
                let kind = if kind == 1 {
                    davey::ProposalsOperationType::REVOKE
                } else {
                    davey::ProposalsOperationType::APPEND
                };
                let mut known: Vec<u64> = self
                    .known
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .copied()
                    .collect();
                known.push(self.user);
                if let Some(session) = &mut self.inner().session {
                    match session.process_proposals(kind, proposals, Some(&known)) {
                        Ok(Some(cw)) => {
                            let mut body = cw.commit;
                            body.extend(cw.welcome.unwrap_or_default());
                            return vec![Reply::Binary(28, body)];
                        }
                        Ok(None) => {}
                        Err(e) => log::warn!("voice DAVE proposals: {e:?}"),
                    }
                }
            }
            // Announce commit transition, and welcome.
            29 | 30 => {
                let Some((id, body)) = transition() else {
                    return Vec::new();
                };
                let result = {
                    let mut inner = self.inner();
                    let Some(session) = &mut inner.session else {
                        return Vec::new();
                    };
                    if op == 29 {
                        session.process_commit(body).map_err(|e| format!("{e:?}"))
                    } else {
                        session.process_welcome(body).map_err(|e| format!("{e:?}"))
                    }
                };
                use std::sync::atomic::Ordering::Relaxed;
                if result.is_ok() {
                    self.invalid.store(0, Relaxed);
                } else {
                    self.invalid.fetch_add(1, Relaxed);
                }
                match result {
                    Ok(()) if id != 0 => {
                        let version = self.inner().version;
                        self.pending().insert(id, version);
                        return vec![Self::ready(id)];
                    }
                    Ok(()) => {}
                    Err(e) => {
                        log::warn!("voice DAVE {op}: {e}");
                        let invalid = json!({ "op": 31, "d": { "transition_id": id } });
                        let mut out = vec![Reply::Json(invalid)];
                        out.extend(self.reinit());
                        return out;
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_pass_through_without_dave() {
        let dave = Dave::new(1, 2);
        let mut out = Vec::new();
        assert!(dave.encrypt(&[1, 2, 3], &mut out));
        assert_eq!(out, [1, 2, 3]);
        out.clear();
        assert!(dave.decrypt(5, &[1, 2, 3], &mut out));
        assert_eq!(out, [1, 2, 3]);
    }

    #[test]
    fn an_encrypted_frame_without_a_key_is_dropped() {
        let dave = Dave::new(1, 2);
        let replies = dave.init(1);
        assert!(matches!(&replies[..], [Reply::Binary(26, _)]));
        let marked = [vec![0; 12], MAGIC.to_vec()].concat();
        let mut out = Vec::new();
        assert!(!dave.decrypt(5, &marked, &mut out));
    }

    #[test]
    fn transport_only_calls_need_no_session() {
        let dave = Dave::new(1, 2);
        assert!(dave.init(0).is_empty());
        let marked = [vec![0; 12], MAGIC.to_vec()].concat();
        let mut out = Vec::new();
        assert!(dave.decrypt(5, &marked, &mut out));
    }
}
