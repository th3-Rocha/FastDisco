//! Threads and forums: each channel's threads (active and archived),
//! starting one from a message or from nothing, forum posts with their
//! tags, and following or leaving a thread. A thread is a channel of its
//! own, so reading and replying go through the usual chat.

use std::collections::HashMap;

use reqwest::Method;
use serde_json::{Value, json};

use crate::app::{Action, App};
use crate::backend::Command;
use crate::discord::parse;
use crate::model::{ChannelKind, Id, Message};

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    /// A channel's active threads or forum posts.
    Active { channel: Id },
    Archived { channel: Id },
    /// A new thread or post; `first` is the message to send into it.
    Created { first: Option<String> },
    /// A thread asked for to be opened.
    Fetched,
    /// Followed or left; the gateway confirms it.
    Membership,
    /// Archived, reopened, locked or renamed.
    Edited,
}

/// The threads of one channel, or the posts of one forum, newest activity
/// first.
#[derive(Default)]
pub struct List {
    pub active: Vec<Id>,
    pub archived: Vec<Id>,
    pub loading: bool,
    pub archived_loaded: bool,
    /// A forum post's first message, for its card.
    pub first: HashMap<Id, Message>,
}

/// The form for a new thread or forum post.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub parent: Id,
    /// The message the thread starts from.
    pub message: Option<Id>,
    pub name: String,
    pub text: String,
    pub tags: Vec<Id>,
    pub private: bool,
    pub busy: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Threads {
    pub lists: HashMap<Id, List>,
    /// The thread popout's tab: active or archived.
    pub show_archived: bool,
    pub draft: Option<Draft>,
    /// The forum's tag filter.
    pub filter: Vec<Id>,
    /// A thread to open once fetched.
    pending_open: Option<Id>,
}

#[derive(Clone, Debug)]
pub enum ThreadAction {
    /// Loads a channel's threads or a forum's posts.
    Load(Id),
    LoadArchived(Id),
    ShowArchived(bool),
    /// Opens a thread, fetching it first when unknown.
    Open(Id),
    /// Starts the new thread form, from a message or not.
    Start {
        parent: Id,
        message: Option<Id>,
    },
    CancelDraft,
    SubmitDraft,
    ToggleDraftTag(Id),
    ToggleFilter(Id),
    /// Follows or leaves a thread.
    Join(Id, bool),
    /// Closes or reopens a thread.
    Archive(Id, bool),
    Lock(Id, bool),
}

impl App {
    fn thread_api(&self, tag: Tag, method: Method, path: String, body: Option<Value>) {
        self.backend().send(Command::Api {
            tag: crate::backend::Tag::Threads(tag),
            method,
            path,
            body,
        });
    }

    pub fn apply_threads(&mut self, action: ThreadAction) {
        match action {
            ThreadAction::Load(channel) => {
                let list = self.threads.lists.entry(channel).or_default();
                if list.loading {
                    return;
                }
                list.loading = true;
                self.thread_api(
                    Tag::Active { channel },
                    Method::GET,
                    format!(
                        "/channels/{channel}/threads/search?archived=false&sort_by=last_message_time&sort_order=desc&limit=25&offset=0"
                    ),
                    None,
                );
            }
            ThreadAction::LoadArchived(channel) => {
                let list = self.threads.lists.entry(channel).or_default();
                if list.archived_loaded {
                    return;
                }
                list.archived_loaded = true;
                self.thread_api(
                    Tag::Archived { channel },
                    Method::GET,
                    format!(
                        "/channels/{channel}/threads/search?archived=true&sort_by=last_message_time&sort_order=desc&limit=25&offset=0"
                    ),
                    None,
                );
            }
            ThreadAction::ShowArchived(on) => {
                self.threads.show_archived = on;
                if on && let Some(channel) = self.channel {
                    let parent = self.thread_parent(channel);
                    self.apply_threads(ThreadAction::LoadArchived(parent));
                }
            }
            ThreadAction::Open(thread) => {
                self.nav.popout = None;
                if self.model.channels.contains_key(&thread) {
                    self.actions.push(Action::OpenChannel(thread));
                } else {
                    self.threads.pending_open = Some(thread);
                    self.thread_api(Tag::Fetched, Method::GET, format!("/channels/{thread}"), None);
                }
            }
            ThreadAction::Start { parent, message } => {
                self.nav.popout = None;
                let name = message
                    .and_then(|m| {
                        self.model
                            .channel(parent)?
                            .messages
                            .iter()
                            .find(|x| x.id == m)
                            .map(|x| x.text.chars().take(40).collect::<String>())
                    })
                    .unwrap_or_default();
                self.threads.draft = Some(Draft {
                    parent,
                    message,
                    name,
                    ..Draft::default()
                });
            }
            ThreadAction::CancelDraft => self.threads.draft = None,
            ThreadAction::ToggleDraftTag(tag) => {
                if let Some(draft) = &mut self.threads.draft {
                    if let Some(at) = draft.tags.iter().position(|t| *t == tag) {
                        draft.tags.remove(at);
                    } else if draft.tags.len() < 5 {
                        draft.tags.push(tag);
                    }
                }
            }
            ThreadAction::ToggleFilter(tag) => {
                if let Some(at) = self.threads.filter.iter().position(|t| *t == tag) {
                    self.threads.filter.remove(at);
                } else {
                    self.threads.filter.push(tag);
                }
            }
            ThreadAction::SubmitDraft => self.submit_thread(),
            ThreadAction::Join(thread, join) => {
                let method = if join { Method::PUT } else { Method::DELETE };
                if let Some(meta) = self
                    .model
                    .channels
                    .get_mut(&thread)
                    .and_then(|c| c.thread.as_mut())
                {
                    meta.joined = join;
                }
                self.thread_api(
                    Tag::Membership,
                    method,
                    format!("/channels/{thread}/thread-members/@me"),
                    None,
                );
            }
            ThreadAction::Archive(thread, archived) => self.thread_api(
                Tag::Edited,
                Method::PATCH,
                format!("/channels/{thread}"),
                Some(json!({ "archived": archived })),
            ),
            ThreadAction::Lock(thread, locked) => self.thread_api(
                Tag::Edited,
                Method::PATCH,
                format!("/channels/{thread}"),
                Some(json!({ "locked": locked })),
            ),
        }
    }

