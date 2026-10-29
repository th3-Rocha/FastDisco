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
    /// The name Discord uses; offline is how invisible looks.
    pub fn name(self) -> &'static str {
        match self {
            Status::Online => "online",
            Status::Idle => "idle",
            Status::DoNotDisturb => "dnd",
            Status::Offline => "invisible",
        }
    }

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
    /// A thread of a text channel, or a post in a forum.
    Thread,
    /// A forum: posts, each a thread, with tags.
    Forum,
    /// A media channel: a forum shown as a grid of pictures.
    Media,
    /// A stage: speakers on the stage and an audience.
    Stage,
    /// Anything newer: listed, not opened.
    Other,
}

impl ChannelKind {
    pub fn from_type(kind: i64) -> Self {
        match kind {
            0 | 5 => ChannelKind::Text,
            2 => ChannelKind::Voice,
            13 => ChannelKind::Stage,
            4 => ChannelKind::Category,
            1 => ChannelKind::Direct,
            3 => ChannelKind::Group,
            10..=12 => ChannelKind::Thread,
            15 => ChannelKind::Forum,
            16 => ChannelKind::Media,
            _ => ChannelKind::Other,
        }
    }

    /// Opens to show messages.
    pub fn is_textual(self) -> bool {
        matches!(
            self,
            ChannelKind::Text | ChannelKind::Direct | ChannelKind::Group | ChannelKind::Thread
        )
    }

    /// Joined to talk: voice channels and stages.
    pub fn is_voice(self) -> bool {
        matches!(self, ChannelKind::Voice | ChannelKind::Stage)
    }

    /// Holds posts rather than messages.
    pub fn is_forum(self) -> bool {
        matches!(self, ChannelKind::Forum | ChannelKind::Media)
    }
}

/// What a thread or a forum post adds to a channel.
#[derive(Clone, Debug, Default)]
pub struct ThreadMeta {
    pub archived: bool,
    pub locked: bool,
    pub messages: u32,
    pub members: u32,
    pub owner: Option<Id>,
    /// The user follows it: it shows under its channel in the list.
    pub joined: bool,
    /// A forum post's tags.
    pub tags: Vec<Id>,
}

/// A forum's tag, which posts carry.
#[derive(Clone, Debug)]
pub struct ForumTag {
    pub id: Id,
    pub name: String,
    pub emoji: Option<String>,
    /// Only moderators may put it on a post.
    pub moderated: bool,
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
    /// Discord refused the history: the channel is age-restricted and the
    /// account has not confirmed its age.
    pub age_gated: bool,
    /// A voice channel's status, the line under its name.
    pub voice_status: Option<String>,
    /// When the call in a voice channel started, in Unix seconds.
    pub voice_start: Option<u64>,
    /// A group's own picture.
    pub icon: Option<String>,
    /// A stage's topic while it is live.
    pub stage_topic: Option<String>,
    /// A thread's or a forum post's details.
    pub thread: Option<ThreadMeta>,
    /// A forum's tags.
    pub tags: Vec<ForumTag>,
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
    /// Discord's message flags: 64 is only visible to the user.
    pub flags: u64,
    /// The bot behind the message, for its buttons and menus.
    pub application: Option<Id>,
    /// A message forwarded from elsewhere, shown inside this one.
    pub forward: Option<Box<Forward>>,
    pub poll: Option<Poll>,
    /// Bots' buttons, menus and layout.
    pub components: Vec<Component>,
    pub pinned: bool,
    /// The thread this message started, or that a "started a thread"
    /// line points to.
    pub thread: Option<ThreadLink>,
}

/// A thread named from a message.
#[derive(Clone, Debug)]
pub struct ThreadLink {
    pub id: Id,
    pub name: String,
    pub messages: u32,
}

/// What a forwarded message carried, and where it came from.
#[derive(Clone, Debug)]
pub struct Forward {
    pub text: String,
    pub time: Option<jiff::Timestamp>,
    pub attachments: Vec<Attachment>,
    pub embeds: Vec<Embed>,
    pub guild: Option<Id>,
    pub channel: Option<Id>,
    pub message: Option<Id>,
}

