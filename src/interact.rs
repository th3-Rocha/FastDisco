//! Doing things with messages and bots: forwarding, pinning, polls (voting
//! and making one), bots' buttons and menus, the forms they open, and
//! slash commands and message apps. Bot interactions go to
//! `POST /interactions` with the gateway session, and the gateway says
//! whether they worked.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use reqwest::Method;
use serde_json::{Value, json};

use crate::app::{Action, App};
use crate::backend::Command;
use crate::discord::parse;
use crate::model::{Id, Poll};

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    /// The commands usable in a server, or in direct messages.
    Commands { scope: Id },
    /// A request whose failure is the only news.
    Done,
    /// An interaction, by its nonce: a refusal shows under its message.
    Interaction { nonce: String },
}

/// Where commands are listed: a server, or a direct message channel.
pub type Scope = Id;

#[derive(Clone, Debug)]
pub struct ForwardDialog {
    pub channel: Id,
    pub message: Id,
    pub query: String,
    pub chosen: Vec<Id>,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct PollDraft {
    pub channel: Id,
    pub question: String,
    pub answers: Vec<String>,
    pub hours: u32,
    pub multiple: bool,
    pub error: Option<String>,
}

/// A form a bot opened.
#[derive(Clone, Debug)]
pub struct BotModal {
    pub id: String,
    pub application: Id,
    pub channel: Id,
    pub title: String,
    pub custom_id: String,
    pub fields: Vec<ModalField>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ModalField {
    pub custom_id: String,
    pub label: String,
    pub description: Option<String>,
    /// 1 one line, 2 paragraph.
    pub style: u8,
    pub placeholder: Option<String>,
    pub required: bool,
    pub value: String,
    pub min: usize,
    pub max: usize,
    /// Sent back inside a label (Discord's newer forms) or a row.
    pub labelled: bool,
}

/// One application command as Discord lists it.
#[derive(Clone, Debug)]
pub struct AppCommand {
    pub id: Id,
    pub application: Id,
    pub version: String,
    pub name: String,
    pub description: String,
    /// 1 slash, 2 user, 3 message.
    pub kind: u8,
    pub raw: Value,
}

/// A slash command, or one of its subcommands, ready to fill in.
#[derive(Clone, Debug)]
pub struct Entry {
    pub command: usize,
    /// Group and subcommand names under the command.
    pub path: Vec<String>,
    pub description: String,
    pub options: Vec<CommandOption>,
}

#[derive(Clone, Debug)]
pub struct CommandOption {
    pub name: String,
    pub description: String,
    /// 3 text, 4 integer, 5 yes/no, 6 user, 7 channel, 8 role,
    /// 9 mentionable, 10 number, 11 attachment.
    pub kind: u8,
    pub required: bool,
    pub choices: Vec<(String, Value)>,
}

#[derive(Default, Clone, Debug)]
pub struct CommandIndex {
    pub commands: Vec<AppCommand>,
    pub entries: Vec<Entry>,
    pub apps: HashMap<Id, String>,
    /// Each app's picture, as the command list shows it.
    pub icons: HashMap<Id, String>,
}

impl CommandIndex {
    /// The app's icon, or the bot's avatar, else Discord's default one.
    pub fn icon(&self, application: Id) -> String {
        self.icons.get(&application).cloned().unwrap_or_else(|| {
            format!(
                "https://cdn.discordapp.com/embed/avatars/{}.png",
                (application >> 22) % 6
            )
        })
    }
}

/// The slash command being filled in.
#[derive(Clone, Debug)]
pub struct Active {
    pub channel: Id,
    pub scope: Scope,
    pub entry: usize,
    pub values: Vec<String>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Interact {
    pub forward: Option<ForwardDialog>,
    pub poll: Option<PollDraft>,
    /// Answers picked in a poll before pressing "Votar".
    pub poll_choice: HashMap<Id, Vec<u32>>,
    /// Polls showing their results without a vote.
    pub poll_results: HashSet<Id>,
    /// Interactions on their way: nonce to (message, the component).
    pub pending: HashMap<String, (Id, String)>,
    /// Messages whose last interaction failed, and when.
    pub failed: HashMap<Id, Instant>,
    pub modal: Option<BotModal>,
    pub commands: HashMap<Scope, CommandIndex>,
    requested: HashSet<Scope>,
    pub active: Option<Active>,
}

#[derive(Clone, Debug)]
pub enum InteractAction {
    OpenForward {
        channel: Id,
        message: Id,
    },
    ToggleForwardTarget(Id),
    SendForward,
    CloseForward,
    Pin {
        channel: Id,
        message: Id,
        pin: bool,
    },
    Report {
        channel: Id,
        message: Id,
    },
    CopyLink {
        channel: Id,
        message: Id,
    },
    PickAnswer {
        message: Id,
        answer: u32,
        multiple: bool,
    },
    Vote {
        channel: Id,
        message: Id,
    },
    RemoveVote {
        channel: Id,
        message: Id,
    },
    ShowResults(Id),
    EndPoll {
        channel: Id,
        message: Id,
    },
    OpenPollDraft,
    SendPoll,
    ClosePollDraft,
    Button {
        channel: Id,
        message: Id,
        custom_id: String,
    },
    Select {
        channel: Id,
        message: Id,
        custom_id: String,
        kind: u8,
        values: Vec<String>,
    },
    SubmitModal,
    CloseModal,
    StartCommand(usize),
    RunCommand,
    CancelCommand,
    MessageApp {
        channel: Id,
        message: Id,
        command: usize,
    },
}

/// Discord's permission bits used here.
pub const MANAGE_MESSAGES: u64 = 1 << 13;

impl App {
    fn interact_api(&self, tag: Tag, method: Method, path: String, body: Option<Value>) {
        self.backend().send(Command::Api {
            tag: crate::backend::Tag::Interact(tag),
            method,
            path,
            body,
        });
    }