    /// The channel whose threads a view lists: a thread's parent, or the
    /// channel itself.
    pub fn thread_parent(&self, channel: Id) -> Id {
        self.model
            .channel(channel)
            .filter(|c| c.kind == ChannelKind::Thread)
            .and_then(|c| c.parent)
            .unwrap_or(channel)
    }

    fn submit_thread(&mut self) {
        let Some(draft) = &mut self.threads.draft else {
            return;
        };
        let name = draft.name.trim().to_owned();
        let text = draft.text.trim().to_owned();
        let parent = draft.parent;
        let forum = self
            .model
            .channel(parent)
            .is_some_and(|c| c.kind.is_forum());
        if name.is_empty() {
            draft.error = Some("Dê um nome ao tópico.".into());
            return;
        }
        if forum && text.is_empty() {
            draft.error = Some("Escreva a primeira mensagem da postagem.".into());
            return;
        }
        draft.busy = true;
        draft.error = None;
        let (tag, path, body) = if forum {
            (
                Tag::Created { first: None },
                format!("/channels/{parent}/threads"),
                json!({
                    "name": name,
                    "auto_archive_duration": 4320,
                    "applied_tags": draft.tags.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                    "message": { "content": text },
                }),
            )
        } else if let Some(message) = draft.message {
            (
                Tag::Created {
                    first: (!text.is_empty()).then_some(text),
                },
                format!("/channels/{parent}/messages/{message}/threads"),
                json!({ "name": name, "auto_archive_duration": 4320 }),
            )
        } else {
            (
                Tag::Created {
                    first: (!text.is_empty()).then_some(text),
                },
                format!("/channels/{parent}/threads"),
                json!({
                    "name": name,
                    "type": if draft.private { 12 } else { 11 },
                    "auto_archive_duration": 4320,
                }),
            )
        };
        self.thread_api(tag, Method::POST, path, Some(body));
    }

    /// Puts a thread from the API or the gateway into the model, under its
    /// server, keeping what was already loaded of it.
    pub fn insert_thread(&mut self, value: &Value) -> Option<Id> {
        let mut channel = parse::channel(value, None)?;
        let id = channel.id;
        match self.model.channels.remove(&id) {
            Some(old) => {
                channel.messages = old.messages;
                channel.requested = old.requested;
                channel.read = old.read;
                channel.mentions = old.mentions;
                if let (Some(new), Some(old)) = (&mut channel.thread, &old.thread)
                    && !value["member"].is_object()
                {
                    new.joined = old.joined;
                }
            }
            None => {
                channel.read = channel.last_message;
                if let Some(g) = channel.guild.and_then(|g| self.model.guild_mut(g))
                    && !g.channels.contains(&id)
                {
                    g.channels.push(id);
                }
            }
        }
        self.model.channels.insert(id, channel);
        Some(id)
    }

