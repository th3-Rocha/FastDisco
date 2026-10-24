//! What the interface shows: servers, channels, people and messages. Until
//! the gateway lands, `Model::sample` fills it with a believable server.

use egui::Color32;

pub type UserId = usize;
pub type ServerId = usize;
pub type ChannelId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Online,
    Idle,
    DoNotDisturb,
    Offline,
}

#[derive(Clone, Debug)]
pub struct User {
    pub name: String,
    pub username: String,
    /// The fill behind the initial, until avatars are downloaded.
    pub avatar: Color32,
    /// The avatar picture's URI, once it is loaded.
    pub picture: Option<String>,
    pub status: Status,
    /// "Playing ..." or a custom status, under the name in lists.
    pub activity: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Role {
    pub name: String,
    pub color: Color32,
    /// Shown as its own group in the member list.
    pub hoisted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    Text,
    Voice,
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub name: String,
    pub kind: ChannelKind,
    pub topic: Option<String>,
    pub unread: bool,
    pub mentions: u32,
    pub messages: Vec<Message>,
    /// Who is connected, for a voice channel.
    pub voice: Vec<UserId>,
}

#[derive(Clone, Debug)]
pub struct Category {
    pub name: String,
    pub channels: Vec<ChannelId>,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub user: UserId,
    /// The highest role, which colours the name and groups the member.
    pub role: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Server {
    pub name: String,
    pub icon: Color32,
    pub unread: bool,
    pub mentions: u32,
    pub roles: Vec<Role>,
    pub members: Vec<Member>,
    /// Channels outside a category come first.
    pub loose: Vec<ChannelId>,
    pub categories: Vec<Category>,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub author: UserId,
    /// Minutes since midnight today; negative is yesterday.
    pub minute: i32,
    pub text: String,
    pub mentions_me: bool,
}

#[derive(Clone, Debug)]
pub struct DirectMessage {
    pub user: UserId,
    pub unread: u32,
    pub channel: ChannelId,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub me: UserId,
    /// Where the window opens: the last server and channel used.
    pub start: (ServerId, ChannelId),
    pub users: Vec<User>,
    pub servers: Vec<Server>,
    pub channels: Vec<Channel>,
    pub direct: Vec<DirectMessage>,
}

impl Model {
    pub fn user(&self, id: UserId) -> &User {
        &self.users[id]
    }

    pub fn channel(&self, id: ChannelId) -> &Channel {
        &self.channels[id]
    }

    /// The colour a name is drawn in on a server: its highest coloured role.
    pub fn name_color(&self, server: Option<ServerId>, user: UserId) -> Option<Color32> {
        let server = &self.servers[server?];
        let member = server.members.iter().find(|m| m.user == user)?;
        Some(server.roles[member.role?].color)
    }

    /// The first text channel of a server, to open when it is chosen.
    pub fn first_text_channel(&self, server: ServerId) -> Option<ChannelId> {
        let server = &self.servers[server];
        server
            .loose
            .iter()
            .chain(server.categories.iter().flat_map(|c| c.channels.iter()))
            .copied()
            .find(|&id| self.channels[id].kind == ChannelKind::Text)
    }