    fn nonce(&self) -> String {
        crate::app::snowflake(jiff::Timestamp::now()).to_string()
    }

    /// Where commands for a channel are listed.
    pub fn command_scope(&self, channel: Id) -> Scope {
        self.model
            .channel(channel)
            .and_then(|c| c.guild)
            .unwrap_or(channel)
    }

    /// Asks once for the commands usable in this channel's server, or in
    /// this conversation, with the user's own apps.
    pub fn want_commands(&mut self, channel: Id) {
        let scope = self.command_scope(channel);
        if !self.interact.requested.insert(scope) {
            return;
        }
        let path = match self.model.channel(channel).and_then(|c| c.guild) {
            Some(guild) => format!("/guilds/{guild}/application-command-index"),
            None => format!("/channels/{channel}/application-command-index"),
        };
        self.interact_api(Tag::Commands { scope }, Method::GET, path, None);
        self.interact_api(
            Tag::Commands { scope },
            Method::GET,
            "/users/@me/application-command-index".into(),
            None,
        );
    }

    pub fn apply_interact(&mut self, action: InteractAction) {
        match action {
            InteractAction::OpenForward { channel, message } => {
                self.interact.forward = Some(ForwardDialog {
                    channel,
                    message,
                    query: String::new(),
                    chosen: Vec::new(),
                    note: String::new(),
                });
            }
            InteractAction::ToggleForwardTarget(target) => {
                if let Some(f) = &mut self.interact.forward {
                    if let Some(at) = f.chosen.iter().position(|c| *c == target) {
                        f.chosen.remove(at);
                    } else if f.chosen.len() < 5 {
                        f.chosen.push(target);
                    }
                }
            }
            InteractAction::SendForward => {
                let Some(f) = self.interact.forward.take() else {
                    return;
                };
                let guild = self.model.channel(f.channel).and_then(|c| c.guild);
                for target in &f.chosen {
                    let body = json!({
                        "nonce": self.nonce(),
                        "message_reference": {
                            "type": 1,
                            "message_id": f.message.to_string(),
                            "channel_id": f.channel.to_string(),
                            "guild_id": guild.map(|g| g.to_string()),
                        },
                    });
                    self.interact_api(
                        Tag::Done,
                        Method::POST,
                        format!("/channels/{target}/messages"),
                        Some(body),
                    );
                    // The note goes as its own message after the forward.
                    if !f.note.trim().is_empty() {
                        self.interact_api(
                            Tag::Done,
                            Method::POST,
                            format!("/channels/{target}/messages"),
                            Some(json!({ "content": f.note.trim(), "nonce": self.nonce() })),
                        );
                    }
                }
            }
            InteractAction::CloseForward => self.interact.forward = None,
            InteractAction::Pin {
                channel,
                message,
                pin,
            } => {
                let method = if pin { Method::PUT } else { Method::DELETE };
                self.interact_api(
                    Tag::Done,
                    method,
                    format!("/channels/{channel}/pins/{message}"),
                    None,
                );
                if let Some(m) = self.find_message(channel, message) {
                    m.pinned = pin;
                }
                self.nav.pins.remove(&channel);
            }
            InteractAction::Report { channel, message } => {
                // Discord's in-app report walks a menu of its own; the
                // web form takes the same report with the message's link.
                let link = self.message_link(channel, message);
                self.ctx.copy_text(link);
                let _ = open::that_detached("https://dis.gd/report");
            }
            InteractAction::CopyLink { channel, message } => {
                let link = self.message_link(channel, message);
                self.ctx.copy_text(link);
            }
            InteractAction::PickAnswer {
                message,
                answer,
                multiple,
            } => {
                let picked = self.interact.poll_choice.entry(message).or_default();
                if multiple {
                    if let Some(at) = picked.iter().position(|a| *a == answer) {
                        picked.remove(at);
                    } else {
                        picked.push(answer);
                    }
                } else {
                    *picked = vec![answer];
                }
            }
            InteractAction::Vote { channel, message } => {
                let answers = self
                    .interact
                    .poll_choice
                    .remove(&message)
                    .unwrap_or_default();
                if answers.is_empty() {
                    return;
                }
                self.vote_locally(channel, message, &answers);
                let ids: Vec<String> = answers.iter().map(u32::to_string).collect();
                self.interact_api(
                    Tag::Done,
                    Method::PUT,
                    format!("/channels/{channel}/polls/{message}/answers/@me"),
                    Some(json!({ "answer_ids": ids })),
                );
            }
            InteractAction::RemoveVote { channel, message } => {
                self.vote_locally(channel, message, &[]);
                self.interact_api(
                    Tag::Done,
                    Method::PUT,
                    format!("/channels/{channel}/polls/{message}/answers/@me"),
                    Some(json!({ "answer_ids": [] })),
                );
            }
            InteractAction::ShowResults(message) => {
                if !self.interact.poll_results.remove(&message) {
                    self.interact.poll_results.insert(message);
                }
            }
            InteractAction::EndPoll { channel, message } => self.interact_api(
                Tag::Done,
                Method::POST,
                format!("/channels/{channel}/polls/{message}/expire"),
                None,
            ),
            InteractAction::OpenPollDraft => {
                let Some(channel) = self.channel else { return };
                self.interact.poll = Some(PollDraft {
                    channel,
                    question: String::new(),
                    answers: vec![String::new(), String::new()],
                    hours: 24,
                    multiple: false,
                    error: None,
                });
            }
            InteractAction::SendPoll => self.send_poll(),
            InteractAction::ClosePollDraft => self.interact.poll = None,
            InteractAction::Button {
                channel,
                message,
                custom_id,
            } => self.component(channel, message, custom_id, 2, None),
            InteractAction::Select {
                channel,
                message,
                custom_id,
                kind,
                values,
            } => self.component(channel, message, custom_id, kind, Some(values)),
            InteractAction::SubmitModal => self.submit_modal_form(),
            InteractAction::CloseModal => self.interact.modal = None,
            InteractAction::StartCommand(entry) => {
                let Some(channel) = self.channel else { return };
                let scope = self.command_scope(channel);
                let count = self
                    .interact
                    .commands
                    .get(&scope)
                    .and_then(|i| i.entries.get(entry))
                    .map(|e| e.options.len());
                let Some(count) = count else { return };
                self.drafts.remove(&channel);
                self.interact.active = Some(Active {
                    channel,
                    scope,
                    entry,
                    values: vec![String::new(); count],
                    error: None,
                });
            }
            InteractAction::RunCommand => self.run_command(),
            InteractAction::CancelCommand => {
                self.interact.active = None;
                self.chat.focus_composer = true;
            }
            InteractAction::MessageApp {
                channel,
                message,
                command,
            } => {
                let scope = self.command_scope(channel);
                let Some(cmd) = self
                    .interact
                    .commands
                    .get(&scope)
                    .and_then(|i| i.commands.get(command))
                    .cloned()
                else {
                    return;
                };
                let data = json!({
                    "version": cmd.version,
                    "id": cmd.id.to_string(),
                    "name": cmd.name,
                    "type": 3,
                    "options": [],
                    "application_command": cmd.raw,
                    "target_id": message.to_string(),
                    "attachments": [],
                });
                self.interaction(
                    2,
                    cmd.application,
                    channel,
                    None,
                    data,
                    message,
                    String::new(),
                );
            }
        }
    }