    /// The thread events: new, changed and gone threads, the threads of a
    /// channel on sync, and the user following or leaving one.
    pub fn thread_dispatch(&mut self, kind: &str, data: &Value) {
        match kind {
            "THREAD_CREATE" | "THREAD_UPDATE" => {
                let Some(id) = self.insert_thread(data) else { return };
                if data["newly_created"].as_bool() == Some(true)
                    && parse::id(&data["owner_id"]) == Some(self.model.me)
                    && let Some(meta) = self.model.channels.get_mut(&id).and_then(|c| c.thread.as_mut())
                {
                    meta.joined = true;
                }
                if let Some(parent) = self.model.channel(id).and_then(|c| c.parent)
                    && let Some(list) = self.threads.lists.get_mut(&parent)
                    && !list.active.contains(&id)
                    && !list.archived.contains(&id)
                {
                    list.active.insert(0, id);
                }
            }
            "THREAD_DELETE" => {
                let Some(id) = parse::id(&data["id"]) else { return };
                self.model.channels.remove(&id);
                for g in &mut self.model.guilds {
                    g.channels.retain(|c| *c != id);
                }
                for list in self.threads.lists.values_mut() {
                    list.active.retain(|c| *c != id);
                    list.archived.retain(|c| *c != id);
                }
                if self.channel == Some(id) {
                    self.channel = parse::id(&data["parent_id"]);
                }
            }
            "THREAD_LIST_SYNC" => {
                for thread in data["threads"].as_array().into_iter().flatten() {
                    self.insert_thread(thread);
                }
                for member in data["members"].as_array().into_iter().flatten() {
                    if parse::id(&member["user_id"]) == Some(self.model.me)
                        && let Some(meta) = parse::id(&member["id"])
                            .and_then(|t| self.model.channels.get_mut(&t))
                            .and_then(|c| c.thread.as_mut())
                    {
                        meta.joined = true;
                    }
                }
            }
            "THREAD_MEMBER_UPDATE" => {
                if let Some(meta) = parse::id(&data["id"])
                    .and_then(|t| self.model.channels.get_mut(&t))
                    .and_then(|c| c.thread.as_mut())
                {
                    meta.joined = true;
                }
            }
            "THREAD_MEMBERS_UPDATE" => {
                let me = self.model.me;
                let Some(meta) = parse::id(&data["id"])
                    .and_then(|t| self.model.channels.get_mut(&t))
                    .and_then(|c| c.thread.as_mut())
                else {
                    return;
                };
                if let Some(count) = data["member_count"].as_u64() {
                    meta.members = count as u32;
                }
                let added = data["added_members"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|m| parse::id(&m["user_id"]) == Some(me));
                let removed = data["removed_member_ids"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|m| parse::id(m) == Some(me));
                if added {
                    meta.joined = true;
                } else if removed {
                    meta.joined = false;
                }
            }
            _ => {}
        }
    }

    pub fn threads_result(&mut self, tag: Tag, result: Result<Value, String>) {
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                match tag {
                    Tag::Created { .. } => {
                        if let Some(draft) = &mut self.threads.draft {
                            draft.busy = false;
                            draft.error = Some(error);
                        }
                    }
                    Tag::Active { channel } => {
                        let list = self.threads.lists.entry(channel).or_default();
                        list.loading = false;
                        log::warn!("threads of {channel}: {error}");
                    }
                    _ => self.chat.dialog = Some(crate::chat::Dialog::Failed(error)),
                }
                return;
            }
        };
        match tag {
            Tag::Active { channel } | Tag::Archived { channel } => {
                let archived = matches!(tag, Tag::Archived { .. });
                let mut ids = Vec::new();
                for thread in value["threads"].as_array().into_iter().flatten() {
                    if let Some(id) = self.insert_thread(thread) {
                        ids.push(id);
                    }
                }
                for member in value["members"].as_array().into_iter().flatten() {
                    if parse::id(&member["user_id"]) == Some(self.model.me)
                        && let Some(meta) = parse::id(&member["id"])
                            .and_then(|t| self.model.channels.get_mut(&t))
                            .and_then(|c| c.thread.as_mut())
                    {
                        meta.joined = true;
                    }
                }
                let me = self.model.me;
                let mut first = HashMap::new();
                for m in value["first_messages"].as_array().into_iter().flatten() {
                    if let Some(user) = parse::user(&m["author"]) {
                        self.model.users.entry(user.id).or_insert(user);
                    }
                    if let (Some(thread), Some(message)) =
                        (parse::id(&m["channel_id"]), parse::message(m, me))
                    {
                        first.insert(thread, message);
                    }
                }
                let list = self.threads.lists.entry(channel).or_default();
                list.first.extend(first);
                if archived {
                    list.archived = ids;
                } else {
                    list.loading = false;
                    list.active = ids;
                }
            }
            Tag::Created { first } => {
                self.threads.draft = None;
                let Some(id) = self.insert_thread(&value) else {
                    return;
                };
                if let Some(meta) = self.model.channels.get_mut(&id).and_then(|c| c.thread.as_mut()) {
                    meta.joined = true;
                }
                if let Some(parent) = parse::id(&value["parent_id"])
                    && let Some(list) = self.threads.lists.get_mut(&parent)
                    && !list.active.contains(&id)
                {
                    list.active.insert(0, id);
                }
                self.actions.push(Action::OpenChannel(id));
                if let Some(text) = first {
                    self.send_text(id, text);
                }
            }
            Tag::Fetched => {
                let Some(id) = self.insert_thread(&value) else {
                    return;
                };
                if self.threads.pending_open.take() == Some(id) {
                    self.actions.push(Action::OpenChannel(id));
                }
            }
            Tag::Membership => {}
            Tag::Edited => {
                self.insert_thread(&value);
            }
        }
    }
}
