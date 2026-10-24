//! What the conversation does beyond showing messages: replying, editing,
//! deleting, reactions, files to send, the emoji, GIF and sticker picker,
//! typing, older history, media playback and the dialogs Discord shows
//! about limits.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::app::App;
use crate::backend::Command;
use crate::discord::{parse, rest};
use crate::media;
use crate::model::{Attachment, Id, Message, Reaction, ReactionEmoji, Sticker};

/// How long "is typing" lasts without news.
const TYPING: Duration = Duration::from_secs(10);
/// How often our own typing is announced.
const TYPING_EVERY: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerTab {
    Gif,
    Sticker,
    Emoji,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerTarget {
    Composer,
    Reaction { channel: Id, message: Id },
}

/// What the GIF tab shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GifView {
    /// The tiles: favourites, trending and the trending searches.
    Home,
    Trending,
    Favorites,
    /// The results for the query (typed, or a tile's search).
    Search,
}

pub struct Picker {
    pub gif_view: GifView,
    pub tab: PickerTab,
    pub target: PickerTarget,
    pub query: String,
    /// The GIF query last sent, to search again only when it changes.
    pub searched: Option<String>,
}

pub struct Editing {
    pub channel: Id,
    pub message: Id,
    pub text: String,
    pub focus: bool,
}

/// A file waiting in the composer.
#[derive(Clone, Debug)]
pub struct Pending {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub spoiler: bool,
}

impl Pending {
    pub fn is_image(&self) -> bool {
        let lower = self.name.to_lowercase();
        [".png", ".jpg", ".jpeg", ".gif", ".webp"]
            .iter()
            .any(|e| lower.ends_with(e))
    }
}

pub enum Dialog {
    /// The message is over the limit: offer to send it as a file.
    TooLong {
        channel: Id,
        length: usize,
    },
    TooLarge {
        names: Vec<String>,
    },
    Delete {
        channel: Id,
        message: Id,
    },
    Failed(String),
}

#[derive(Clone, Debug)]
pub enum ChatAction {
    Reply {
        channel: Id,
        message: Id,
    },
    CancelReply,
    StartEdit {
        channel: Id,
        message: Id,
    },
    /// Up arrow in an empty composer: edit the last own message.
    EditLast,
    SaveEdit,
    CancelEdit,
    AskDelete {
        channel: Id,
        message: Id,
    },
    /// Shift-click deletes without asking, as in Discord.
    Delete {
        channel: Id,
        message: Id,
    },
    ConfirmDelete,
    React {
        channel: Id,
        message: Id,
        emoji: ReactionEmoji,
        add: bool,
    },
    OpenPicker {
        tab: PickerTab,
        target: PickerTarget,
    },
    PickerTab(PickerTab),
    ClosePicker,
    /// Text for the composer, or an emoji for a reaction.
    PickEmoji {
        text: String,
        reaction: ReactionEmoji,
    },
    PickGif(String),
    /// Searches GIFs; empty goes back to the tiles.
    SearchGifs(String),
    GifView(GifView),
    /// Adds or removes a favourite GIF.
    Favorite {
        gif: crate::discord::frecency::Favorite,
        add: bool,
    },
    PickSticker(Id),
    AttachFiles,
    AddFiles(Vec<PathBuf>),
    RemoveUpload(usize),
    ToggleSpoiler(usize),
    SendAsFile,
    CloseDialog,
    LoadOlder(Id),
    Typed,
    Play {
        key: String,
        source: String,
    },
    PlayerToggle,
    PlayerSeek(f64),
    PlayerMute,
    ViewImage(String),
    CloseViewer,
    RevealSpoiler(Id, usize),
    RevealMedia(String),
    OpenUrl(String),
    Copy(String),
    ShowProfile {
        user: Id,
        anchor: egui::Rect,
        side: Side,
    },
    CloseProfile,
    SaveNote {
        user: Id,
        note: String,
    },
    /// Opens the direct message with a user, sending `text` if any.
    MessageUser {
        user: Id,
        text: String,
    },
    /// Puts @name in the composer.
    Mention(Id),
    ToggleDmProfile,
    /// Replaces the word being typed with an autocomplete choice.
    Complete {
        word: String,
        insert: String,
        raw: Option<String>,
    },
}