    pub fn find_message(&mut self, channel: Id, message: Id) -> Option<&mut crate::model::Message> {
        self.model
            .channels
            .get_mut(&channel)?
            .messages
            .iter_mut()
            .find(|m| m.id == message)
    }

    fn message_link(&self, channel: Id, message: Id) -> String {
        let place = self
            .model
            .channel(channel)
            .and_then(|c| c.guild)
            .map_or_else(|| "@me".to_owned(), |g| g.to_string());
        format!("https://discord.com/channels/{place}/{channel}/{message}")
    }

    /// The user's vote shows at once; the gateway's own events follow.
    fn vote_locally(&mut self, channel: Id, message: Id, answers: &[u32]) {
        let Some(poll) = self
            .find_message(channel, message)
            .and_then(|m| m.poll.as_mut())
        else {
            return;
        };
        for answer in &mut poll.answers {
            let now = answers.contains(&answer.id);
            if answer.me && !now {
                answer.count = answer.count.saturating_sub(1);
            } else if !answer.me && now {
                answer.count += 1;
            }
            answer.me = now;
        }
    }

    /// A vote from the gateway: someone's, or the user's from elsewhere.
    pub fn poll_vote(&mut self, data: &Value, add: bool) {
        let (Some(channel), Some(message), Some(answer)) = (
            parse::id(&data["channel_id"]),
            parse::id(&data["message_id"]),
            data["answer_id"].as_u64(),
        ) else {
            return;
        };
        let mine = parse::id(&data["user_id"]) == Some(self.model.me);
        let Some(poll) = self
            .find_message(channel, message)
            .and_then(|m| m.poll.as_mut())
        else {
            return;
        };
        let Some(a) = poll.answers.iter_mut().find(|a| u64::from(a.id) == answer) else {
            return;
        };
        if mine {
            // Already counted when the user voted here.
            if a.me == add {
                return;
            }
            a.me = add;
        }
        a.count = if add {
            a.count + 1
        } else {
            a.count.saturating_sub(1)
        };
    }

