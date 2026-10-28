//! Ctrl+V of a picture or of copied files into the composer, as Discord
//! does: a screenshot arrives as image.png, files copied in a file
//! manager as themselves.
//!
//! egui-winit reads only text on Ctrl+V and, when the clipboard holds
//! none, swallows the key press. The V release still arrives, so a
//! release with Ctrl down and no press before it is the paste. The idea
//! comes from crmne's ZapFast.

use std::path::PathBuf;

/// How long after Ctrl was last seen down a V release still counts.
const GRACE: f64 = 0.5;

#[derive(Clone, Copy, Default)]
struct Keys {
    /// Ctrl is down, as of the last event that said.
    command: bool,
    /// When Ctrl was last seen down.
    command_at: Option<f64>,
    /// A V press reached us since the last release: typing, not a paste.
    typed_v: bool,
}

/// Whether this frame's input asks for a paste: a text paste, or the
/// release of a swallowed Ctrl+V.
pub fn requested(ctx: &egui::Context) -> bool {
    let id = egui::Id::new("paste-keys");
    let mut keys: Keys = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let requested = ctx.input(|input| {
        let now = input.time;
        if !input.focused {
            keys = Keys::default();
            return false;
        }
        if keys.command {
            keys.command_at = Some(now);
        }
        let recent = |at: Option<f64>| at.is_some_and(|at| now - at <= GRACE);
        let mut requested = false;
        for event in &input.events {
            match event {
                egui::Event::WindowFocused(_) => keys = Keys::default(),
                egui::Event::Paste(_) => requested = true,
                egui::Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } => {
                    if modifiers.command {
                        keys.command_at = Some(now);
                    }
                    keys.command = modifiers.command;
                    if *key != egui::Key::V {
                        continue;
                    }
                    if *pressed {
                        keys.typed_v = true;
                        continue;
                    }
                    requested |= !keys.typed_v && recent(keys.command_at);
                    keys.typed_v = false;
                }
                _ => {}
            }
        }
        requested
    });
    ctx.data_mut(|d| d.insert_temp(id, keys));
    requested
}

/// What the clipboard has to attach: the files copied in a file manager
/// first, since those also offer their icon as a picture, then a picture
/// saved as image.png. None when it holds only text.
pub fn files() -> Option<Vec<PathBuf>> {
    let mut clipboard = arboard::Clipboard::new()
        .inspect_err(|e| log::warn!("clipboard: {e}"))
        .ok()?;
    if let Ok(list) = clipboard.get().file_list() {
        // text/uri-list lines end in CRLF, and arboard splits only on LF.
        let list: Vec<PathBuf> = list
            .into_iter()
            .map(|p| match p.to_str() {
                Some(text) if text.ends_with('\r') => PathBuf::from(text.trim_end_matches('\r')),
                _ => p,
            })
            .filter(|p| p.is_file())
            .collect();
        if !list.is_empty() {
            return Some(list);
        }
    }
    let picture = clipboard.get_image().ok()?;
    let image = image::RgbaImage::from_raw(
        picture.width as u32,
        picture.height as u32,
        picture.bytes.into_owned(),
    )?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let folder = std::env::temp_dir()
        .join("discofast-paste")
        .join(nanos.to_string());
    std::fs::create_dir_all(&folder).ok()?;
    let path = folder.join("image.png");
    image
        .save_with_format(&path, image::ImageFormat::Png)
        .inspect_err(|e| log::warn!("pasted picture: {e}"))
        .ok()?;
    Some(vec![path])
}
