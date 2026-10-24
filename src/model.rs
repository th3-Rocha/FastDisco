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
    /// "Jogando ...", or the custom status.
    pub activity: Option<String>,
    pub bot: bool,
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
    pub text: String,
    pub mentions_me: bool,
    pub edited: bool,
    /// Sent but not yet confirmed.
    pub pending: bool,
    pub failed: bool,
    /// Names of files, until attachments are drawn.
    pub attachments: Vec<String>,
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
}

const VIEW_CHANNEL: u64 = 1 << 10;
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