    fn send_poll(&mut self) {
        let Some(draft) = &mut self.interact.poll else {
            return;
        };
        let answers: Vec<&str> = draft
            .answers
            .iter()
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .collect();
        if draft.question.trim().is_empty() {
            draft.error = Some("Escreva uma pergunta.".into());
            return;
        }
        if answers.len() < 2 {
            draft.error = Some("Uma enquete precisa de pelo menos duas respostas.".into());
            return;
        }
        let body = json!({
            "content": "",
            "nonce": crate::app::snowflake(jiff::Timestamp::now()).to_string(),
            "poll": {
                "question": { "text": draft.question.trim() },
                "answers": answers
                    .iter()
                    .map(|a| json!({ "poll_media": { "text": a } }))
                    .collect::<Vec<_>>(),
                "duration": draft.hours,
                "allow_multiselect": draft.multiple,
                "layout_type": 1,
            },
        });
        let channel = draft.channel;
        self.interact.poll = None;
        self.interact_api(
            Tag::Done,
            Method::POST,
            format!("/channels/{channel}/messages"),
            Some(body),
        );
    }

    /// A button pressed or a menu chosen on a bot's message.
    fn component(
        &mut self,
        channel: Id,
        message: Id,
        custom_id: String,
        kind: u8,
        values: Option<Vec<String>>,
    ) {
        let Some(m) = self
            .model
            .channel(channel)
            .and_then(|c| c.messages.iter().find(|m| m.id == message))
        else {
            return;
        };
        let Some(application) = m.application else {
            return;
        };
        let mut data = json!({ "component_type": kind, "custom_id": custom_id });
        if let Some(values) = values {
            data["type"] = json!(kind);
            data["values"] = json!(values);
        }
        let flags = m.flags;
        self.interaction(
            3,
            application,
            channel,
            Some((message, flags)),
            data,
            message,
            custom_id,
        );
    }

