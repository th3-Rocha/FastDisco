//! Discord's JSON into the model. Lenient: a missing field takes a default
//! rather than failing the whole payload, since READY is large and its shape
//! drifts.

use egui::Color32;
use serde_json::Value;

use super::{CDN, avatar_url};
use crate::model::{
    Attachment, Channel, ChannelKind, CustomEmoji, Embed, EmbedField, EmbedMedia, Guild, Id,
    ListItem, Member, Message, Model, Overwrite, Reaction, ReactionEmoji, Reply, Role, Status,
    Sticker, User,
};

pub fn id(value: &Value) -> Option<Id> {
    match value {
        Value::String(text) => text.parse().ok(),
        Value::Number(number) => number.as_u64(),
        _ => None,
    }
}

fn text(value: &Value) -> Option<String> {
    value.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

/// A name for display. Inter's wide U+002D would put gaps in names like
/// "info-and-rules", so they keep a real hyphen; names are never sent back.
fn label(value: &Value) -> Option<String> {
    text(value).map(|t| crate::text::hyphens(&t).into_owned())
}

fn bits(value: &Value) -> u64 {
    id(value).unwrap_or(0)
}

pub fn user(value: &Value) -> Option<User> {
    let id = id(&value["id"])?;
    let username = text(&value["username"]).unwrap_or_default();
    let name = label(&value["global_name"])
        .unwrap_or_else(|| crate::text::hyphens(&username).into_owned());
    let picture = value["avatar"]
        .as_str()
        .and_then(|hash| avatar_url(&id.to_string(), hash))
        .or_else(|| Some(format!("{CDN}/embed/avatars/{}.png", (id >> 22) % 6)));
    Some(User {
        id,
        name,
        username,
        picture,
        status: Status::Offline,
        activity: None,
        bot: value["bot"].as_bool().unwrap_or(false),
    })
}

/// Status and the activity line from a presence.
pub fn presence(value: &Value) -> (Status, Option<String>) {
    let status = Status::parse(value["status"].as_str().unwrap_or("offline"));
    let activity = value["activities"].as_array().and_then(|list| {
        list.iter().find_map(|a| {
            let name = a["name"].as_str()?;
            Some(match a["type"].as_i64() {
                Some(0) => format!("Jogando {name}"),
                Some(1) => format!("Transmitindo {name}"),
                Some(2) => format!("Ouvindo {name}"),
                Some(3) => format!("Assistindo {name}"),
                Some(4) => text(&a["state"])?,
                Some(5) => format!("Competindo em {name}"),
                _ => name.to_owned(),
            })
        })
    });
    (status, activity)
}

pub fn channel(value: &Value, guild: Option<Id>) -> Option<Channel> {
    let id = id(&value["id"])?;
    let kind = ChannelKind::from_type(value["type"].as_i64().unwrap_or(0));
    let overwrites = value["permission_overwrites"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|o| {
                    Some(Overwrite {
                        id: self::id(&o["id"])?,
                        kind: match &o["type"] {
                            Value::String(s) if s == "member" => 1,
                            Value::String(_) => 0,
                            other => other.as_i64().unwrap_or(0),
                        },
                        allow: bits(&o["allow"]),
                        deny: bits(&o["deny"]),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let recipients = value["recipient_ids"]
        .as_array()
        .map(|list| list.iter().filter_map(self::id).collect())
        .or_else(|| {
            value["recipients"]
                .as_array()
                .map(|list| list.iter().filter_map(|u| self::id(&u["id"])).collect())
        })
        .unwrap_or_default();
    Some(Channel {
        id,
        guild: guild.or_else(|| self::id(&value["guild_id"])),
        name: label(&value["name"]).unwrap_or_default(),
        kind,
        topic: label(&value["topic"]),
        position: value["position"].as_i64().unwrap_or(0),
        parent: self::id(&value["parent_id"]),
        overwrites,
        last_message: self::id(&value["last_message_id"]),
        read: None,
        mentions: 0,
        messages: Vec::new(),
        requested: false,
        more: true,
        loading: false,
        recipients,
    })
}

fn role(value: &Value) -> Option<Role> {
    let color = value["color"].as_u64().unwrap_or(0);
    Some(Role {
        id: id(&value["id"])?,
        name: label(&value["name"]).unwrap_or_default(),
        color: (color != 0)
            .then(|| Color32::from_rgb((color >> 16) as u8, (color >> 8) as u8, color as u8)),
        position: value["position"].as_i64().unwrap_or(0),
        permissions: bits(&value["permissions"]),
    })
}

pub fn member(value: &Value) -> Member {
    Member {
        nick: label(&value["nick"]),
        roles: value["roles"]
            .as_array()
            .map(|list| list.iter().filter_map(id).collect())
            .unwrap_or_default(),
    }
}

/// A guild and its channels. Newer gateways put the name and icon under
/// `properties`.
pub fn guild(value: &Value, model: &mut Model) -> Option<Guild> {
    let id = id(&value["id"])?;
    let properties = if value["properties"].is_object() {
        &value["properties"]
    } else {
        value
    };
    let mut roles: Vec<Role> = value["roles"]
        .as_array()
        .map(|l| l.iter().filter_map(role).collect())
        .unwrap_or_default();
    roles.sort_by_key(|r| std::cmp::Reverse((r.position, r.id)));
    let mut channels = Vec::new();
    for list in [&value["channels"], &value["threads"]] {
        for c in list.as_array().into_iter().flatten() {
            if let Some(channel) = channel(c, Some(id)) {
                channels.push(channel.id);
                model.channels.insert(channel.id, channel);
            }
        }
    }
    let mut guild = Guild {
        id,
        name: label(&properties["name"]).unwrap_or_default(),
        icon: properties["icon"]
            .as_str()
            .map(|hash| format!("{CDN}/icons/{id}/{hash}.png?size=96")),
        owner: self::id(&properties["owner_id"]).unwrap_or(0),
        roles,
        channels,
        emojis: value["emojis"]
            .as_array()
            .map(|l| l.iter().filter_map(custom_emoji).collect())
            .unwrap_or_default(),
        stickers: value["stickers"]
            .as_array()
            .map(|l| l.iter().filter_map(sticker).collect())
            .unwrap_or_default(),
        ..Default::default()
    };
    for m in value["members"].as_array().into_iter().flatten() {
        if let Some(user) = user(&m["user"]) {
            guild.members.insert(user.id, member(m));
            model.users.entry(user.id).or_insert(user);
        }
    }
    Some(guild)
}

pub fn message(value: &Value, me: Id) -> Option<Message> {
    let author = &value["author"];
    let time = value["timestamp"].as_str()?.parse().ok()?;
    let mentions_me = value["mention_everyone"].as_bool().unwrap_or(false)
        || value["mentions"]
            .as_array()
            .is_some_and(|l| l.iter().any(|u| id(&u["id"]) == Some(me)));
    let flags = value["flags"].as_u64().unwrap_or(0);
    let reply = value["message_reference"]["message_id"]
        .is_string()
        .then(|| {
            let quoted = &value["referenced_message"];
            Some(Reply {
                author: id(&quoted["author"]["id"]),
                text: text(&quoted["content"]),
                has_media: quoted["attachments"]
                    .as_array()
                    .is_some_and(|l| !l.is_empty())
                    || quoted["embeds"].as_array().is_some_and(|l| !l.is_empty()),
            })
        })
        .flatten();
    let command = value["interaction_metadata"]["name"]
        .as_str()
        .or_else(|| value["interaction"]["name"].as_str())
        .map(|name| format!("/{name}"));
    Some(Message {
        id: id(&value["id"])?,
        nonce: match &value["nonce"] {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        },
        author: id(&author["id"])?,
        time,
        text: value["content"].as_str().unwrap_or_default().to_owned(),
        kind: value["type"].as_i64().unwrap_or(0),
        mentions_me,
        edited: !value["edited_timestamp"].is_null(),
        pending: false,
        failed: false,
        voice: flags & (1 << 13) != 0,
        attachments: value["attachments"]
            .as_array()
            .map(|l| l.iter().filter_map(attachment).collect())
            .unwrap_or_default(),
        embeds: value["embeds"]
            .as_array()
            .map(|l| l.iter().map(embed).collect())
            .unwrap_or_default(),
        stickers: value["sticker_items"]
            .as_array()
            .map(|l| l.iter().filter_map(sticker).collect())
            .unwrap_or_default(),
        reactions: value["reactions"]
            .as_array()
            .map(|l| l.iter().filter_map(reaction).collect())
            .unwrap_or_default(),
        reply,
        command,
    })
}

/// Everyone a message names, to learn their names.
pub fn mentioned_users(value: &Value) -> Vec<User> {
    value["mentions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(user)
        .collect()
}

fn attachment(value: &Value) -> Option<Attachment> {
    let name = value["filename"].as_str()?.to_owned();
    let waveform = value["waveform"]
        .as_str()
        .and_then(|w| base64::Engine::decode(&base64::engine::general_purpose::STANDARD, w).ok())
        .unwrap_or_default();
    Some(Attachment {
        url: value["url"].as_str()?.to_owned(),
        proxy: value["proxy_url"]
            .as_str()
            .or(value["url"].as_str())?
            .to_owned(),
        spoiler: name.starts_with("SPOILER_"),
        name,
        size: value["size"].as_u64().unwrap_or(0),
        content_type: text(&value["content_type"]),
        width: value["width"].as_u64().map(|v| v as u32),
        height: value["height"].as_u64().map(|v| v as u32),
        duration: value["duration_secs"].as_f64().map(|v| v as f32),
        waveform,
    })
}

fn media(value: &Value) -> Option<EmbedMedia> {
    Some(EmbedMedia {
        url: value["url"].as_str()?.to_owned(),
        proxy: text(&value["proxy_url"]),
        width: value["width"].as_u64().map(|v| v as u32),
        height: value["height"].as_u64().map(|v| v as u32),
    })
}

fn embed(value: &Value) -> Embed {
    let color = value["color"]
        .as_u64()
        .map(|c| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8));
    Embed {
        kind: value["type"].as_str().unwrap_or("rich").to_owned(),
        url: text(&value["url"]),
        title: text(&value["title"]),
        description: text(&value["description"]),
        color,
        provider: text(&value["provider"]["name"]),
        author: text(&value["author"]["name"]),
        author_url: text(&value["author"]["url"]),
        author_icon: text(&value["author"]["proxy_icon_url"])
            .or_else(|| text(&value["author"]["icon_url"])),
        footer: text(&value["footer"]["text"]),
        footer_icon: text(&value["footer"]["proxy_icon_url"])
            .or_else(|| text(&value["footer"]["icon_url"])),
        timestamp: value["timestamp"].as_str().and_then(|t| t.parse().ok()),
        fields: value["fields"]
            .as_array()
            .map(|l| {
                l.iter()
                    .map(|f| EmbedField {
                        name: f["name"].as_str().unwrap_or_default().to_owned(),
                        value: f["value"].as_str().unwrap_or_default().to_owned(),
                        inline: f["inline"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        image: media(&value["image"]),
        thumbnail: media(&value["thumbnail"]),
        video: media(&value["video"]),
    }
}

pub fn sticker(value: &Value) -> Option<Sticker> {
    Some(Sticker {
        id: id(&value["id"])?,
        name: value["name"].as_str().unwrap_or_default().to_owned(),
        format: value["format_type"].as_i64().unwrap_or(1),
    })
}

pub fn reaction_emoji(value: &Value) -> Option<ReactionEmoji> {
    match id(&value["id"]) {
        Some(id) => Some(ReactionEmoji::Custom {
            id,
            name: value["name"].as_str().unwrap_or("emoji").to_owned(),
            animated: value["animated"].as_bool().unwrap_or(false),
        }),
        None => Some(ReactionEmoji::Unicode(value["name"].as_str()?.to_owned())),
    }
}

fn reaction(value: &Value) -> Option<Reaction> {
    Some(Reaction {
        emoji: reaction_emoji(&value["emoji"])?,
        count: value["count"].as_u64().unwrap_or(1) as u32,
        me: value["me"].as_bool().unwrap_or(false),
    })
}

fn custom_emoji(value: &Value) -> Option<CustomEmoji> {
    Some(CustomEmoji {
        id: id(&value["id"])?,
        name: value["name"].as_str()?.to_owned(),
        animated: value["animated"].as_bool().unwrap_or(false),
    })
}

/// The whole READY payload into a fresh model.
pub fn ready(value: &Value) -> Model {
    let mut model = Model::default();
    if let Some(mut me) = user(&value["user"]) {
        model.me = me.id;
        // The status the user chose; "invisible" shows as offline.
        me.status = Status::parse(
            value["user_settings"]["status"]
                .as_str()
                .unwrap_or("online"),
        );
        model.premium = value["user"]["premium_type"].as_u64().unwrap_or(0) as u8;
        model.users.insert(me.id, me);
    }
    for u in value["users"].as_array().into_iter().flatten() {
        if let Some(u) = user(u) {
            model.users.entry(u.id).or_insert(u);
        }
    }
    for r in value["relationships"].as_array().into_iter().flatten() {
        if let Some(u) = user(&r["user"]) {
            model.users.entry(u.id).or_insert(u);
        }
    }
    let mut guilds: Vec<Guild> = value["guilds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| guild(g, &mut model))
        .collect();
    // Our own member entry on each guild, for permissions and colours.
    if let Some(merged) = value["merged_members"].as_array() {
        for (guild, members) in guilds.iter_mut().zip(merged) {
            for m in members.as_array().into_iter().flatten() {
                if let Some(user) = id(&m["user_id"]) {
                    guild.members.insert(user, member(m));
                }
            }
        }
    }
    // The rail's order, from the user's settings when they say.
    let order: Vec<Id> = value["user_settings"]["guild_positions"]
        .as_array()
        .map(|l| l.iter().filter_map(id).collect())
        .unwrap_or_default();
    if !order.is_empty() {
        guilds.sort_by_key(|g| {
            order
                .iter()
                .position(|id| *id == g.id)
                .unwrap_or(usize::MAX)
        });
    }
    model.guilds = guilds;

    for c in value["private_channels"].as_array().into_iter().flatten() {
        for u in c["recipients"].as_array().into_iter().flatten() {
            if let Some(u) = user(u) {
                model.users.entry(u.id).or_insert(u);
            }
        }
        if let Some(channel) = channel(c, None) {
            model.direct.push(channel.id);
            model.channels.insert(channel.id, channel);
        }
    }

    let read_state = if value["read_state"].is_array() {
        &value["read_state"]
    } else {
        &value["read_state"]["entries"]
    };
    for entry in read_state.as_array().into_iter().flatten() {
        if let Some(channel) = id(&entry["id"]).and_then(|id| model.channels.get_mut(&id)) {
            channel.read = id(&entry["last_message_id"]);
            channel.mentions = entry["mention_count"].as_u64().unwrap_or(0) as u32;
        }
    }
    // A DM never opened has no read state: Discord counts it read.
    for id in &model.direct {
        if let Some(channel) = model.channels.get_mut(id)
            && channel.read.is_none()
        {
            channel.read = channel.last_message;
        }
    }
    model
}

/// Presences from READY_SUPPLEMENTAL.
pub fn supplemental(value: &Value, model: &mut Model) {
    let friends = value["merged_presences"]["friends"]
        .as_array()
        .into_iter()
        .flatten();
    let guilds = value["merged_presences"]["guilds"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g.as_array().into_iter().flatten());
    for p in friends.chain(guilds) {
        let Some(uid) = id(&p["user_id"]).or_else(|| id(&p["user"]["id"])) else {
            continue;
        };
        let (status, activity) = presence(p);
        if let Some(user) = model.users.get_mut(&uid) {
            user.status = status;
            user.activity = activity;
        }
    }
}

/// One GUILD_MEMBER_LIST_UPDATE, applied to the guild's list.
pub fn member_list(value: &Value, model: &mut Model) {
    let Some(guild_id) = id(&value["guild_id"]) else {
        return;
    };
    let mut users = Vec::new();
    let item = |value: &Value,
                users: &mut Vec<(User, Member, Status, Option<String>)>|
     -> Option<ListItem> {
        if let Some(group) = value.get("group") {
            return Some(ListItem::Group {
                id: group["id"].as_str().unwrap_or_default().to_owned(),
                count: group["count"].as_u64().unwrap_or(0) as u32,
            });
        }
        let m = value.get("member")?;
        let u = user(&m["user"])?;
        let (status, activity) = presence(&m["presence"]);
        let id = u.id;
        users.push((u, member(m), status, activity));
        Some(ListItem::Member(id))
    };
    let mut list = model
        .guild(guild_id)
        .map(|g| g.list.clone())
        .unwrap_or_default();
    for op in value["ops"].as_array().into_iter().flatten() {
        match op["op"].as_str() {
            Some("SYNC") => {
                let start = op["range"][0].as_u64().unwrap_or(0) as usize;
                let items: Vec<ListItem> = op["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| item(v, &mut users))
                    .collect();
                list.truncate(start);
                list.extend(items);
            }
            Some("INSERT") => {
                let index = op["index"].as_u64().unwrap_or(0) as usize;
                if let Some(i) = item(&op["item"], &mut users) {
                    list.insert(index.min(list.len()), i);
                }
            }
            Some("UPDATE") => {
                let index = op["index"].as_u64().unwrap_or(0) as usize;
                if let Some(i) = item(&op["item"], &mut users)
                    && index < list.len()
                {
                    list[index] = i;
                }
            }
            Some("DELETE") => {
                let index = op["index"].as_u64().unwrap_or(0) as usize;
                if index < list.len() {
                    list.remove(index);
                }
            }
            _ => {}
        }
    }
    // Group rows carry no count of their own; the list's groups do.
    let counts: std::collections::HashMap<String, u32> = value["groups"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| {
            Some((
                g["id"].as_str()?.to_owned(),
                g["count"].as_u64().unwrap_or(0) as u32,
            ))
        })
        .collect();
    for item in &mut list {
        if let ListItem::Group { id, count } = item
            && let Some(c) = counts.get(id)
        {
            *count = *c;
        }
    }
    for (u, m, status, activity) in users {
        let id = u.id;
        let entry = model.users.entry(id).or_insert(u);
        entry.status = status;
        entry.activity = activity;
        if let Some(guild) = model.guild_mut(guild_id) {
            guild.members.insert(id, m);
        }
    }
    if let Some(guild) = model.guild_mut(guild_id) {
        guild.list = list;
    }
}
