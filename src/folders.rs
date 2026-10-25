//! Moving servers in the rail, into and out of folders, and saving the
//! order where Discord keeps it: the `guild_folders` part (field 14) of the
//! user's protobuf settings. The rest of the settings travels back
//! byte for byte, and the field is only written over when what is there
//! reads as the user's own folders; otherwise the older JSON settings take
//! the change.

use base64::Engine;
use reqwest::Method;
use serde_json::{Value, json};

use crate::app::App;
use crate::backend::Command;
use crate::model::{Folder, Id};

const GUILD_FOLDERS: u64 = 14;
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    /// The current settings, to put the folders in.
    Settings,
    Saved,
}

/// Which rail entry a drop is about: a folder by its id, or a server
/// alone by the server's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Folder(u64),
    Guild(Id),
}

impl Key {
    pub fn of(folder: &Folder) -> Self {
        match folder.id {
            Some(id) => Key::Folder(id),
            None => Key::Guild(folder.guilds[0]),
        }
    }
}

/// Where a dragged server lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drop {
    /// On its own, before or after an entry.
    Before(Key),
    After(Key),
    /// Into a folder, before a server in it or at the end; onto a server
    /// alone, a new folder with both.
    Into(Key, Option<Id>),
}

impl App {
    fn folders_api(&self, tag: Tag, method: Method, path: &str, body: Option<Value>) {
        self.backend().send(Command::Api {
            tag: crate::backend::Tag::Folders(tag),
            method,
            path: path.into(),
            body,
        });
    }

    pub fn move_guild(&mut self, guild: Id, drop: Drop) {
        let before = self.model.folders.clone();
        if !place(&mut self.model.folders, guild, drop) {
            return;
        }
        self.model.settle_folders();
        if self.model.folders != before {
            self.folders_api(
                Tag::Settings,
                Method::GET,
                "/users/@me/settings-proto/1",
                None,
            );
        }
    }

    pub fn folders_result(&mut self, tag: Tag, result: Result<Value, String>) {
        match (tag, result) {
            (Tag::Settings, Ok(value)) => {
                let bytes = value["settings"]
                    .as_str()
                    .and_then(|s| B64.decode(s).ok())
                    .unwrap_or_default();
                let known: Vec<Id> = self.model.guilds.iter().map(|g| g.id).collect();
                match with_folders(&bytes, &self.model.folders, &known) {
                    Some(settings) => self.folders_api(
                        Tag::Saved,
                        Method::PATCH,
                        "/users/@me/settings-proto/1",
                        Some(json!({ "settings": B64.encode(settings) })),
                    ),
                    None => self.save_folders_json(),
                }
            }
            (Tag::Settings, Err(_)) => self.save_folders_json(),
            (Tag::Saved, Err(error)) => {
                log::warn!("saving the rail's folders: {error}");
                self.chat.dialog = Some(crate::chat::Dialog::Failed(format!(
                    "Não foi possível salvar a ordem dos servidores: {error}"
                )));
            }
            (Tag::Saved, Ok(_)) => {}
        }
    }

    /// The older settings, which name their fields.
    fn save_folders_json(&self) {
        let folders: Vec<Value> = self
            .model
            .folders
            .iter()
            .map(|f| {
                json!({
                    "id": f.id,
                    "name": f.name,
                    "color": f.color,
                    "guild_ids": f.guilds.iter().map(|g| g.to_string()).collect::<Vec<_>>(),
                })
            })
            .collect();
        let positions: Vec<String> = self.model.guilds.iter().map(|g| g.id.to_string()).collect();
        self.folders_api(
            Tag::Saved,
            Method::PATCH,
            "/users/@me/settings",
            Some(json!({ "guild_folders": folders, "guild_positions": positions })),
        );
    }
}