    /// Sends an interaction and remembers it until the gateway answers.
    #[allow(clippy::too_many_arguments)]
    fn interaction(
        &mut self,
        kind: u8,
        application: Id,
        channel: Id,
        on: Option<(Id, u64)>,
        data: Value,
        message: Id,
        component: String,
    ) {
        let nonce = self.nonce();
        let guild = self.model.channel(channel).and_then(|c| c.guild);
        let mut body = json!({
            "type": kind,
            "nonce": nonce,
            "application_id": application.to_string(),
            "channel_id": channel.to_string(),
            "session_id": self.model.session,
            "data": data,
        });
        if let Some(guild) = guild {
            body["guild_id"] = json!(guild.to_string());
        }
        if let Some((id, flags)) = on {
            body["message_id"] = json!(id.to_string());
            body["message_flags"] = json!(flags);
        }
        self.interact.failed.remove(&message);
        self.interact
            .pending
            .insert(nonce.clone(), (message, component));
        self.interact_api(
            Tag::Interaction { nonce },
            Method::POST,
            "/interactions".into(),
            Some(body),
        );
    }

    fn submit_modal_form(&mut self) {
        let Some(modal) = &mut self.interact.modal else {
            return;
        };
        if let Some(field) = modal
            .fields
            .iter()
            .find(|f| f.required && f.value.trim().is_empty())
        {
            modal.error = Some(format!("Preencha \"{}\".", field.label));
            return;
        }
        if let Some(field) = modal
            .fields
            .iter()
            .find(|f| !f.value.is_empty() && f.value.chars().count() < f.min)
        {
            modal.error = Some(format!(
                "\"{}\" precisa de pelo menos {} caracteres.",
                field.label, field.min
            ));
            return;
        }
        let components: Vec<Value> = modal
            .fields
            .iter()
            .map(|f| {
                let input = json!({ "type": 4, "custom_id": f.custom_id, "value": f.value });
                if f.labelled {
                    json!({ "type": 18, "component": input })
                } else {
                    json!({ "type": 1, "components": [input] })
                }
            })
            .collect();
        let modal = self.interact.modal.take().expect("checked above");
        let data = json!({
            "id": modal.id,
            "custom_id": modal.custom_id,
            "components": components,
        });
        self.interaction(
            5,
            modal.application,
            modal.channel,
            None,
            data,
            0,
            String::new(),
        );
    }