/// Which side of what was clicked the profile card opens on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Right,
    Left,
    Above,
}

#[derive(Clone, Copy, Debug)]
pub struct Popout {
    pub user: Id,
    pub guild: Option<Id>,
    pub anchor: egui::Rect,
    pub side: Side,
}

pub enum ProfileState {
    Loading,
    Ready(Box<crate::model::Profile>),
    Failed,
}

#[derive(Default)]
pub struct Chat {
    pub popout: Option<Popout>,
    pub profiles: HashMap<Id, (ProfileState, Instant)>,
    /// The message typed in a profile card, to send once the direct
    /// message exists.
    pub card_message: HashMap<Id, String>,
    pending_direct: Option<(Id, String)>,
    pub app_icons: HashMap<Id, Option<String>>,
    /// Where the unread messages began when a channel was opened, for the
    /// "new" line.
    pub unread_from: HashMap<Id, Id>,
    /// What autocomplete put in a draft and what it stands for when sent:
    /// "@name" is "<@id>", "#channel" is "<#id>".
    pub mentioned: HashMap<Id, Vec<(String, String)>>,
    /// The highlighted autocomplete suggestion.
    pub suggestion: usize,
    /// An autocomplete closed with Escape, until the word changes.
    pub dismissed: Option<String>,
    pub reply: HashMap<Id, Id>,
    pub editing: Option<Editing>,
    pub uploads: HashMap<Id, Vec<Pending>>,
    pub picker: Option<Picker>,
    pub gifs: Vec<rest::Gif>,
    pub gif_home: Option<rest::GifHome>,
    pub favorites: Vec<crate::discord::frecency::Favorite>,
    /// Who is typing where, and since when.
    pub typing: HashMap<Id, Vec<(Id, Instant)>>,
    last_typing: HashMap<Id, Instant>,
    pub revealed: HashSet<(Id, usize)>,
    pub revealed_media: HashSet<String>,
    pub dialog: Option<Dialog>,
    pub viewer: Option<String>,
    pub player: Option<media::Player>,
    /// Put the cursor in the composer on the next frame.
    pub focus_composer: bool,
    pub cursor_to_end: bool,
}

impl Chat {
    pub fn new() -> Self {
        Self::default()
    }

