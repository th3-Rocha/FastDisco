//! Updating itself from GitHub releases, through fastframe-update: a
//! check when Discofast opens and once a day, the download in the
//! background, and, when the user chooses, a helper that swaps the
//! executable and restarts it, going back to this one if the new one does
//! not start. Releases carry a `checksums.txt` signed with the key in
//! `assets/update-public-key.hex`. Copies a package manager installed
//! (the AUR) or built from source are left alone.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use fastframe_update::{Prepared, ReqwestTransport, UpdateConfig, Updater};

use crate::backend::Event;

/// The command name before the rename to Discofast.
pub const LEGACY_SLUG: &str = "fastdisco";

pub const CONFIG: UpdateConfig = UpdateConfig {
    publisher_key: Some(include_str!("../assets/update-public-key.hex")),
    // The AppImage replaces itself in place.
    appimage: true,
    // The app was FastDisco up to 0.9: its markers, executables and
    // `--version` answer still count as this app's.
    legacy_names: &[LEGACY_SLUG],
    ..UpdateConfig::new(
        "th3-Rocha/FastDisco",
        "Discofast",
        "discofast",
        env!("CARGO_PKG_VERSION"),
    )
};

/// Where updating stands, for the interface.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Idle,
    Checking,
    UpToDate,
    /// This copy cannot replace itself, and why.
    Unsupported(String),
    /// Downloading the version, this far (0 to 1).
    Downloading(String, f32),
    /// Downloaded and checked; restarting installs it.
    Ready(String),
    Failed(String),
}

/// The downloaded update, kept until the user restarts.
#[derive(Default)]
pub struct Pending(Mutex<Option<(Updater, Prepared)>>);

fn updater() -> anyhow::Result<Updater> {
    Ok(Updater::new(
        CONFIG,
        ReqwestTransport::new(reqwest::blocking::Client::builder())?,
    ))
}

/// Checks, and downloads what it finds, on a thread of its own: the
/// updater's calls block.
pub fn check(events: Sender<Event>, wake: impl Fn() + Send + 'static, pending: Arc<Pending>) {
    std::thread::spawn(move || {
        let send = |status: Status| {
            let _ = events.send(Event::Update(status));
            wake();
        };
        if pending
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
        {
            return;
        }
        send(Status::Checking);
        let updater = match updater() {
            Ok(u) => u,
            Err(e) => return send(Status::Failed(e.to_string())),
        };
        let release = match updater.check() {
            Ok(Some(release)) => release,
            Ok(None) => return send(Status::UpToDate),
            Err(e) => return send(Status::Failed(e.to_string())),
        };
        if let Err(reason) = updater.installation() {
            return send(Status::Unsupported(reason.to_string()));
        }
        let version = release.version.clone();
        let mut last = -1.0f32;
        let prepared = updater.download(&release, |received, total| {
            let share = if total == 0 {
                0.0
            } else {
                received as f32 / total as f32
            };
            // A redraw per percent, not per packet.
            if share - last >= 0.01 {
                last = share;
                send(Status::Downloading(version.clone(), share));
            }
        });
        match prepared {
            Ok(prepared) => {
                *pending.0.lock().unwrap_or_else(|e| e.into_inner()) = Some((updater, prepared));
                send(Status::Ready(version));
            }
            Err(e) => send(Status::Failed(e.to_string())),
        }
    });
}

/// Hands the downloaded update to the helper, which waits for Discofast
/// to close. True when the app should now quit.
pub fn install(pending: &Pending) -> Result<(), String> {
    let Some((updater, prepared)) = pending.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    else {
        return Err("Nenhuma atualização baixada.".into());
    };
    updater
        .handoff(prepared, Vec::new())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_update_config_is_valid() {
        CONFIG.validate().unwrap();
        assert_eq!(CONFIG.slug, "discofast");
        assert_eq!(CONFIG.current_version, env!("CARGO_PKG_VERSION"));
    }
}