    /// The form Discord sends when a bot answers with one.
    pub fn modal_from(&mut self, data: &Value) {
        let mut fields = Vec::new();
        for item in data["components"].as_array().into_iter().flatten() {
            let (input, label, description, labelled) = if item["type"].as_u64() == Some(18) {
                (
                    &item["component"],
                    item["label"].as_str(),
                    item["description"].as_str(),
                    true,
                )
            } else {
                let input = &item["components"][0];
                (input, input["label"].as_str(), None, false)
            };
            if input["type"].as_u64() != Some(4) {
                continue;
            }
            let Some(custom_id) = input["custom_id"].as_str() else {
                continue;
            };
            fields.push(ModalField {
                custom_id: custom_id.to_owned(),
                label: label.unwrap_or_default().to_owned(),
                description: description.map(str::to_owned),
                style: input["style"].as_u64().unwrap_or(1) as u8,
                placeholder: input["placeholder"].as_str().map(str::to_owned),
                required: input["required"].as_bool().unwrap_or(true),
                value: input["value"].as_str().unwrap_or_default().to_owned(),
                min: input["min_length"].as_u64().unwrap_or(0) as usize,
                max: input["max_length"].as_u64().unwrap_or(4000) as usize,
                labelled,
            });
        }
        let (Some(id), Some(application), Some(channel)) = (
            data["id"].as_str(),
            parse::id(&data["application"]["id"]),
            parse::id(&data["channel_id"]),
        ) else {
            return;
        };
        self.interact.modal = Some(BotModal {
            id: id.to_owned(),
            application,
            channel,
            title: data["title"].as_str().unwrap_or_default().to_owned(),
            custom_id: data["custom_id"].as_str().unwrap_or_default().to_owned(),
            fields,
            error: None,
        });
    }

    /// The gateway's word on an interaction: failure marks its message.
    pub fn interaction_done(&mut self, data: &Value, ok: bool) {
        let Some(nonce) = data["nonce"].as_str() else {
            return;
        };
        if let Some((message, _)) = self.interact.pending.remove(nonce)
            && !ok
            && message != 0
        {
            self.interact.failed.insert(message, Instant::now());
        }
    }

    fn run_command(&mut self) {
        let Some(active) = self.interact.active.clone() else {
            return;
        };
        let Some(index) = self.interact.commands.get(&active.scope) else {
            return;
        };
        let Some(entry) = index.entries.get(active.entry).cloned() else {
            return;
        };
        let Some(cmd) = index.commands.get(entry.command).cloned() else {
            return;
        };
        let mut options = Vec::new();
        for (option, text) in entry.options.iter().zip(&active.values) {
            let text = text.trim();
            if text.is_empty() {
                if option.required {
                    if let Some(a) = &mut self.interact.active {
                        a.error = Some(format!("Preencha \"{}\".", option.name));
                    }
                    return;
                }
                continue;
            }
            let value = match self.option_value(option, text, active.channel) {
                Ok(value) => value,
                Err(error) => {
                    if let Some(a) = &mut self.interact.active {
                        a.error = Some(error);
                    }
                    return;
                }
            };
            options.push(json!({ "type": option.kind, "name": option.name, "value": value }));
        }
        // Subcommands and groups wrap the options, innermost first.
        let mut options = Value::Array(options);
        let mut kinds = vec![1u8; entry.path.len()];
        if entry.path.len() == 2 {
            kinds[0] = 2;
        }
        for (name, kind) in entry.path.iter().zip(kinds).rev() {
            options = json!([{ "type": kind, "name": name, "options": options }]);
        }
        let data = json!({
            "version": cmd.version,
            "id": cmd.id.to_string(),
            "name": cmd.name,
            "type": 1,
            "options": options,
            "application_command": cmd.raw,
            "attachments": [],
        });
        self.interact.active = None;
        self.chat.focus_composer = true;
        self.interaction(
            2,
            cmd.application,
            active.channel,
            None,
            data,
            0,
            String::new(),
        );
    }

    /// What the user typed for an option, in the form Discord wants.
    fn option_value(
        &self,
        option: &CommandOption,
        text: &str,
        channel: Id,
    ) -> Result<Value, String> {
        if let Some((_, value)) = option.choices.iter().find(|(name, _)| name == text) {
            return Ok(value.clone());
        }
        let wrong = || format!("\"{text}\" não serve para \"{}\".", option.name);
        Ok(match option.kind {
            4 => json!(text.parse::<i64>().map_err(|_| wrong())?),
            10 => json!(text.replace(',', ".").parse::<f64>().map_err(|_| wrong())?),
            5 => json!(matches!(
                text.to_lowercase().as_str(),
                "true" | "sim" | "s" | "yes" | "1"
            )),
            6..=9 => json!(
                self.resolve(option.kind, text, channel)
                    .ok_or_else(wrong)?
                    .to_string()
            ),
            _ => json!(text),
        })
    }

