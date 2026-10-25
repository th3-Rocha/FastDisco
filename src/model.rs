//! What the interface shows: servers, channels, people and messages, keyed
//! by Discord's snowflake ids. The gateway fills it (`discord::parse`).

use std::collections::HashMap;

use egui::Color32;

/// A Discord snowflake.
pub type Id = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    Online,
    Idle,
    DoNotDisturb,
    #[default]
    Offline,
}

impl Status {
    pub fn parse(text: &str) -> Self {
        match text {
            "online" => Status::Online,
            "idle" => Status::Idle,
            "dnd" => Status::DoNotDisturb,
            _ => Status::Offline,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct User {
    pub id: Id,
    /// The display name, else the username.
    pub name: String,
    pub username: String,
    /// The avatar picture's URL on Discord's CDN.
    pub picture: Option<String>,
    pub status: Status,
    /// "Jogando ...", or the custom status: the line under the name.
    pub activity: Option<String>,
    /// Everything the user is doing, for the profile.
    pub activities: Vec<Activity>,
    pub bot: bool,
}

/// What the profile endpoint adds to a user.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub banner: Option<String>,
    /// The banner's colour when there is no picture.
    pub accent: Option<Color32>,
    /// Nitro profile colours: the top and the bottom of the card.
    pub theme: Option<(Color32, Color32)>,
    pub badges: Vec<Badge>,
    /// When they joined the server the profile was asked from.
    pub joined: Option<jiff::Timestamp>,
    pub roles: Vec<Id>,
    pub mutual_guilds: Vec<Id>,
    pub mutual_friends: u32,
    pub connections: Vec<(String, String)>,
    pub note: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Badge {
    pub description: String,
    pub icon: String,
    pub link: Option<String>,
}

/// What the gateway says a user is up to.
#[derive(Clone, Debug, Default)]
pub struct Presence {
    pub status: Status,
    pub activities: Vec<Activity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ActivityKind {
    #[default]
    Playing,
    Streaming,
    Listening,
    Watching,
    Custom,
    Competing,
}

/// A game, a song, a stream or a custom status, with Rich Presence.
#[derive(Clone, Debug, Default)]
pub struct Activity {
    pub kind: ActivityKind,
    pub name: String,
    pub details: Option<String>,
    pub state: Option<String>,
    pub application: Option<Id>,
    /// Picture URLs, already resolved from Discord's asset keys.
    pub large_image: Option<String>,
    pub large_text: Option<String>,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
    /// Unix milliseconds.
    pub start: Option<i64>,
    pub end: Option<i64>,
    /// A custom status's emoji: Unicode text, or a custom emoji's picture.
    pub emoji: Option<String>,
    pub emoji_url: Option<String>,
    /// A stream's page.
    pub url: Option<String>,
}

impl Activity {
    pub fn is_spotify(&self) -> bool {
        self.kind == ActivityKind::Listening && self.name == "Spotify"
    }

    /// The short line Discord shows under a name in lists.
    pub fn line(&self) -> Option<String> {
        Some(match self.kind {
            ActivityKind::Playing => format!("Jogando {}", self.name),
            ActivityKind::Streaming => format!(
                "Transmitindo {}",
                self.details.as_deref().unwrap_or(&self.name)
            ),
            ActivityKind::Listening => format!("Ouvindo {}", self.name),
            ActivityKind::Watching => format!("Assistindo {}", self.name),
            ActivityKind::Competing => format!("Competindo em {}", self.name),
            ActivityKind::Custom => return self.state.clone().or_else(|| self.emoji.clone()),
        })
    }
}

impl User {
    pub fn set_presence(&mut self, presence: Presence) {
        self.status = presence.status;
        self.activity = presence.activities.iter().find_map(Activity::line);
        self.activities = presence.activities;
    }
}

impl User {
    /// The fill behind the initial while the picture loads.
    pub fn color(&self) -> Color32 {
        const COLORS: [u32; 6] = [0x5865f2, 0x757e8a, 0x3ba55c, 0xfaa61a, 0xed4245, 0xeb459e];
        let hex = COLORS[(self.id >> 22) as usize % COLORS.len()];
        Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }
}

#[derive(Clone, Debug)]
pub struct Role {
    pub id: Id,
    pub name: String,
    pub color: Option<Color32>,
    pub position: i64,
    pub permissions: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    Text,
    Voice,
    Category,
    Direct,
    Group,
    /// Forums, stages, threads and anything newer: listed, not opened.
    Other,
}

impl ChannelKind {
    pub fn from_type(kind: i64) -> Self {
        match kind {
            0 | 5 => ChannelKind::Text,
            2 | 13 => ChannelKind::Voice,
            4 => ChannelKind::Category,
            1 => ChannelKind::Direct,
            3 => ChannelKind::Group,
            _ => ChannelKind::Other,
        }
    }

    /// Opens to show messages.
    pub fn is_textual(self) -> bool {
        matches!(
            self,
            ChannelKind::Text | ChannelKind::Direct | ChannelKind::Group
        )
    }
}

#[derive(Clone, Debug)]
pub struct Overwrite {
    pub id: Id,
    /// 0 is a role, 1 a member.
    pub kind: i64,
    pub allow: u64,
    pub deny: u64,
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub id: Id,
    pub guild: Option<Id>,
    pub name: String,
    pub kind: ChannelKind,
    pub topic: Option<String>,
    pub position: i64,
    pub parent: Option<Id>,
    pub overwrites: Vec<Overwrite>,
    /// The newest message, to compare with what was read.
    pub last_message: Option<Id>,
    /// The newest message the user has read.
    pub read: Option<Id>,
    pub mentions: u32,
    /// Oldest first.
    pub messages: Vec<Message>,
    /// The history has been asked for.
    pub requested: bool,
    /// Older messages may exist before the first loaded one.
    pub more: bool,
    /// Older messages are on their way.
    pub loading: bool,
    /// For direct messages and groups.
    pub recipients: Vec<Id>,
}

impl Channel {
    pub fn unread(&self) -> bool {
        match (self.last_message, self.read) {
            (Some(last), Some(read)) => last > read,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub id: Id,
    /// Ours until the gateway echoes it back with an id.
    pub nonce: Option<String>,
    pub author: Id,
    pub time: jiff::Timestamp,
    /// The raw content, markdown and all.
    pub text: String,
    /// Discord's message type: 0 normal, 19 reply, 7 join, 6 pin, ...
    pub kind: i64,
    pub mentions_me: bool,
    pub edited: bool,
    /// Sent but not yet confirmed.
    pub pending: bool,
    pub failed: bool,
    /// A voice message: one audio attachment with a waveform.
    pub voice: bool,
    pub attachments: Vec<Attachment>,
    pub embeds: Vec<Embed>,
    pub stickers: Vec<Sticker>,
    pub reactions: Vec<Reaction>,
    /// The message this one answers.
    pub reply: Option<Reply>,
    /// The slash command a bot answered, as "/name".
    pub command: Option<String>,
}

impl Message {
    pub fn plain(author: Id, time: jiff::Timestamp, text: String) -> Self {
        Self {
            id: 0,
            nonce: None,
            author,
            time,
            text,
            kind: 0,
            mentions_me: false,
            edited: false,
            pending: false,
            failed: false,
            voice: false,
            attachments: Vec::new(),
            embeds: Vec::new(),
            stickers: Vec::new(),
            reactions: Vec::new(),
            reply: None,
            command: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Attachment {
    pub url: String,
    pub proxy: String,
    pub name: String,
    pub size: u64,
    pub content_type: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub spoiler: bool,
    pub duration: Option<f32>,
    /// A voice message's waveform: 0 to 255 per bar.
    pub waveform: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Gif,
    Video,
    Audio,
    File,
}

impl Attachment {
    pub fn kind(&self) -> MediaKind {
        let lower = self.name.to_lowercase();
        let content = self.content_type.as_deref().unwrap_or_default();
        if content == "image/gif" || lower.ends_with(".gif") {
            MediaKind::Gif
        } else if content.starts_with("image/")
            || [".png", ".jpg", ".jpeg", ".webp"]
                .iter()
                .any(|e| lower.ends_with(e))
        {
            MediaKind::Image
        } else if content.starts_with("video/")
            || [".mp4", ".webm", ".mov", ".mkv"]
                .iter()
                .any(|e| lower.ends_with(e))
        {
            MediaKind::Video
        } else if content.starts_with("audio/")
            || [".mp3", ".ogg", ".wav", ".flac", ".m4a", ".opus"]
                .iter()
                .any(|e| lower.ends_with(e))
        {
            MediaKind::Audio
        } else {
            MediaKind::File
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct EmbedMedia {
    pub url: String,
    /// Discord's media proxy, which can resize.
    pub proxy: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Embed {
    /// "rich", "image", "video", "gifv", "article", "link".
    pub kind: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<egui::Color32>,
    pub provider: Option<String>,
    pub author: Option<String>,
    pub author_url: Option<String>,
    pub author_icon: Option<String>,
    pub footer: Option<String>,
    pub footer_icon: Option<String>,
    pub timestamp: Option<jiff::Timestamp>,
    pub fields: Vec<EmbedField>,
    pub image: Option<EmbedMedia>,
    pub thumbnail: Option<EmbedMedia>,
    pub video: Option<EmbedMedia>,
}

impl Embed {
    pub fn is_youtube(&self) -> bool {
        self.provider.as_deref() == Some("YouTube")
            || self
                .url
                .as_deref()
                .is_some_and(|u| u.contains("youtube.com/") || u.contains("youtu.be/"))
    }
}

#[derive(Clone, Debug)]
pub struct Sticker {
    pub id: Id,
    pub name: String,
    /// 1 PNG, 2 APNG, 3 Lottie, 4 GIF.
    pub format: i64,
}

impl Sticker {
    pub fn url(&self) -> Option<String> {
        match self.format {
            1 | 2 => Some(format!(
                "https://media.discordapp.net/stickers/{}.png?size=160",
                self.id
            )),
            4 => Some(format!(
                "https://media.discordapp.net/stickers/{}.gif?size=160",
                self.id
            )),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReactionEmoji {
    Unicode(String),
    Custom {
        id: Id,
        name: String,
        animated: bool,
    },
}

impl ReactionEmoji {
    /// The form the reactions endpoint takes in its path.
    pub fn path(&self) -> String {
        match self {
            ReactionEmoji::Unicode(text) => percent_encode(text),
            ReactionEmoji::Custom { id, name, .. } => format!("{name}:{id}"),
        }
    }
}

fn percent_encode(text: &str) -> String {
    text.bytes().map(|b| format!("%{b:02X}")).collect()
}

#[derive(Clone, Debug)]
pub struct Reaction {
    pub emoji: ReactionEmoji,
    pub count: u32,
    pub me: bool,
}

#[derive(Clone, Debug)]
pub struct Reply {
    /// The quoted message, when Discord sent it along.
    pub author: Option<Id>,
    pub text: Option<String>,
    pub has_media: bool,
}

#[derive(Clone, Debug)]
pub struct CustomEmoji {
    pub id: Id,
    pub name: String,
    pub animated: bool,
}

impl CustomEmoji {
    pub fn url(id: Id, animated: bool) -> String {
        let ext = if animated { "gif" } else { "png" };
        format!("https://cdn.discordapp.com/emojis/{id}.{ext}?size=48")
    }
}

#[derive(Clone, Debug)]
pub struct Member {
    pub nick: Option<String>,
    pub roles: Vec<Id>,
}

/// A row of the member list, as the gateway's lazy list sends it.
#[derive(Clone, Debug)]
pub enum ListItem {
    Group { id: String, count: u32 },
    Member(Id),
}

#[derive(Clone, Debug, Default)]
pub struct Guild {
    pub id: Id,
    pub name: String,
    pub icon: Option<String>,
    pub owner: Id,
    /// Highest position first.
    pub roles: Vec<Role>,
    pub members: HashMap<Id, Member>,
    pub channels: Vec<Id>,
    pub list: Vec<ListItem>,
    /// The member list has been asked for.
    pub subscribed: bool,
    pub emojis: Vec<CustomEmoji>,
    pub stickers: Vec<Sticker>,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub me: Id,
    pub users: HashMap<Id, User>,
    /// In the rail's order.
    pub guilds: Vec<Guild>,
    pub channels: HashMap<Id, Channel>,
    /// Direct messages and groups.
    pub direct: Vec<Id>,
    /// 0 none, 1 Nitro Classic, 2 Nitro, 3 Nitro Basic.
    pub premium: u8,
    pub account: Account,
    /// Friends, blocked people and requests, by Discord's type
    /// (`nav::relationship`).
    pub relationships: HashMap<Id, u8>,
    /// The rail: servers alone and in folders, in the user's order.
    pub folders: Vec<Folder>,
}

/// A folder of servers in the rail; one without an id is a server alone.
#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    pub id: Option<u64>,
    pub name: Option<String>,
    /// 0xRRGGBB.
    pub color: Option<u32>,
    pub guilds: Vec<Id>,
}

impl Folder {
    pub fn single(guild: Id) -> Self {
        Self {
            id: None,
            name: None,
            color: None,
            guilds: vec![guild],
        }
    }
}

/// The signed-in account's private details, from READY.
#[derive(Clone, Debug, Default)]
pub struct Account {
    pub email: Option<String>,
    pub phone: Option<String>,
    pub mfa: bool,
    pub verified: bool,
}

const VIEW_CHANNEL: u64 = 1 << 10;

/// Discord's permission bits that the interface checks.
pub mod permission {
    pub const CREATE_INVITE: u64 = 1 << 0;
    pub const KICK: u64 = 1 << 1;
    pub const BAN: u64 = 1 << 2;
    pub const ADMINISTRATOR: u64 = 1 << 3;
    pub const MANAGE_CHANNELS: u64 = 1 << 4;
    pub const MANAGE_GUILD: u64 = 1 << 5;
    pub const VIEW_AUDIT_LOG: u64 = 1 << 7;
    pub const CHANGE_NICKNAME: u64 = 1 << 26;
    pub const MANAGE_ROLES: u64 = 1 << 28;
    pub const MANAGE_EMOJIS: u64 = 1 << 30;
}

impl Model {
    /// Makes the rail's folders hold every server once, and the server
    /// list follow their order. New servers go to the top, as Discord's.
    pub fn settle_folders(&mut self) {
        let known: std::collections::HashSet<Id> = self.guilds.iter().map(|g| g.id).collect();
        let mut seen = std::collections::HashSet::new();
        for folder in &mut self.folders {
            folder
                .guilds
                .retain(|g| known.contains(g) && seen.insert(*g));
        }
        self.folders.retain(|f| !f.guilds.is_empty());
        let missing: Vec<Folder> = self
            .guilds
            .iter()
            .filter(|g| !seen.contains(&g.id))
            .map(|g| Folder::single(g.id))
            .collect();
        self.folders.splice(0..0, missing);
        let order: Vec<Id> = self.folders.iter().flat_map(|f| f.guilds.clone()).collect();
        self.guilds.sort_by_key(|g| {
            order
                .iter()
                .position(|id| *id == g.id)
                .unwrap_or(usize::MAX)
        });
    }

    /// The longest message the user may send.
    pub fn max_message(&self) -> usize {
        if self.premium == 2 { 4000 } else { 2000 }
    }

    /// The largest upload, in bytes, for the user's plan.
    pub fn max_upload(&self) -> u64 {
        match self.premium {
            2 => 500 << 20,
            1 | 3 => 50 << 20,
            _ => 10 << 20,
        }
    }
}
const ADMINISTRATOR: u64 = 1 << 3;

impl Model {
    pub fn user(&self, id: Id) -> Option<&User> {
        self.users.get(&id)
    }

    pub fn channel(&self, id: Id) -> Option<&Channel> {
        self.channels.get(&id)
    }

    pub fn guild(&self, id: Id) -> Option<&Guild> {
        self.guilds.iter().find(|g| g.id == id)
    }

    pub fn guild_mut(&mut self, id: Id) -> Option<&mut Guild> {
        self.guilds.iter_mut().find(|g| g.id == id)
    }

    /// The name shown on a server: the nickname, else the display name.
    pub fn name_in(&self, guild: Option<Id>, user: Id) -> String {
        let nick = guild
            .and_then(|g| self.guild(g))
            .and_then(|g| g.members.get(&user))
            .and_then(|m| m.nick.clone());
        nick.or_else(|| self.user(user).map(|u| u.name.clone()))
            .unwrap_or_else(|| "Desconhecido".into())
    }

    /// The colour a name is drawn in on a server: its highest coloured role.
    pub fn name_color(&self, guild: Option<Id>, user: Id) -> Option<Color32> {
        let guild = self.guild(guild?)?;
        let member = guild.members.get(&user)?;
        guild
            .roles
            .iter()
            .filter(|r| member.roles.contains(&r.id))
            .find_map(|r| r.color)
    }

    /// The name of a direct message: the other person, or the group.
    pub fn direct_name(&self, channel: &Channel) -> String {
        if !channel.name.is_empty() {
            return channel.name.clone();
        }
        let names: Vec<String> = channel
            .recipients
            .iter()
            .filter_map(|id| self.user(*id).map(|u| u.name.clone()))
            .collect();
        if names.is_empty() {
            "Grupo sem nome".into()
        } else {
            names.join(", ")
        }
    }

    /// What the user may do on a server, from its roles; everything for the
    /// owner and administrators.
    pub fn guild_permissions(&self, guild: Id) -> u64 {
        let Some(g) = self.guild(guild) else { return 0 };
        if g.owner == self.me {
            return u64::MAX;
        }
        let roles = g
            .members
            .get(&self.me)
            .map(|m| m.roles.clone())
            .unwrap_or_default();
        let mut bits = g
            .roles
            .iter()
            .find(|r| r.id == g.id)
            .map_or(0, |r| r.permissions);
        for role in g.roles.iter().filter(|r| roles.contains(&r.id)) {
            bits |= role.permissions;
        }
        if bits & permission::ADMINISTRATOR != 0 {
            u64::MAX
        } else {
            bits
        }
    }

    pub fn can(&self, guild: Id, bit: u64) -> bool {
        self.guild_permissions(guild) & bit != 0
    }

    /// Whether the user may see a channel: the guild's roles, then the
    /// channel's overwrites for @everyone, the roles and the member.
    pub fn can_view(&self, channel: &Channel) -> bool {
        let Some(guild) = channel.guild.and_then(|g| self.guild(g)) else {
            return true;
        };
        if guild.owner == self.me {
            return true;
        }
        let Some(member) = guild.members.get(&self.me) else {
            return true;
        };
        let everyone = guild
            .roles
            .iter()
            .find(|r| r.id == guild.id)
            .map_or(0, |r| r.permissions);
        let mut permissions = everyone;
        for role in guild.roles.iter().filter(|r| member.roles.contains(&r.id)) {
            permissions |= role.permissions;
        }
        if permissions & ADMINISTRATOR != 0 {
            return true;
        }
        if let Some(o) = channel.overwrites.iter().find(|o| o.id == guild.id) {
            permissions = (permissions & !o.deny) | o.allow;
        }
        let (mut allow, mut deny) = (0, 0);
        for o in channel
            .overwrites
            .iter()
            .filter(|o| o.kind == 0 && member.roles.contains(&o.id))
        {
            allow |= o.allow;
            deny |= o.deny;
        }
        permissions = (permissions & !deny) | allow;
        if let Some(o) = channel
            .overwrites
            .iter()
            .find(|o| o.kind == 1 && o.id == self.me)
        {
            permissions = (permissions & !o.deny) | o.allow;
        }
        permissions & VIEW_CHANNEL != 0
    }

    /// A guild's visible channels in Discord's order: channels outside a
    /// category, then each category with its channels; text before voice.
    pub fn layout(&self, guild: &Guild) -> (Vec<Id>, Vec<(Id, Vec<Id>)>) {
        let rank = |c: &Channel| (c.kind == ChannelKind::Voice, c.position, c.id);
        let mut loose: Vec<&Channel> = Vec::new();
        let mut categories: Vec<&Channel> = Vec::new();
        let mut children: HashMap<Id, Vec<&Channel>> = HashMap::new();
        for channel in guild.channels.iter().filter_map(|id| self.channel(*id)) {
            if channel.kind == ChannelKind::Category {
                categories.push(channel);
            } else if !self.can_view(channel) {
            } else if let Some(parent) = channel.parent {
                children.entry(parent).or_default().push(channel);
            } else {
                loose.push(channel);
            }
        }
        loose.sort_by_key(|c| rank(c));
        categories.sort_by_key(|c| (c.position, c.id));
        let loose = loose.into_iter().map(|c| c.id).collect();
        let categories = categories
            .into_iter()
            .filter_map(|category| {
                let mut list = children.remove(&category.id)?;
                list.sort_by_key(|c| rank(c));
                Some((category.id, list.into_iter().map(|c| c.id).collect()))
            })
            .collect();
        (loose, categories)
    }

    /// The first channel to open on a server.
    pub fn first_text_channel(&self, guild: Id) -> Option<Id> {
        let guild = self.guild(guild)?;
        let (loose, categories) = self.layout(guild);
        loose
            .into_iter()
            .chain(categories.into_iter().flat_map(|(_, list)| list))
            .find(|id| self.channel(*id).is_some_and(|c| c.kind.is_textual()))
    }

    /// Unread dot and mention count for the rail.
    pub fn guild_unread(&self, guild: &Guild) -> (bool, u32) {
        guild
            .channels
            .iter()
            .filter_map(|id| self.channel(*id))
            .filter(|c| c.kind.is_textual() && self.can_view(c))
            .fold((false, 0), |(unread, mentions), c| {
                (unread || c.unread(), mentions + c.mentions)
            })
    }

    /// Direct messages, the most recent conversation first.
    pub fn sorted_direct(&self) -> Vec<Id> {
        let mut list = self.direct.clone();
        list.sort_by_key(|id| {
            std::cmp::Reverse(
                self.channel(*id)
                    .and_then(|c| c.last_message)
                    .unwrap_or(*id),
            )
        });
        list
    }
}
