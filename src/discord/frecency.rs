//! Favourite GIFs, kept by Discord in the user's "frecency" settings: a
//! protobuf (FrecencyUserSettings) at `/users/@me/settings-proto/2`.
//! Field 2 holds FavoriteGIFs, whose field 1 maps each GIF's page URL to
//! { format = 1, src = 2, width = 3, height = 4, order = 5 }.
//!
//! Only that map is ever changed, and only by adding or removing whole
//! entries: every other byte of the settings is written back as it came,
//! so fields this code does not know survive.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::API;

#[derive(Clone, Debug)]
pub struct Favorite {
    /// The page link, which is what gets sent.
    pub url: String,
    /// The media shown in the picker.
    pub src: String,
    pub width: u32,
    pub height: u32,
    pub order: u32,
}

/// One protobuf field: its number, wire type and the bytes of its value
/// (the whole encoded field in `raw`).
struct Field<'a> {
    number: u64,
    wire: u8,
    value: &'a [u8],
    varint: u64,
    raw: &'a [u8],
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

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
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

fn fields(bytes: &[u8]) -> Option<Vec<Field<'_>>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let key = read_varint(bytes, &mut at)?;
        let wire = (key & 7) as u8;
        let (value, varint) = match wire {
            0 => {
                let v = read_varint(bytes, &mut at)?;
                (&bytes[at..at], v)
            }
            1 => {
                at += 8;
                (bytes.get(at - 8..at)?, 0)
            }
            2 => {
                let len = read_varint(bytes, &mut at)? as usize;
                at += len;
                (bytes.get(at - len..at)?, 0)
            }
            5 => {
                at += 4;
                (bytes.get(at - 4..at)?, 0)
            }
            _ => return None,
        };
        out.push(Field {
            number: key >> 3,
            wire,
            value,
            varint,
            raw: bytes.get(start..at)?,
        });
    }
    Some(out)
}

fn put_bytes(out: &mut Vec<u8>, number: u64, value: &[u8]) {
    write_varint(out, number << 3 | 2);
    write_varint(out, value.len() as u64);
    out.extend_from_slice(value);
}

fn put_varint(out: &mut Vec<u8>, number: u64, value: u64) {
    write_varint(out, number << 3);
    write_varint(out, value);
}

/// The favourites in a settings blob, newest first.
pub fn parse(settings: &[u8]) -> Vec<Favorite> {
    let mut list = Vec::new();
    for top in fields(settings)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| f.number == 2 && f.wire == 2)
    {
        for entry in fields(top.value)
            .unwrap_or_default()
            .into_iter()
            .filter(|f| f.number == 1 && f.wire == 2)
        {
            if let Some(favorite) = entry_of(entry.value) {
                list.push(favorite);
            }
        }
    }
    list.sort_by_key(|f| std::cmp::Reverse(f.order));
    list
}