    /// A person, channel or role from a mention, an id or a name.
    fn resolve(&self, kind: u8, text: &str, channel: Id) -> Option<Id> {
        let digits: String = text.chars().filter(char::is_ascii_digit).collect();
        if digits.len() >= 15 {
            return digits.parse().ok();
        }
        let name = text.trim_start_matches(['@', '#']).to_lowercase();
        let guild = self.model.channel(channel).and_then(|c| c.guild);
        match kind {
            7 => guild
                .and_then(|g| self.model.guild(g))
                .and_then(|g| {
                    g.channels.iter().find(|c| {
                        self.model
                            .channel(**c)
                            .is_some_and(|c| c.name.to_lowercase() == name)
                    })
                })
                .copied(),
            8 => guild
                .and_then(|g| self.model.guild(g))
                .and_then(|g| g.roles.iter().find(|r| r.name.to_lowercase() == name))
                .map(|r| r.id),
            _ => self
                .model
                .users
                .values()
                .find(|u| {
                    u.username.to_lowercase() == name
                        || self.model.name_in(guild, u.id).to_lowercase() == name
                })
                .map(|u| u.id),
        }
    }

    pub fn interact_result(&mut self, tag: Tag, result: Result<Value, String>) {
        match (tag, result) {
            (Tag::Commands { scope }, Ok(value)) => {
                let index = self.interact.commands.entry(scope).or_default();
                merge_index(index, &value);
            }
            (Tag::Commands { .. }, Err(error)) => log::info!("commands: {error}"),
            (Tag::Interaction { nonce }, Err(error)) => {
                if let Some((message, _)) = self.interact.pending.remove(&nonce)
                    && message != 0
                {
                    self.interact.failed.insert(message, Instant::now());
                } else {
                    self.chat.dialog = Some(crate::chat::Dialog::Failed(error));
                }
            }
            (Tag::Done, Err(error)) => {
                self.chat.dialog = Some(crate::chat::Dialog::Failed(error));
            }
            _ => {}
        }
    }