/// Takes `guild` out of where it is and puts it where `drop` says.
/// False when there is nothing to do.
fn place(folders: &mut Vec<Folder>, guild: Id, drop: Drop) -> bool {
    let target = match drop {
        Drop::Before(k) | Drop::After(k) | Drop::Into(k, _) => k,
    };
    if target == Key::Guild(guild) {
        return false;
    }
    let Some(from) = folders.iter().position(|f| f.guilds.contains(&guild)) else {
        return false;
    };
    folders[from].guilds.retain(|g| *g != guild);
    if folders[from].guilds.is_empty() {
        folders.remove(from);
    }
    let Some(at) = folders.iter().position(|f| Key::of(f) == target) else {
        // The target was the folder it left, now gone: put it back alone.
        folders.insert(from.min(folders.len()), Folder::single(guild));
        return true;
    };
    match drop {
        Drop::Before(_) => folders.insert(at, Folder::single(guild)),
        Drop::After(_) => folders.insert(at + 1, Folder::single(guild)),
        Drop::Into(_, before) => {
            let folder = &mut folders[at];
            if folder.id.is_none() {
                // Two servers make a folder, with a new id as Discord does.
                folder.id = Some(new_folder_id());
            }
            let index = before
                .and_then(|b| folder.guilds.iter().position(|g| *g == b))
                .unwrap_or(folder.guilds.len());
            folder.guilds.insert(index, guild);
        }
    }
    true
}

/// Discord makes folder ids from random numbers below 2^32.
fn new_folder_id() -> u64 {
    u64::from(rand::random::<u32>())
}

/// The settings with their folders replaced by `folders`, every other
/// field kept as it was; `None` when the settings do not read as
/// protobuf, or their folder field does not hold this user's servers.
fn with_folders(settings: &[u8], folders: &[Folder], known: &[Id]) -> Option<Vec<u8>> {
    let fields = fields(settings)?;
    let mut old_ids = Vec::new();
    for (number, _, body) in &fields {
        if *number == GUILD_FOLDERS {
            old_ids.extend(folder_guilds(body)?);
        }
    }
    // What is there must be folders of servers the user is or was on.
    if !old_ids.is_empty() {
        let mine = old_ids.iter().filter(|id| known.contains(id)).count();
        if mine * 2 < old_ids.len() {
            return None;
        }
    }
    let mut out = Vec::with_capacity(settings.len() + 64);
    for (number, raw, _) in &fields {
        if *number != GUILD_FOLDERS {
            out.extend_from_slice(raw);
        }
    }
    let encoded = encode_folders(folders);
    varint(&mut out, (GUILD_FOLDERS << 3) | 2);
    varint(&mut out, encoded.len() as u64);
    out.extend_from_slice(&encoded);
    Some(out)
}

/// A field: its number, its bytes as they were, and the body of a
/// length-delimited one.
type Field<'a> = (u64, &'a [u8], &'a [u8]);

/// The fields of a protobuf message, in order.
fn fields(bytes: &[u8]) -> Option<Vec<Field<'_>>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let tag = read_varint(bytes, &mut at)?;
        let body: &[u8] = match tag & 7 {
            0 => {
                read_varint(bytes, &mut at)?;
                &[]
            }
            1 => {
                at = at.checked_add(8).filter(|e| *e <= bytes.len())?;
                &[]
            }
            2 => {
                let len = read_varint(bytes, &mut at)? as usize;
                let end = at.checked_add(len).filter(|e| *e <= bytes.len())?;
                let body = &bytes[at..end];
                at = end;
                body
            }
            5 => {
                at = at.checked_add(4).filter(|e| *e <= bytes.len())?;
                &[]
            }
            _ => return None,
        };
        out.push((tag >> 3, &bytes[start..at], body));
    }
    Some(out)
}

/// Every server id in a GuildFolders message.
fn folder_guilds(body: &[u8]) -> Option<Vec<Id>> {
    let mut ids = Vec::new();
    for (number, _, folder) in fields(body)? {
        if number != 1 {
            continue;
        }
        for (field, _, packed) in fields(folder)? {
            if field == 1 {
                if packed.len() % 8 != 0 {
                    return None;
                }
                ids.extend(
                    packed
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|c| u64::from_le_bytes(*c)),
                );
            }
        }
    }
    Some(ids)
}