#[derive(Clone, Debug)]
pub struct Poll {
    pub question: String,
    pub answers: Vec<PollAnswer>,
    pub multiple: bool,
    pub expiry: Option<jiff::Timestamp>,
    pub finalized: bool,
}

#[derive(Clone, Debug)]
pub struct PollAnswer {
    pub id: u32,
    pub text: String,
    pub emoji: Option<ReactionEmoji>,
    pub count: u32,
    pub me: bool,
}

impl Poll {
    pub fn total(&self) -> u32 {
        self.answers.iter().map(|a| a.count).sum()
    }

    pub fn voted(&self) -> bool {
        self.answers.iter().any(|a| a.me)
    }

    pub fn closed(&self) -> bool {
        self.finalized || self.expiry.is_some_and(|e| e <= jiff::Timestamp::now())
    }
}

/// A bot's message component, from the first kind (rows of buttons and
/// menus) and the second (text, sections, containers, galleries).
#[derive(Clone, Debug)]
pub enum Component {
    Row(Vec<Component>),
    Button(Button),
    Select(Select),
    Text(String),
    Section {
        texts: Vec<String>,
        accessory: Option<Box<Component>>,
    },
    Thumbnail(String),
    Gallery(Vec<String>),
    File(String),
    Separator {
        divider: bool,
        large: bool,
    },
    Container {
        accent: Option<Color32>,
        children: Vec<Component>,
    },
}

#[derive(Clone, Debug)]
pub struct Button {
    /// 1 primary, 2 secondary, 3 success, 4 danger, 5 link, 6 premium.
    pub style: u8,
    pub label: Option<String>,
    pub emoji: Option<ReactionEmoji>,
    pub custom_id: Option<String>,
    pub url: Option<String>,
    pub disabled: bool,
}

#[derive(Clone, Debug)]
pub struct Select {
    /// 3 text, 5 user, 6 role, 7 mentionable, 8 channel.
    pub kind: u8,
    pub custom_id: String,
    pub placeholder: Option<String>,
    pub options: Vec<SelectOption>,
    pub min: u32,
    pub max: u32,
    pub disabled: bool,
}

#[derive(Clone, Debug)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    pub description: Option<String>,
    pub emoji: Option<ReactionEmoji>,
    pub default: bool,
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
            flags: 0,
            application: None,
            forward: None,
            poll: None,
            components: Vec::new(),
            pinned: false,
            thread: None,
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
    /// Scheduled events that have not ended.
    pub events: Vec<Event>,
}

/// A server's scheduled event.
#[derive(Clone, Debug)]
pub struct Event {
    pub id: Id,
    pub name: String,
    /// Happening now, rather than still to come.
    pub live: bool,
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
    /// The gateway session, which interactions name.
    pub session: String,
    /// Who is in which voice channel: by server (none for direct message
    /// calls) and person.
    pub voice: HashMap<(Option<Id>, Id), VoiceMember>,
}

/// Someone in a voice channel, and how.
#[derive(Clone, Debug, Default)]
pub struct VoiceMember {
    pub channel: Id,
    pub self_mute: bool,
    pub self_deaf: bool,
    /// Muted or deafened by the server.
    pub mute: bool,
    pub deaf: bool,
    /// Sharing their screen: "AO VIVO" beside the name.
    pub stream: bool,
    /// Their camera is on.
    pub video: bool,
    /// On a stage: in the audience rather than speaking.
    pub suppress: bool,
    /// In a stage's audience with a hand raised.
    pub hand: bool,
}