    pub fn push_interact(&mut self, action: InteractAction) {
        self.actions.push(Action::Interact(action));
    }
}

/// Adds an index answer's commands, each once.
fn merge_index(index: &mut CommandIndex, value: &Value) {
    for app in value["applications"].as_array().into_iter().flatten() {
        if let (Some(id), Some(name)) = (parse::id(&app["id"]), app["name"].as_str()) {
            index.apps.insert(id, name.to_owned());
            let icon = match (
                app["icon"].as_str(),
                parse::id(&app["bot"]["id"]),
                app["bot"]["avatar"].as_str(),
            ) {
                (Some(hash), _, _) => Some(format!(
                    "https://cdn.discordapp.com/app-icons/{id}/{hash}.png?size=64"
                )),
                (None, Some(bot), Some(avatar)) => Some(format!(
                    "https://cdn.discordapp.com/avatars/{bot}/{avatar}.png?size=64"
                )),
                _ => None,
            };
            if let Some(icon) = icon {
                index.icons.insert(id, icon);
            }
        }
    }
    let list = value["application_commands"]
        .as_array()
        .or_else(|| value["commands"].as_array());
    for raw in list.into_iter().flatten() {
        let (Some(id), Some(application), Some(name)) = (
            parse::id(&raw["id"]),
            parse::id(&raw["application_id"]),
            raw["name"].as_str(),
        ) else {
            continue;
        };
        if index.commands.iter().any(|c| c.id == id) {
            continue;
        }
        let command = AppCommand {
            id,
            application,
            version: raw["version"].as_str().unwrap_or_default().to_owned(),
            name: name.to_owned(),
            description: raw["description"].as_str().unwrap_or_default().to_owned(),
            kind: raw["type"].as_u64().unwrap_or(1) as u8,
            raw: raw.clone(),
        };
        let at = index.commands.len();
        if command.kind == 1 {
            flatten(
                &mut index.entries,
                at,
                Vec::new(),
                &command.description,
                &raw["options"],
            );
        }
        index.commands.push(command);
    }
}

/// A command and each of its subcommands as entries to pick.
fn flatten(
    entries: &mut Vec<Entry>,
    command: usize,
    path: Vec<String>,
    description: &str,
    options: &Value,
) {
    let list = options.as_array().cloned().unwrap_or_default();
    let subs: Vec<&Value> = list
        .iter()
        .filter(|o| matches!(o["type"].as_u64(), Some(1 | 2)))
        .collect();
    if subs.is_empty() {
        entries.push(Entry {
            command,
            path,
            description: description.to_owned(),
            options: list
                .iter()
                .map(|o| CommandOption {
                    name: o["name"].as_str().unwrap_or_default().to_owned(),
                    description: o["description"].as_str().unwrap_or_default().to_owned(),
                    kind: o["type"].as_u64().unwrap_or(3) as u8,
                    required: o["required"].as_bool().unwrap_or(false),
                    choices: o["choices"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|c| Some((c["name"].as_str()?.to_owned(), c["value"].clone())))
                        .collect(),
                })
                .collect(),
        });
        return;
    }
    for sub in subs {
        let mut path = path.clone();
        path.push(sub["name"].as_str().unwrap_or_default().to_owned());
        flatten(
            entries,
            command,
            path,
            sub["description"].as_str().unwrap_or(description),
            &sub["options"],
        );
    }
}

/// How long a poll has left, or that it ended.
pub fn poll_time_left(poll: &Poll) -> String {
    if poll.closed() {
        return "Enquete encerrada".into();
    }
    let Some(expiry) = poll.expiry else {
        return String::new();
    };
    let seconds = (expiry.as_second() - jiff::Timestamp::now().as_second()).max(0);
    match seconds {
        s if s >= 86_400 => format!("{}d restantes", s / 86_400),
        s if s >= 3_600 => format!("{}h restantes", s / 3_600),
        s => format!("{}m restantes", (s / 60).max(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcommands_become_entries_with_their_own_options() {
        let raw = json!({
            "application_commands": [{
                "id": "10", "application_id": "20", "version": "1", "name": "music", "type": 1,
                "description": "Music",
                "options": [
                    { "type": 1, "name": "play", "description": "Play a song",
                      "options": [{ "type": 3, "name": "query", "required": true }] },
                    { "type": 2, "name": "queue", "description": "Queue", "options": [
                        { "type": 1, "name": "clear", "description": "Empty it" }
                    ]}
                ]
            }, {
                "id": "11", "application_id": "20", "version": "1", "name": "ping", "type": 1,
                "description": "Pong", "options": [{ "type": 4, "name": "times",
                  "choices": [{ "name": "one", "value": 1 }] }]
            }, {
                "id": "12", "application_id": "20", "version": "1", "name": "Quote", "type": 3
            }],
            "applications": [{ "id": "20", "name": "Bot", "icon": "abc" }]
        });
        let mut index = CommandIndex::default();
        merge_index(&mut index, &raw);
        merge_index(&mut index, &raw);
        assert_eq!(index.commands.len(), 3);
        let paths: Vec<Vec<String>> = index.entries.iter().map(|e| e.path.clone()).collect();
        assert_eq!(
            paths,
            vec![
                vec!["play".to_owned()],
                vec!["queue".into(), "clear".into()],
                vec![]
            ]
        );
        assert!(index.entries[0].options[0].required);
        assert_eq!(index.entries[2].options[0].choices[0].1, json!(1));
        assert_eq!(index.apps[&20], "Bot");
        assert_eq!(
            index.icon(20),
            "https://cdn.discordapp.com/app-icons/20/abc.png?size=64"
        );
    }
}