fn entry_of(bytes: &[u8]) -> Option<Favorite> {
    let mut url = None;
    let mut favorite = Favorite {
        url: String::new(),
        src: String::new(),
        width: 0,
        height: 0,
        order: 0,
    };
    for f in fields(bytes)? {
        match (f.number, f.wire) {
            (1, 2) => url = Some(String::from_utf8_lossy(f.value).into_owned()),
            (2, 2) => {
                for g in fields(f.value)? {
                    match (g.number, g.wire) {
                        (2, 2) => favorite.src = String::from_utf8_lossy(g.value).into_owned(),
                        (3, 0) => favorite.width = g.varint as u32,
                        (4, 0) => favorite.height = g.varint as u32,
                        (5, 0) => favorite.order = g.varint as u32,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    favorite.url = url?;
    Some(favorite)
}

/// The settings with `gif` added to the favourites, or removed when `add`
/// is false. `None` when the blob cannot be read safely.
pub fn change(settings: &[u8], gif: &Favorite, add: bool) -> Option<Vec<u8>> {
    let top = fields(settings)?;
    let current = parse(settings);
    let next_order = current.iter().map(|f| f.order).max().unwrap_or(0) + 1;
    let mut out = Vec::with_capacity(settings.len() + 256);
    let mut written = false;
    for field in &top {
        if field.number != 2 || field.wire != 2 {
            out.extend_from_slice(field.raw);
            continue;
        }
        if written {
            continue;
        }
        written = true;
        let mut inner = Vec::new();
        for f in fields(field.value)? {
            let drop =
                f.number == 1 && f.wire == 2 && entry_of(f.value).is_some_and(|e| e.url == gif.url);
            if !drop {
                inner.extend_from_slice(f.raw);
            }
        }
        if add {
            put_entry(&mut inner, gif, next_order);
        }
        put_bytes(&mut out, 2, &inner);
    }
    if !written && add {
        let mut inner = Vec::new();
        put_entry(&mut inner, gif, next_order);
        put_bytes(&mut out, 2, &inner);
    }
    Some(out)
}

fn put_entry(out: &mut Vec<u8>, gif: &Favorite, order: u32) {
    let mut value = Vec::new();
    // 2 is a video (MP4), 1 an image (GIF).
    let format = if gif.src.ends_with(".mp4") { 2 } else { 1 };
    put_varint(&mut value, 1, format);
    put_bytes(&mut value, 2, gif.src.as_bytes());
    put_varint(&mut value, 3, u64::from(gif.width));
    put_varint(&mut value, 4, u64::from(gif.height));
    put_varint(&mut value, 5, u64::from(order));
    let mut entry = Vec::new();
    put_bytes(&mut entry, 1, gif.url.as_bytes());
    put_bytes(&mut entry, 2, &value);
    put_bytes(out, 1, &entry);
}

pub async fn load(client: &reqwest::Client, token: &str) -> Result<Vec<u8>, String> {
    let response = client
        .get(format!("{API}/users/@me/settings-proto/2"))
        .header("Authorization", token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let value: Value = response.json().await.map_err(|e| e.to_string())?;
    STANDARD
        .decode(value["settings"].as_str().unwrap_or_default())
        .map_err(|e| e.to_string())
}

pub async fn save(client: &reqwest::Client, token: &str, settings: &[u8]) -> Result<(), String> {
    let response = client
        .patch(format!("{API}/users/@me/settings-proto/2"))
        .header("Authorization", token)
        .json(&json!({ "settings": STANDARD.encode(settings) }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(response.status().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gif(url: &str) -> Favorite {
        Favorite {
            url: url.into(),
            src: format!("{url}.mp4"),
            width: 200,
            height: 100,
            order: 0,
        }
    }

    #[test]
    fn adding_and_removing_keeps_every_other_byte() {
        // Field 1 (versions) and field 3 (stickers) must survive untouched.
        let mut settings = Vec::new();
        put_bytes(&mut settings, 1, &[8, 5]);
        put_bytes(&mut settings, 3, b"stickers");
        let added = change(&settings, &gif("https://tenor.com/a"), true).unwrap();
        let added = change(&added, &gif("https://tenor.com/b"), true).unwrap();
        let list = parse(&added);
        assert_eq!(
            list.iter().map(|f| f.url.as_str()).collect::<Vec<_>>(),
            ["https://tenor.com/b", "https://tenor.com/a"]
        );
        assert_eq!(list[0].src, "https://tenor.com/b.mp4");
        assert_eq!((list[0].width, list[0].height), (200, 100));
        let removed = change(&added, &gif("https://tenor.com/b"), false).unwrap();
        assert_eq!(parse(&removed).len(), 1);
        let kept: Vec<_> = fields(&removed)
            .unwrap()
            .into_iter()
            .filter(|f| f.number != 2)
            .map(|f| f.raw.to_vec())
            .collect();
        let original: Vec<_> = fields(&settings)
            .unwrap()
            .into_iter()
            .map(|f| f.raw.to_vec())
            .collect();
        assert_eq!(kept, original);
    }

    #[test]
    fn unknown_fields_inside_favorites_survive() {
        let mut inner = Vec::new();
        put_varint(&mut inner, 2, 1); // hide_tooltip
        let mut settings = Vec::new();
        put_bytes(&mut settings, 2, &inner);
        let added = change(&settings, &gif("https://tenor.com/x"), true).unwrap();
        let top = fields(&added).unwrap();
        let favorites = fields(top[0].value).unwrap();
        assert!(favorites.iter().any(|f| f.number == 2 && f.varint == 1));
    }
}
