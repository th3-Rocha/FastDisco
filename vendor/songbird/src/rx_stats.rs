//! Discofast: where received voice packets went missing, counted for the
//! call statistics. Each counter is taken (read and zeroed) by the report.

use std::sync::atomic::{AtomicU64, Ordering};

/// The counters.
#[derive(Debug, Default)]
pub struct RxStats {
    /// End-to-end encrypted packets that failed to decrypt.
    pub dave_failed: AtomicU64,
    /// Encrypted packets dropped while the DAVE session was not ready, or
    /// from someone whose stream is not known yet.
    pub dave_waiting: AtomicU64,
    /// Packets that arrived after their turn to play had passed.
    pub late: AtomicU64,
    /// Packets too far ahead of the playout to keep.
    pub too_far: AtomicU64,
    /// The playout started over after losing its place.
    pub desync: AtomicU64,
    /// The playout ran dry while someone was talking: silence until it
    /// refilled.
    pub underrun: AtomicU64,
    /// A packet held back because its timestamp was ahead of the playout.
    pub withheld: AtomicU64,
    /// A missing packet whose successor was there, so its FEC can recover it.
    pub fec_available: AtomicU64,
}

impl RxStats {
    /// The counters since the last call, zeroed.
    pub fn take(&self) -> [u64; 8] {
        [
            &self.dave_failed,
            &self.dave_waiting,
            &self.late,
            &self.too_far,
            &self.desync,
            &self.underrun,
            &self.withheld,
            &self.fec_available,
        ]
        .map(|c| c.swap(0, Ordering::Relaxed))
    }
}

/// The process's counters: one call at a time.
pub static RX: RxStats = RxStats {
    dave_failed: AtomicU64::new(0),
    dave_waiting: AtomicU64::new(0),
    late: AtomicU64::new(0),
    too_far: AtomicU64::new(0),
    desync: AtomicU64::new(0),
    underrun: AtomicU64::new(0),
    withheld: AtomicU64::new(0),
    fec_available: AtomicU64::new(0),
};

pub(crate) fn count(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}