    pub fn sample() -> Self {
        let mut b = Builder::default();
        let me = b.user("Murilo", "th3rocha", 0x5865f2, Status::Online, None);
        let ana = b.user(
            "Ana",
            "anabits",
            0xeb459e,
            Status::Online,
            Some("Playing Hades II"),
        );
        let caio = b.user("Caio", "caiodev", 0x3ba55c, Status::Idle, None);
        let bia = b.user(
            "Bia",
            "biaxyz",
            0xfaa61a,
            Status::DoNotDisturb,
            Some("focado no TCC"),
        );
        let leo = b.user(
            "Leo",
            "leo.rs",
            0xed4245,
            Status::Online,
            Some("Listening to Spotifast"),
        );
        let rafa = b.user("Rafa", "rafinha", 0x9b59b6, Status::Offline, None);
        let duda = b.user("Duda", "dudinha", 0x1abc9c, Status::Offline, None);
        let ferris = b.user(
            "Ferris",
            "ferris",
            0xe67e22,
            Status::Online,
            Some("cargo build --release"),
        );

        let general = b.channel(
            "geral",
            ChannelKind::Text,
            Some("Papo livre. Seja gente boa."),
            vec![
                msg(ana, -70, "alguém viu o update novo do egui?"),
                msg(leo, -68, "vi! o 0.36 ficou bem mais liso no wayland"),
                msg(
                    leo,
                    -67,
                    "e o fastframe já cuida de tray, tema e notificação",
                ),
                msg(caio, 545, "bom dia povo"),
                msg(bia, 552, "bom diaaa"),
                msg(ana, 601, "@Murilo bora testar o FastDisco hoje?"),
                msg(me, 604, "bora, já tô com o layout pronto"),
                msg(
                    me,
                    604,
                    "servidores na esquerda, canais, chat e membros na direita, igualzinho",
                ),
                msg(ferris, 612, "compila em release que abre em menos de 1s 🦀"),
                msg(leo, 615, "e sem Electron, só pra deixar registrado"),
            ],
        );
        let mut mention = b.channels[general].messages[5].clone();
        mention.mentions_me = true;
        b.channels[general].messages[5] = mention;
        let rules = b.channel(
            "regras",
            ChannelKind::Text,
            Some("Leia antes de postar"),
            vec![msg(
                ferris,
                -900,
                "1. Respeito acima de tudo.\n2. Sem spam.\n3. Memes no canal certo.",
            )],
        );
        let memes = b.channel(
            "memes",
            ChannelKind::Text,
            None,
            vec![
                msg(caio, 400, "quando o borrow checker aceita de primeira"),
                msg(ana, 402, "isso nunca aconteceu"),
            ],
        );
        b.channels[memes].unread = true;
        let rust = b.channel(
            "rust",
            ChannelKind::Text,
            Some("Tudo sobre Rust"),
            vec![
                msg(ferris, 500, "alguém já usou twilight pro gateway?"),
                msg(leo, 503, "@Murilo usou, pergunta pra ele"),
            ],
        );
        b.channels[rust].unread = true;
        b.channels[rust].mentions = 1;
        b.channels[rust].messages[1].mentions_me = true;
        let egui_ch = b.channel("egui", ChannelKind::Text, None, vec![]);
        let lobby = b.channel("Lobby", ChannelKind::Voice, None, vec![]);
        b.channels[lobby].voice = vec![ana, leo];
        let games = b.channel("Jogando", ChannelKind::Voice, None, vec![]);

        let rust_br = Server {
            name: "Rust Brasil".into(),
            icon: Color32::from_rgb(0xce, 0x42, 0x2b),
            unread: true,
            mentions: 1,
            roles: vec![
                Role {
                    name: "Admin".into(),
                    color: Color32::from_rgb(0xed, 0x42, 0x45),
                    hoisted: true,
                },
                Role {
                    name: "Rustacean".into(),
                    color: Color32::from_rgb(0xe6, 0x7e, 0x22),
                    hoisted: true,
                },
                Role {
                    name: "Membro".into(),
                    color: Color32::from_rgb(0x34, 0x98, 0xdb),
                    hoisted: false,
                },
            ],
            members: vec![
                Member {
                    user: ferris,
                    role: Some(0),
                },
                Member {
                    user: me,
                    role: Some(1),
                },
                Member {
                    user: leo,
                    role: Some(1),
                },
                Member {
                    user: ana,
                    role: Some(2),
                },
                Member {
                    user: caio,
                    role: None,
                },
                Member {
                    user: bia,
                    role: Some(2),
                },
                Member {
                    user: rafa,
                    role: Some(1),
                },
                Member {
                    user: duda,
                    role: None,
                },
            ],
            loose: vec![rules],
            categories: vec![
                Category {
                    name: "Canais de texto".into(),
                    channels: vec![general, memes, rust, egui_ch],
                },
                Category {
                    name: "Canais de voz".into(),
                    channels: vec![lobby, games],
                },
            ],
        };

        let mut servers = vec![rust_br];
        for (name, color, unread) in [
            ("Omarchy", 0x2f3136, false),
            ("Linux Gamers", 0x5865f2, true),
            ("Faculdade", 0x3ba55c, false),
            ("Tauri Devs", 0x24c8db, false),
        ] {
            let general = b.channel("geral", ChannelKind::Text, None, vec![]);
            let voice = b.channel("Geral", ChannelKind::Voice, None, vec![]);
            servers.push(Server {
                name: name.into(),
                icon: Color32::from_rgb((color >> 16) as u8, (color >> 8) as u8, color as u8),
                unread,
                mentions: 0,
                roles: vec![],
                members: vec![Member {
                    user: me,
                    role: None,
                }],
                loose: vec![],
                categories: vec![
                    Category {
                        name: "Canais de texto".into(),
                        channels: vec![general],
                    },
                    Category {
                        name: "Canais de voz".into(),
                        channels: vec![voice],
                    },
                ],
            });
        }

        let dm_ana = b.channel(
            "ana",
            ChannelKind::Text,
            None,
            vec![
                msg(ana, 590, "viu o print que te mandei?"),
                msg(me, 592, "vi kkkk"),
            ],
        );
        let dm_leo = b.channel(
            "leo",
            ChannelKind::Text,
            None,
            vec![msg(leo, 300, "manda o link do repo")],
        );
        let dm_bia = b.channel("bia", ChannelKind::Text, None, vec![]);
        let direct = vec![
            DirectMessage {
                user: ana,
                unread: 0,
                channel: dm_ana,
            },
            DirectMessage {
                user: leo,
                unread: 2,
                channel: dm_leo,
            },
            DirectMessage {
                user: bia,
                unread: 0,
                channel: dm_bia,
            },
        ];

        Model {
            me,
            start: (0, general),
            users: b.users,
            servers,
            channels: b.channels,
            direct,
        }
    }
}

fn msg(author: UserId, minute: i32, text: &str) -> Message {
    Message {
        author,
        minute,
        text: text.into(),
        mentions_me: false,
    }
}

#[derive(Default)]
struct Builder {
    users: Vec<User>,
    channels: Vec<Channel>,
}

impl Builder {
    fn user(
        &mut self,
        name: &str,
        username: &str,
        avatar: u32,
        status: Status,
        activity: Option<&str>,
    ) -> UserId {
        self.users.push(User {
            name: name.into(),
            username: username.into(),
            avatar: Color32::from_rgb((avatar >> 16) as u8, (avatar >> 8) as u8, avatar as u8),
            picture: None,
            status,
            activity: activity.map(Into::into),
        });
        self.users.len() - 1
    }

    fn channel(
        &mut self,
        name: &str,
        kind: ChannelKind,
        topic: Option<&str>,
        messages: Vec<Message>,
    ) -> ChannelId {
        self.channels.push(Channel {
            name: name.into(),
            kind,
            topic: topic.map(Into::into),
            unread: false,
            mentions: 0,
            messages,
            voice: vec![],
        });
        self.channels.len() - 1
    }
}