/// GuildFolders { repeated GuildFolder folders = 1; repeated fixed64
/// guild_positions = 2; }, each GuildFolder { repeated fixed64 guild_ids
/// = 1; Int64Value id = 2; StringValue name = 3; UInt64Value color = 4; }.
fn encode_folders(folders: &[Folder]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut positions = Vec::new();
    for folder in folders {
        let mut f = Vec::new();
        let mut ids = Vec::new();
        for guild in &folder.guilds {
            ids.extend_from_slice(&guild.to_le_bytes());
            positions.extend_from_slice(&guild.to_le_bytes());
        }
        message(&mut f, 1, &ids);
        if let Some(id) = folder.id {
            let mut wrapper = Vec::new();
            varint(&mut wrapper, 1 << 3);
            varint(&mut wrapper, id);
            message(&mut f, 2, &wrapper);
        }
        if let Some(name) = &folder.name {
            let mut wrapper = Vec::new();
            message(&mut wrapper, 1, name.as_bytes());
            message(&mut f, 3, &wrapper);
        }
        if let Some(color) = folder.color {
            let mut wrapper = Vec::new();
            varint(&mut wrapper, 1 << 3);
            varint(&mut wrapper, u64::from(color));
            message(&mut f, 4, &wrapper);
        }
        message(&mut out, 1, &f);
    }
    message(&mut out, 2, &positions);
    out
}

fn message(out: &mut Vec<u8>, number: u64, body: &[u8]) {
    varint(out, (number << 3) | 2);
    varint(out, body.len() as u64);
    out.extend_from_slice(body);
}

fn varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn read_varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rail() -> Vec<Folder> {
        vec![
            Folder::single(1),
            Folder {
                id: Some(77),
                name: Some("Jogos".into()),
                color: Some(0x5865f2),
                guilds: vec![2, 3],
            },
            Folder::single(4),
        ]
    }

    fn order(folders: &[Folder]) -> Vec<Vec<Id>> {
        folders.iter().map(|f| f.guilds.clone()).collect()
    }

    #[test]
    fn a_server_moves_before_and_after_others() {
        let mut f = rail();
        assert!(place(&mut f, 4, Drop::Before(Key::Guild(1))));
        assert_eq!(order(&f), vec![vec![4], vec![1], vec![2, 3]]);
        assert!(place(&mut f, 4, Drop::After(Key::Folder(77))));
        assert_eq!(order(&f), vec![vec![1], vec![2, 3], vec![4]]);
        assert!(!place(&mut f, 4, Drop::Before(Key::Guild(4))));
    }

    #[test]
    fn dropping_on_a_server_makes_a_folder_and_leaving_one_keeps_the_rest() {
        let mut f = rail();
        assert!(place(&mut f, 1, Drop::Into(Key::Guild(4), None)));
        assert_eq!(order(&f), vec![vec![2, 3], vec![4, 1]]);
        assert!(f[1].id.is_some());
        assert!(place(&mut f, 2, Drop::Before(Key::Folder(77))));
        assert_eq!(order(&f), vec![vec![2], vec![3], vec![4, 1]]);
        let folder = f[2].id.unwrap();
        assert!(place(&mut f, 3, Drop::Into(Key::Folder(folder), Some(1))));
        assert_eq!(order(&f), vec![vec![2], vec![4, 3, 1]]);
    }

    #[test]
    fn the_folders_go_back_into_the_settings_and_nothing_else_changes() {
        let mut settings = Vec::new();
        // Field 13 (appearance) with some bytes, then the old folders.
        message(&mut settings, 13, &[8, 1]);
        message(&mut settings, GUILD_FOLDERS, &encode_folders(&rail()));
        message(&mut settings, 20, &[8, 5]);
        let mut moved = rail();
        place(&mut moved, 4, Drop::Before(Key::Guild(1)));
        let out = with_folders(&settings, &moved, &[1, 2, 3, 4]).expect("reads");
        let read = fields(&out).unwrap();
        assert_eq!(
            read.iter().map(|f| f.0).collect::<Vec<_>>(),
            vec![13, 20, 14]
        );
        assert_eq!(read[0].2, &[8, 1]);
        assert_eq!(folder_guilds(read[2].2).unwrap(), vec![4, 1, 2, 3]);
    }

    #[test]
    fn folders_of_strangers_are_left_alone() {
        let mut settings = Vec::new();
        message(&mut settings, GUILD_FOLDERS, &encode_folders(&rail()));
        assert!(with_folders(&settings, &rail(), &[99]).is_none());
        assert!(with_folders(&[0xff, 0xff], &rail(), &[1]).is_none());
    }
}
