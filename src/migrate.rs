//! Moving what FastDisco kept onto Discofast, the app's name since 0.10:
//! the config, state and cache folders, and starting at login. The token
//! in the keyring moves on its first read (see `session`). Each step runs
//! only while the old one is there and the new one is not, so this costs
//! nothing once done.

use std::path::Path;

use crate::updates::LEGACY_SLUG;

/// Where FastDisco kept its files.
const LEGACY_DIRS: fastframe_dirs::App =
    fastframe_dirs::App::new(LEGACY_SLUG, "io.github", "th3-Rocha", LEGACY_SLUG);

/// Runs once this process is the running copy, before the settings load.
pub fn run() {
    if let (Some(old), Some(new)) = (LEGACY_DIRS.installed(), crate::settings::DIRS.installed()) {
        for (from, to) in [
            (old.config, new.config),
            (old.state, new.state),
            (old.cache, new.cache),
        ] {
            move_folder(&from, &to);
        }
    }
    // The desktop's records may sit behind a portal; never hold up the window.
    std::thread::spawn(move_autostart);
}

fn move_folder(from: &Path, to: &Path) {
    if from == to || !from.is_dir() || to.exists() {
        return;
    }
    let moved = to
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::rename(from, to));
    match moved {
        Ok(()) => log::info!("moved {} to {}", from.display(), to.display()),
        Err(error) => log::warn!(
            "could not move {} to {}: {error}",
            from.display(),
            to.display()
        ),
    }
}

fn move_autostart() {
    use fastframe_autostart::{Autostart, State};
    let old = Autostart::new(LEGACY_SLUG, "FastDisco");
    let (State::On(start) | State::NeedsApproval(start)) = old.state() else {
        return;
    };
    let new = Autostart::new(crate::updates::CONFIG.slug, "Discofast");
    match new.set(Some(start)).and_then(|_| old.set(None)) {
        Ok(_) => log::info!("starting at login moved to Discofast"),
        Err(error) => log::warn!("could not move starting at login: {error}"),
    }
}
