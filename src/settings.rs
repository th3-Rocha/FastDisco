//! What the user chose, kept in `settings.json` in the config folder. Read
//! once at start; saved whole through fastframe-file's writer, so the
//! interface never waits on the disk and a crash never leaves half a file.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The colours: the desktop's (Omarchy), Discord's dark or light, or a
/// palette file from the themes folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Omarchy's theme when the desktop is Omarchy, else Discord's dark.
    #[default]
    System,
    Dark,
    Light,
    /// A palette file in the themes folder.
    File(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    /// The interface's zoom, as Discord's "Chat font scaling".
    pub zoom: f32,
    pub show_members: bool,
    pub dm_profile: bool,
    /// Desktop notifications for direct messages and mentions.
    pub notifications: bool,
    pub notification_sound: bool,
    /// Closing the window keeps FastDisco running in the tray.
    pub close_to_tray: bool,
    pub start_at_login: bool,
    /// Show GIFs and animated emoji moving.
    pub animate: bool,
    /// Show link previews and bot embeds.
    pub embeds: bool,
    pub muted: bool,
    pub deafened: bool,
    /// Underlines misspelt words in the message field.
    pub spellcheck: bool,
    /// Words the user added to the dictionary, lowercased.
    pub dictionary: Vec<String>,
    /// The call's voice filter: noise suppression 0 off, 1 moderate,
    /// 2 high, 3 very high.
    pub noise_suppression: u8,
    pub echo_cancellation: bool,
    pub auto_gain: bool,
    /// The microphone sends only while the user talks.
    pub auto_sensitivity: bool,
    /// The app's little sounds: calls, muting, messages.
    pub sounds: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            zoom: 1.0,
            show_members: true,
            dm_profile: true,
            notifications: true,
            notification_sound: true,
            close_to_tray: true,
            start_at_login: false,
            animate: true,
            embeds: true,
            muted: false,
            deafened: false,
            spellcheck: true,
            dictionary: Vec::new(),
            noise_suppression: 2,
            echo_cancellation: true,
            auto_gain: true,
            auto_sensitivity: true,
            sounds: true,
        }
    }
}

/// Where FastDisco keeps its files.
pub const DIRS: fastframe_dirs::App =
    fastframe_dirs::App::new("fastdisco", "io.github", "th3-Rocha", "fastdisco");

pub fn config_dir() -> Option<PathBuf> {
    DIRS.installed().map(|d| d.config)
}

fn file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.json"))
}

impl Settings {
    /// The saved settings, or the defaults when there are none or they
    /// cannot be read.
    pub fn load() -> Self {
        file()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| {
                serde_json::from_slice(&bytes)
                    .inspect_err(|e| log::warn!("settings: {e}"))
                    .ok()
            })
            .unwrap_or_default()
    }

    pub fn save(&self, writer: &fastframe_file::Writer) {
        let Some(path) = file() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_vec_pretty(self) {
            Ok(bytes) => writer.write(&path, bytes, fastframe_file::Access::Default),
            Err(error) => log::warn!("settings: {error}"),
        }
    }
}