    /// Names of who is typing in a channel now, not counting the user.
    pub fn typing_in(&self, channel: Id) -> Vec<Id> {
        self.typing
            .get(&channel)
            .map(|list| {
                list.iter()
                    .filter(|(_, at)| at.elapsed() < TYPING)
                    .map(|(u, _)| *u)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl App {
    pub fn apply_chat(&mut self, action: ChatAction) {
        let channel = self.channel;
        match action {
            ChatAction::Reply { channel, message } => {
                self.chat.reply.insert(channel, message);
                self.chat.focus_composer = true;
            }
            ChatAction::CancelReply => {
                if let Some(channel) = channel {
                    self.chat.reply.remove(&channel);
                }
            }
            ChatAction::StartEdit { channel, message } => self.start_edit(channel, message),
            ChatAction::EditLast => {
                let me = self.model.me;
                let last = channel.and_then(|c| {
                    self.model
                        .channel(c)?
                        .messages
                        .iter()
                        .rev()
                        .find(|m| m.author == me && !m.pending)
                        .map(|m| m.id)
                });
                if let (Some(channel), Some(message)) = (channel, last) {
                    self.start_edit(channel, message);
                }
            }
            ChatAction::SaveEdit => {
                let Some(editing) = self.chat.editing.take() else {
                    return;
                };
                let text = editing.text.trim().to_owned();
                if text.is_empty() {
                    self.chat.dialog = Some(Dialog::Delete {
                        channel: editing.channel,
                        message: editing.message,
                    });
                    return;
                }
                let text = self.expand_emoji(&text);
                if let Some(m) = self.message_mut(editing.channel, editing.message)
                    && m.text != text
                {
                    m.text = text.clone();
                    m.edited = true;
                    self.backend().send(Command::Edit {
                        channel: editing.channel,
                        message: editing.message,
                        content: text,
                    });
                }
                self.chat.focus_composer = true;
            }
            ChatAction::CancelEdit => {
                self.chat.editing = None;
                self.chat.focus_composer = true;
            }
            ChatAction::AskDelete { channel, message } => {
                self.chat.dialog = Some(Dialog::Delete { channel, message })
            }
            ChatAction::Delete { channel, message } => self.delete(channel, message),
            ChatAction::ConfirmDelete => {
                if let Some(Dialog::Delete { channel, message }) = self.chat.dialog.take() {
                    self.delete(channel, message);
                }
            }
            ChatAction::React {
                channel,
                message,
                emoji,
                add,
            } => self.react(channel, message, emoji, add),
            ChatAction::OpenPicker { tab, target } => {
                let same = self
                    .chat
                    .picker
                    .as_ref()
                    .is_some_and(|p| p.tab == tab && p.target == target);
                self.chat.picker = (!same).then(|| Picker {
                    gif_view: GifView::Home,
                    tab,
                    target,
                    query: String::new(),
                    searched: None,
                });
                if !same && tab == PickerTab::Gif {
                    self.gif_home();
                }
            }
            ChatAction::PickerTab(tab) => {
                if let Some(picker) = &mut self.chat.picker {
                    picker.tab = tab;
                    picker.query.clear();
                    picker.gif_view = GifView::Home;
                    if tab == PickerTab::Gif {
                        self.gif_home();
                    }
                }
            }
            ChatAction::ClosePicker => {
                self.chat.picker = None;
                self.chat.focus_composer = true;
            }
            ChatAction::PickEmoji { text, reaction } => {
                match self.chat.picker.as_ref().map(|p| p.target) {
                    Some(PickerTarget::Reaction { channel, message }) => {
                        self.chat.picker = None;
                        self.react(channel, message, reaction, true);
                    }
                    _ => {
                        if let Some(channel) = channel {
                            let draft = self.drafts.entry(channel).or_default();
                            draft.push_str(&text);
                            self.chat.focus_composer = true;
                            self.chat.cursor_to_end = true;
                        }
                    }
                }
            }
            ChatAction::PickGif(url) => {
                self.chat.picker = None;
                if let Some(channel) = channel {
                    self.send_message(channel, url, Vec::new(), None);
                }
            }
            ChatAction::SearchGifs(query) => {
                let Some(picker) = &mut self.chat.picker else {
                    return;
                };
                if query.is_empty() {
                    picker.gif_view = GifView::Home;
                    picker.searched = None;
                    return;
                }
                picker.gif_view = GifView::Search;
                picker.query = query.clone();
                picker.searched = Some(query.clone());
                self.chat.gifs.clear();
                self.backend().send(Command::Gifs(Some(query)));
            }
            ChatAction::GifView(view) => {
                let Some(picker) = &mut self.chat.picker else {
                    return;
                };
                picker.gif_view = view;
                if view != GifView::Search {
                    picker.query.clear();
                    picker.searched = None;
                }
                if view == GifView::Trending {
                    self.chat.gifs.clear();
                    self.backend().send(Command::Gifs(None));
                }
            }
            ChatAction::Favorite { gif, add } => {
                self.chat.favorites.retain(|f| f.url != gif.url);
                if add {
                    let order = self
                        .chat
                        .favorites
                        .iter()
                        .map(|f| f.order)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    self.chat.favorites.insert(
                        0,
                        crate::discord::frecency::Favorite {
                            order,
                            ..gif.clone()
                        },
                    );
                }
                self.backend().send(Command::Favorite { gif, add });
            }
            ChatAction::PickSticker(id) => {
                self.chat.picker = None;
                let sticker = self
                    .model
                    .guilds
                    .iter()
                    .flat_map(|g| g.stickers.iter())
                    .find(|s| s.id == id)
                    .cloned();
                if let (Some(channel), Some(sticker)) = (channel, sticker) {
                    self.send_message(channel, String::new(), Vec::new(), Some(sticker));
                }
            }
            ChatAction::AttachFiles => self.backend().send(Command::PickFiles),
            ChatAction::AddFiles(paths) => self.add_files(paths),
            ChatAction::RemoveUpload(index) => {
                if let Some(list) = channel.and_then(|c| self.chat.uploads.get_mut(&c))
                    && index < list.len()
                {
                    list.remove(index);
                }
            }
            ChatAction::ToggleSpoiler(index) => {
                if let Some(file) = channel
                    .and_then(|c| self.chat.uploads.get_mut(&c))
                    .and_then(|l| l.get_mut(index))
                {
                    file.spoiler = !file.spoiler;
                }
            }
            ChatAction::SendAsFile => {
                let Some(Dialog::TooLong { channel, .. }) = self.chat.dialog.take() else {
                    return;
                };
                let text = self.drafts.remove(&channel).unwrap_or_default();
                let upload = rest::Upload {
                    name: "message.txt".into(),
                    source: rest::UploadSource::Bytes(text.into_bytes()),
                };
                self.send_with(channel, String::new(), vec![upload], None, Vec::new());
            }
            ChatAction::CloseDialog => {
                self.chat.dialog = None;
                self.chat.focus_composer = true;
            }
            ChatAction::LoadOlder(id) => {
                let Some(c) = self.model.channels.get_mut(&id) else {
                    return;
                };
                if c.loading || !c.more {
                    return;
                }
                let Some(before) = c.messages.iter().find(|m| !m.pending).map(|m| m.id) else {
                    return;
                };
                c.loading = true;
                self.backend().send(Command::LoadMessages {
                    channel: id,
                    before: Some(before),
                });
            }
            ChatAction::Typed => {
                let Some(channel) = channel else { return };
                if self.drafts.get(&channel).is_none_or(|d| d.is_empty()) {
                    return;
                }
                let due = self
                    .chat
                    .last_typing
                    .get(&channel)
                    .is_none_or(|at| at.elapsed() > TYPING_EVERY);
                if due {
                    self.chat.last_typing.insert(channel, Instant::now());
                    self.backend().send(Command::Typing(channel));
                }
            }
            ChatAction::Play { key, source } => {
                if !media::available() {
                    let url = source.strip_prefix("ytdl:").unwrap_or(&source).to_owned();
                    let _ = open::that_detached(url);
                    return;
                }
                self.chat.player = Some(media::Player::start(&self.ctx, key, source));
            }
            ChatAction::PlayerToggle => {
                if let Some(player) = &mut self.chat.player {
                    player.toggle();
                }
            }
            ChatAction::PlayerSeek(seconds) => {
                if let Some(player) = &mut self.chat.player {
                    player.seek(seconds);
                }
            }
            ChatAction::PlayerMute => {
                if let Some(player) = &mut self.chat.player {
                    let muted = !player.muted;
                    player.set_muted(muted);
                }
            }
            ChatAction::ViewImage(url) => self.chat.viewer = Some(url),
            ChatAction::CloseViewer => self.chat.viewer = None,
            ChatAction::RevealSpoiler(message, index) => {
                self.chat.revealed.insert((message, index));
            }
            ChatAction::RevealMedia(url) => {
                self.chat.revealed_media.insert(url);
            }
            ChatAction::OpenUrl(url) => {
                let _ = open::that_detached(url);
            }
            ChatAction::Copy(text) => self.ctx.copy_text(text),
            ChatAction::ShowProfile { user, anchor, side } => {
                let same = self
                    .chat
                    .popout
                    .is_some_and(|p| p.user == user && p.anchor == anchor);
                self.chat.popout = (!same).then_some(Popout {
                    user,
                    guild: self.server(),
                    anchor,
                    side,
                });
                if !same {
                    self.load_profile(user);
                }
            }
            ChatAction::CloseProfile => self.chat.popout = None,
            ChatAction::SaveNote { user, note } => {
                let current = match self.chat.profiles.get(&user) {
                    Some((ProfileState::Ready(p), _)) => p.note.clone().unwrap_or_default(),
                    _ => String::new(),
                };
                if note != current {
                    if let Some((ProfileState::Ready(p), _)) = self.chat.profiles.get_mut(&user) {
                        p.note = Some(note.clone());
                    }
                    self.backend().send(Command::SetNote { user, note });
                }
            }
            ChatAction::MessageUser { user, text } => {
                self.chat.popout = None;
                self.chat.card_message.remove(&user);
                let existing = self.model.direct.iter().copied().find(|c| {
                    self.model.channel(*c).is_some_and(|c| {
                        c.kind == crate::model::ChannelKind::Direct && c.recipients == [user]
                    })
                });
                match existing {
                    Some(channel) => self.open_direct(channel, text),
                    None => {
                        self.chat.pending_direct = Some((user, text));
                        self.backend().send(Command::OpenDirect(user));
                    }
                }
            }
            ChatAction::Mention(user) => {
                self.chat.popout = None;
                if let Some(channel) = channel {
                    let name = self.model.name_in(self.server(), user);
                    let draft = self.drafts.entry(channel).or_default();
                    if !draft.is_empty() && !draft.ends_with(' ') {
                        draft.push(' ');
                    }
                    draft.push_str(&format!("@{name} "));
                    self.chat
                        .mentioned
                        .entry(channel)
                        .or_default()
                        .push((format!("@{name}"), format!("<@{user}>")));
                    self.chat.focus_composer = true;
                    self.chat.cursor_to_end = true;
                }
            }
            ChatAction::ToggleDmProfile => {
                self.settings.dm_profile = !self.settings.dm_profile;
                self.save_settings();
            }
            ChatAction::Complete { word, insert, raw } => {
                let Some(channel) = channel else { return };
                let draft = self.drafts.entry(channel).or_default();
                if draft.ends_with(&word) {
                    draft.truncate(draft.len() - word.len());
                }
                draft.push_str(&insert);
                draft.push(' ');
                if let Some(raw) = raw {
                    self.chat
                        .mentioned
                        .entry(channel)
                        .or_default()
                        .push((insert, raw));
                }
                self.chat.suggestion = 0;
                self.chat.focus_composer = true;
                self.chat.cursor_to_end = true;
            }
        }
    }

    /// Asks for a profile unless a recent one is at hand.
    pub fn load_profile(&mut self, user: Id) {
        let fresh = self.chat.profiles.get(&user).is_some_and(|(state, at)| {
            !matches!(state, ProfileState::Failed) && at.elapsed() < Duration::from_secs(300)
        });
        if !fresh {
            self.chat
                .profiles
                .insert(user, (ProfileState::Loading, Instant::now()));
            let guild = self.server();
            self.backend().send(Command::Profile { user, guild });
        }
    }

    /// A game's icon for Rich Presence without its own pictures.
    pub fn app_icon(&self, application: Id) -> Option<String> {
        self.chat.app_icons.get(&application).cloned().flatten()
    }

    pub fn want_app_icon(&mut self, application: Id) {
        if !self.chat.app_icons.contains_key(&application) {
            self.chat.app_icons.insert(application, None);
            self.backend().send(Command::ApplicationIcon(application));
        }
    }

    /// The direct message Discord created or found: open it, and send what
    /// was typed in the profile card.
    pub fn direct_opened(&mut self, value: &Value) {
        for u in value["recipients"].as_array().into_iter().flatten() {
            if let Some(u) = parse::user(u) {
                self.model.users.entry(u.id).or_insert(u);
            }
        }
        let Some(mut channel) = parse::channel(value, None) else {
            return;
        };
        let id = channel.id;
        if !self.model.channels.contains_key(&id) {
            channel.read = channel.last_message;
            self.model.channels.insert(id, channel);
            self.model.direct.push(id);
        }
        let text = self
            .chat
            .pending_direct
            .take()
            .map(|(_, t)| t)
            .unwrap_or_default();
        self.open_direct(id, text);
    }

    fn open_direct(&mut self, channel: Id, text: String) {
        self.go_direct(channel);
        if !text.trim().is_empty() {
            self.send_message(channel, text.trim().to_owned(), Vec::new(), None);
        }
    }

    /// The tiles need the trending searches and the favourites.
    fn gif_home(&mut self) {
        if self.chat.gif_home.is_none() {
            self.backend().send(Command::GifHome);
        }
        self.backend().send(Command::LoadFavorites);
    }

    fn start_edit(&mut self, channel: Id, message: Id) {
        let Some(text) = self
            .model
            .channel(channel)
            .and_then(|c| c.messages.iter().find(|m| m.id == message))
            .map(|m| m.text.clone())
        else {
            return;
        };
        self.chat.editing = Some(Editing {
            channel,
            message,
            text,
            focus: true,
        });
    }

    fn delete(&mut self, channel: Id, message: Id) {
        if let Some(c) = self.model.channels.get_mut(&channel) {
            c.messages.retain(|m| m.id != message);
        }
        self.backend().send(Command::Delete { channel, message });
        self.chat.focus_composer = true;
    }

    fn message_mut(&mut self, channel: Id, message: Id) -> Option<&mut Message> {
        self.model
            .channels
            .get_mut(&channel)?
            .messages
            .iter_mut()
            .find(|m| m.id == message)
    }

    /// Shows the reaction at once; the gateway's echo is then ignored.
    fn react(&mut self, channel: Id, message: Id, emoji: ReactionEmoji, add: bool) {
        let path = emoji.path();
        let Some(m) = self.message_mut(channel, message) else {
            return;
        };
        match m.reactions.iter_mut().position(|r| r.emoji == emoji) {
            Some(index) => {
                let reaction = &mut m.reactions[index];
                if reaction.me == add {
                    return;
                }
                reaction.me = add;
                if add {
                    reaction.count += 1;
                } else {
                    reaction.count = reaction.count.saturating_sub(1);
                    if reaction.count == 0 {
                        m.reactions.remove(index);
                    }
                }
            }
            None if add => m.reactions.push(Reaction {
                emoji,
                count: 1,
                me: true,
            }),
            None => return,
        }
        self.backend().send(Command::React {
            channel,
            message,
            emoji: path,
            add,
        });
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        let Some(channel) = self.channel else { return };
        let list = self.chat.uploads.entry(channel).or_default();
        let mut refused = Vec::new();
        let limit = self.model.max_upload();
        for path in paths {
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "arquivo".into());
            let total: u64 = list.iter().map(|f| f.size).sum::<u64>() + meta.len();
            if total > limit || list.len() >= 10 {
                refused.push(name);
                continue;
            }
            list.push(Pending {
                path,
                name,
                size: meta.len(),
                spoiler: false,
            });
        }
        if !refused.is_empty() {
            self.chat.dialog = Some(Dialog::TooLarge { names: refused });
        }
        self.chat.focus_composer = true;
    }

    /// The composer's Enter: checks the limits, then sends.
    pub fn send_draft(&mut self) {
        let Some(channel) = self.channel else { return };
        let text = self
            .drafts
            .get(&channel)
            .map(|d| d.trim().to_owned())
            .unwrap_or_default();
        let files = self.chat.uploads.get(&channel).cloned().unwrap_or_default();
        if text.is_empty() && files.is_empty() {
            return;
        }
        let mut text = self.expand_emoji(&text);
        for (shown, raw) in self.chat.mentioned.remove(&channel).unwrap_or_default() {
            text = text.replace(&shown, &raw);
        }
        let length = text.chars().count();
        if length > self.model.max_message() {
            self.chat.dialog = Some(Dialog::TooLong { channel, length });
            return;
        }
        self.drafts.remove(&channel);
        self.chat.uploads.remove(&channel);
        let uploads = files
            .iter()
            .map(|f| rest::Upload {
                name: if f.spoiler {
                    format!("SPOILER_{}", f.name)
                } else {
                    f.name.clone()
                },
                source: rest::UploadSource::Path(f.path.clone()),
            })
            .collect();
        let shown = files
            .iter()
            .map(|f| Attachment {
                url: String::new(),
                proxy: String::new(),
                name: f.name.clone(),
                size: f.size,
                content_type: None,
                width: None,
                height: None,
                spoiler: f.spoiler,
                duration: None,
                waveform: Vec::new(),
            })
            .collect();
        self.send_with(channel, text, uploads, None, shown);
    }

    fn send_message(
        &mut self,
        channel: Id,
        text: String,
        files: Vec<rest::Upload>,
        sticker: Option<Sticker>,
    ) {
        self.send_with(channel, text, files, sticker, Vec::new());
    }

    /// The message shows at once, greyed until the gateway echoes it.
    fn send_with(
        &mut self,
        channel: Id,
        text: String,
        files: Vec<rest::Upload>,
        sticker: Option<Sticker>,
        shown: Vec<Attachment>,
    ) {
        let now = jiff::Timestamp::now();
        let nonce = crate::app::snowflake(now).to_string();
        let reply = self.chat.reply.remove(&channel);
        let me = self.model.me;
        let mut message = Message::plain(me, now, text.clone());
        message.id = u64::MAX;
        message.nonce = Some(nonce.clone());
        message.pending = true;
        message.attachments = shown;
        message.stickers = sticker.iter().cloned().collect();
        if let Some(reply) = reply {
            let quoted = self
                .model
                .channel(channel)
                .and_then(|c| c.messages.iter().find(|m| m.id == reply));
            message.reply = Some(crate::model::Reply {
                author: quoted.map(|m| m.author),
                text: quoted.map(|m| m.text.clone()),
                has_media: quoted.is_some_and(|m| !m.attachments.is_empty()),
            });
            message.kind = 19;
        }
        if let Some(c) = self.model.channels.get_mut(&channel) {
            c.messages.push(message);
        }
        self.chat.last_typing.remove(&channel);
        self.chat.unread_from.remove(&channel);
        self.backend().send(Command::Send {
            channel,
            message: rest::Outgoing {
                content: text,
                nonce,
                reply,
                sticker: sticker.map(|s| s.id),
                files,
            },
        });
        self.chat.focus_composer = true;
    }

    /// `:smile:` becomes 😄 and `:name:` a server emoji, as Discord does
    /// when sending.
    fn expand_emoji(&self, text: &str) -> String {
        let guild = self.server().and_then(|g| self.model.guild(g));
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find(':') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let end = after
                .find(':')
                .filter(|&e| e > 0 && !after[..e].contains(char::is_whitespace));
            let Some(end) = end else {
                out.push(':');
                rest = after;
                continue;
            };
            let name = &after[..end];
            if let Some(emoji) = emojis::get_by_shortcode(name) {
                out.push_str(emoji.as_str());
            } else if let Some(custom) =
                guild.and_then(|g| g.emojis.iter().find(|e| e.name == name))
            {
                let prefix = if custom.animated { "a" } else { "" };
                out.push_str(&format!("<{prefix}:{}:{}>", custom.name, custom.id));
            } else {
                out.push(':');
                out.push_str(name);
                rest = &after[end..];
                continue;
            }
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out
    }

    /// Older history arrived: put it in front.
    pub fn older(&mut self, channel: Id, list: &[Value]) {
        let me = self.model.me;
        let guild = self.model.channel(channel).and_then(|c| c.guild);
        let mut older = Vec::new();
        for value in list.iter().rev() {
            self.learn(guild, value);
            if let Some(m) = parse::message(value, me) {
                older.push(m);
            }
        }
        if let Some(c) = self.model.channels.get_mut(&channel) {
            c.loading = false;
            c.more = list.len() >= 50;
            let first = c.messages.first().map_or(u64::MAX, |m| m.id);
            older.retain(|m| m.id < first);
            older.append(&mut c.messages);
            c.messages = older;
        }
    }

    /// Gateway events about messages beyond create, update and delete.
    pub fn chat_dispatch(&mut self, kind: &str, data: &Value) {
        let me = self.model.me;
        let channel = parse::id(&data["channel_id"]);
        let message = parse::id(&data["message_id"]);
        match kind {
            "MESSAGE_REACTION_ADD" | "MESSAGE_REACTION_REMOVE" => {
                let (Some(channel), Some(message)) = (channel, message) else {
                    return;
                };
                let Some(emoji) = parse::reaction_emoji(&data["emoji"]) else {
                    return;
                };
                let mine = parse::id(&data["user_id"]) == Some(me);
                let add = kind == "MESSAGE_REACTION_ADD";
                let Some(m) = self.message_mut(channel, message) else {
                    return;
                };
                match m.reactions.iter_mut().position(|r| r.emoji == emoji) {
                    Some(index) => {
                        let reaction = &mut m.reactions[index];
                        // Our own, already shown when we reacted.
                        if mine && reaction.me == add {
                            return;
                        }
                        if mine {
                            reaction.me = add;
                        }
                        if add {
                            reaction.count += 1;
                        } else {
                            reaction.count = reaction.count.saturating_sub(1);
                            if reaction.count == 0 {
                                m.reactions.remove(index);
                            }
                        }
                    }
                    None if add => m.reactions.push(Reaction {
                        emoji,
                        count: 1,
                        me: mine,
                    }),
                    None => {}
                }
            }
            "MESSAGE_REACTION_REMOVE_ALL" => {
                if let (Some(channel), Some(message)) = (channel, message)
                    && let Some(m) = self.message_mut(channel, message)
                {
                    m.reactions.clear();
                }
            }
            "TYPING_START" => {
                let (Some(channel), Some(user)) = (channel, parse::id(&data["user_id"])) else {
                    return;
                };
                if user == me {
                    return;
                }
                if let Some(member) = data.get("member")
                    && let Some(u) = parse::user(&member["user"])
                {
                    self.model.users.entry(u.id).or_insert(u);
                }
                let list = self.chat.typing.entry(channel).or_default();
                list.retain(|(u, at)| *u != user && at.elapsed() < TYPING);
                list.push((user, Instant::now()));
                self.ctx.request_repaint_after(TYPING);
            }
            _ => {}
        }
    }

    /// A message from someone ends their "is typing".
    pub fn stopped_typing(&mut self, channel: Id, user: Id) {
        if let Some(list) = self.chat.typing.get_mut(&channel) {
            list.retain(|(u, _)| *u != user);
        }
    }

    /// Learns the author, their roles and whoever the message mentions.
    pub fn learn(&mut self, guild: Option<Id>, value: &Value) {
        for user in parse::mentioned_users(value) {
            self.model.users.entry(user.id).or_insert(user);
        }
        let Some(user) = parse::user(&value["author"]) else {
            return;
        };
        let id = user.id;
        self.model.users.entry(id).or_insert(user);
        if let Some(guild) = guild
            && value["member"].is_object()
            && let Some(g) = self.model.guild_mut(guild)
        {
            g.members.insert(id, parse::member(&value["member"]));
        }
    }
}