impl Model {
    /// The people in a voice channel, in the order they would be listed.
    pub fn in_voice(&self, channel: Id) -> Vec<(Id, VoiceMember)> {
        let mut list: Vec<(Id, VoiceMember)> = self
            .voice
            .iter()
            .filter(|(_, v)| v.channel == channel)
            .map(|((_, user), v)| (*user, v.clone()))
            .collect();
        list.sort_by_cached_key(|(user, _)| {
            self.user(*user)
                .map(|u| u.name.to_lowercase())
                .unwrap_or_default()
        });
        list
    }
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
    /// Discord says the account sees age-restricted content. Only "yes"
    /// means anything: Discord answers "no" to clients it does not know,
    /// adult accounts included.
    pub adult: Option<bool>,
    /// Discord's account settings as it sends them (who may send DMs and
    /// friend requests, the mobile notification delay, /tts...), changed
    /// with PATCH /users/@me/settings.
    pub settings: serde_json::Value,
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
    pub const MUTE_MEMBERS: u64 = 1 << 22;
    pub const DEAFEN_MEMBERS: u64 = 1 << 23;
    pub const MOVE_MEMBERS: u64 = 1 << 24;
    pub const CHANGE_NICKNAME: u64 = 1 << 26;
    pub const MANAGE_ROLES: u64 = 1 << 28;
    pub const MANAGE_EMOJIS: u64 = 1 << 30;
    pub const MANAGE_EVENTS: u64 = 1 << 33;
    pub const MANAGE_THREADS: u64 = 1 << 34;
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

    /// Whether the user may see a channel.
    pub fn can_view(&self, channel: &Channel) -> bool {
        // Before our own member entry arrives, nothing is hidden.
        let known = channel
            .guild
            .and_then(|g| self.guild(g))
            .is_none_or(|g| g.members.contains_key(&self.me));
        !known || self.channel_permissions(channel) & VIEW_CHANNEL != 0
    }

    /// What the user may do in a channel, as Discord works it out: the
    /// guild's roles, then the channel's overwrites for @everyone, the
    /// roles and the member. Everything for the owner and administrators,
    /// and outside a server.
    pub fn channel_permissions(&self, channel: &Channel) -> u64 {
        let Some(guild) = channel.guild.and_then(|g| self.guild(g)) else {
            return u64::MAX;
        };
        if guild.owner == self.me {
            return u64::MAX;
        }
        let roles = guild
            .members
            .get(&self.me)
            .map(|m| m.roles.clone())
            .unwrap_or_default();
        let everyone = guild
            .roles
            .iter()
            .find(|r| r.id == guild.id)
            .map_or(0, |r| r.permissions);
        let mut permissions = everyone;
        for role in guild.roles.iter().filter(|r| roles.contains(&r.id)) {
            permissions |= role.permissions;
        }
        if permissions & ADMINISTRATOR != 0 {
            return u64::MAX;
        }
        if let Some(o) = channel.overwrites.iter().find(|o| o.id == guild.id) {
            permissions = (permissions & !o.deny) | o.allow;
        }
        let (mut allow, mut deny) = (0, 0);
        for o in channel
            .overwrites
            .iter()
            .filter(|o| o.kind == 0 && roles.contains(&o.id))
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
        permissions
    }

    /// A guild's visible channels in Discord's order: channels outside a
    /// category, then each category with its channels; text before voice.
    pub fn layout(&self, guild: &Guild) -> (Vec<Id>, Vec<(Id, Vec<Id>)>) {
        let rank = |c: &Channel| (c.kind.is_voice(), c.position, c.id);
        let mut loose: Vec<&Channel> = Vec::new();
        let mut categories: Vec<&Channel> = Vec::new();
        let mut children: HashMap<Id, Vec<&Channel>> = HashMap::new();
        for channel in guild.channels.iter().filter_map(|id| self.channel(*id)) {
            if channel.kind == ChannelKind::Category {
                categories.push(channel);
            } else if channel.kind == ChannelKind::Thread || !self.can_view(channel) {
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

    /// The threads the user follows under a channel, newest first, as
    /// the channel list shows them.
    pub fn joined_threads(&self, parent: Id) -> Vec<Id> {
        let Some(guild) = self.channel(parent).and_then(|c| c.guild).and_then(|g| self.guild(g))
        else {
            return Vec::new();
        };
        let mut list: Vec<Id> = guild
            .channels
            .iter()
            .filter_map(|id| self.channel(*id))
            .filter(|c| {
                c.parent == Some(parent)
                    && c.thread.as_ref().is_some_and(|t| t.joined && !t.archived)
            })
            .map(|c| c.id)
            .collect();
        list.sort_by_key(|id| std::cmp::Reverse(*id));
        list
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
