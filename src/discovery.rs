//! Finding and joining servers, as Discord's "Explorar": the public
//! servers by category, joining one, and then what a server may ask of a
//! newcomer: agreeing to its rules (membership screening) and answering
//! its onboarding questions.

use std::collections::HashMap;

use reqwest::Method;
use serde_json::{Value, json};

use crate::app::{Action, App};
use crate::backend::Command;
use crate::discord::parse;
use crate::model::Id;

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    Categories,
    Guilds { category: Option<u32> },
    Joined { guild: Id },
    Screening { guild: Id },
    ScreeningDone { guild: Id },
    Onboarding { guild: Id },
    OnboardingDone,
}

/// A public server, as the discovery page shows it.
#[derive(Clone, Debug)]
pub struct Listed {
    pub id: Id,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub splash: Option<String>,
    pub members: u64,
    pub online: u64,
}

impl Listed {
    fn parse(value: &Value) -> Option<Self> {
        let id = parse::id(&value["id"])?;
        let cdn = crate::discord::CDN;
        Some(Self {
            id,
            name: value["name"].as_str().unwrap_or_default().to_owned(),
            description: value["description"].as_str().unwrap_or_default().to_owned(),
            icon: value["icon"]
                .as_str()
                .map(|h| format!("{cdn}/icons/{id}/{h}.png?size=96")),
            splash: value["discovery_splash"]
                .as_str()
                .or(value["splash"].as_str())
                .map(|h| format!("{cdn}/discovery-splashes/{id}/{h}.jpg?size=480")),
            members: value["approximate_member_count"].as_u64().unwrap_or(0),
            online: value["approximate_presence_count"].as_u64().unwrap_or(0),
        })
    }
}

#[derive(Default)]
pub struct Discovery {
    pub open: bool,
    /// Discord's categories: id and name.
    pub categories: Vec<(u32, String)>,
    pub category: Option<u32>,
    pub guilds: Option<Vec<Listed>>,
    pub search: String,
    pub joining: Option<Id>,
    pub error: Option<String>,
    /// The rules a server asks the user to agree to, and its version.
    pub screening: Option<Screening>,
    /// A server's onboarding questions while they are answered.
    pub onboarding: Option<Onboarding>,
}

#[derive(Clone, Debug)]
pub struct Screening {
    pub guild: Id,
    pub description: Option<String>,
    pub rules: Vec<String>,
    pub form: Value,
    pub version: Value,
    pub agreed: bool,
    pub busy: bool,
}

#[derive(Clone, Debug)]
pub struct Onboarding {
    pub guild: Id,
    pub prompts: Vec<Prompt>,
    /// The options picked, by prompt.
    pub picked: HashMap<Id, Vec<Id>>,
    pub step: usize,
}

#[derive(Clone, Debug)]
pub struct Prompt {
    pub id: Id,
    pub title: String,
    pub single: bool,
    pub required: bool,
    pub options: Vec<(Id, String, Option<String>)>,
}

#[derive(Clone, Debug)]
pub enum DiscoveryAction {
    Open,
    Close,
    Category(Option<u32>),
    Join(Id),
    /// A server is in the list now: asks what it wants of a newcomer.
    Welcome(Id),
    Agree(bool),
    SubmitScreening,
    CloseScreening,
    Pick { prompt: Id, option: Id },
    Step(usize),
    FinishOnboarding,
    SkipOnboarding,
}

impl App {
    fn discovery_api(&self, tag: Tag, method: Method, path: String, body: Option<Value>) {
        self.backend().send(Command::Api {
            tag: crate::backend::Tag::Discovery(tag),
            method,
            path,
            body,
        });
    }

    pub fn apply_discovery(&mut self, action: DiscoveryAction) {
        match action {
            DiscoveryAction::Open => {
                self.discovery.open = true;
                self.call_view = false;
                if self.discovery.categories.is_empty() {
                    self.discovery_api(
                        Tag::Categories,
                        Method::GET,
                        "/discovery/categories?locale=pt-BR&primary_only=true".into(),
                        None,
                    );
                }
                if self.discovery.guilds.is_none() {
                    self.apply_discovery(DiscoveryAction::Category(None));
                }
            }
            DiscoveryAction::Close => self.discovery.open = false,
            DiscoveryAction::Category(category) => {
                self.discovery.category = category;
                self.discovery.guilds = None;
                self.discovery.error = None;
                let filter = category.map_or_else(String::new, |c| format!("&categories={c}"));
                self.discovery_api(
                    Tag::Guilds { category },
                    Method::GET,
                    format!("/discoverable-guilds?offset=0&limit=48{filter}"),
                    None,
                );
            }
            DiscoveryAction::Join(guild) => {
                if self.model.guild(guild).is_some() {
                    self.discovery.open = false;
                    self.actions.push(Action::OpenServer(guild));
                    return;
                }
                self.discovery.joining = Some(guild);
                let session = self.model.session.clone();
                self.discovery_api(
                    Tag::Joined { guild },
                    Method::PUT,
                    format!("/guilds/{guild}/members/@me?lurker=false&session_id={session}"),
                    Some(json!({})),
                );
            }
            DiscoveryAction::Welcome(guild) => {
                let features = self
                    .model
                    .guild(guild)
                    .map(|g| g.features.clone())
                    .unwrap_or_default();
                let me = self.model.me;
                let pending = self
                    .model
                    .guild(guild)
                    .and_then(|g| g.members.get(&me))
                    .is_none_or(|m| m.pending);
                if features.iter().any(|f| f == "MEMBER_VERIFICATION_GATE_ENABLED") && pending {
                    self.discovery_api(
                        Tag::Screening { guild },
                        Method::GET,
                        format!("/guilds/{guild}/member-verification?with_guild=false"),
                        None,
                    );
                } else if features.iter().any(|f| f == "GUILD_ONBOARDING") {
                    self.discovery_api(
                        Tag::Onboarding { guild },
                        Method::GET,
                        format!("/guilds/{guild}/onboarding"),
                        None,
                    );
                }
            }
            DiscoveryAction::Agree(on) => {
                if let Some(s) = &mut self.discovery.screening {
                    s.agreed = on;
                }
            }
            DiscoveryAction::SubmitScreening => {
                let Some(s) = &mut self.discovery.screening else { return };
                if !s.agreed {
                    return;
                }
                s.busy = true;
                let mut form = s.form.clone();
                for field in form.as_array_mut().into_iter().flatten() {
                    field["response"] = json!(true);
                }
                let guild = s.guild;
                let body = json!({ "version": s.version, "form_fields": form });
                self.discovery_api(
                    Tag::ScreeningDone { guild },
                    Method::PUT,
                    format!("/guilds/{guild}/requests/@me"),
                    Some(body),
                );
            }
            DiscoveryAction::CloseScreening => self.discovery.screening = None,
            DiscoveryAction::Pick { prompt, option } => {
                let Some(o) = &mut self.discovery.onboarding else { return };
                let single = o.prompts.iter().find(|p| p.id == prompt).is_some_and(|p| p.single);
                let picked = o.picked.entry(prompt).or_default();
                if let Some(at) = picked.iter().position(|x| *x == option) {
                    picked.remove(at);
                } else {
                    if single {
                        picked.clear();
                    }
                    picked.push(option);
                }
            }
            DiscoveryAction::Step(step) => {
                if let Some(o) = &mut self.discovery.onboarding {
                    o.step = step.min(o.prompts.len().saturating_sub(1));
                }
            }
            DiscoveryAction::SkipOnboarding => self.discovery.onboarding = None,
            DiscoveryAction::FinishOnboarding => {
                let Some(o) = self.discovery.onboarding.take() else { return };
                let now = jiff::Timestamp::now().as_millisecond();
                let responses: Vec<String> = o.picked.values().flatten().map(|i| i.to_string()).collect();
                let prompts_seen: serde_json::Map<String, Value> =
                    o.prompts.iter().map(|p| (p.id.to_string(), json!(now))).collect();
                let options_seen: serde_json::Map<String, Value> = o
                    .prompts
                    .iter()
                    .flat_map(|p| p.options.iter().map(|(id, ..)| (id.to_string(), json!(now))))
                    .collect();
                self.discovery_api(
                    Tag::OnboardingDone,
                    Method::POST,
                    format!("/guilds/{}/onboarding-responses", o.guild),
                    Some(json!({
                        "onboarding_responses": responses,
                        "onboarding_prompts_seen": prompts_seen,
                        "onboarding_responses_seen": options_seen,
                    })),
                );
            }
        }
    }

    pub fn discovery_result(&mut self, tag: Tag, result: Result<Value, String>) {
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                match tag {
                    Tag::Joined { guild } => {
                        self.discovery.joining = None;
                        // Discord asks for a captcha to join from outside
                        // its apps: the browser takes over.
                        if error.to_lowercase().contains("captcha") {
                            self.apply_chat(crate::chat::ChatAction::OpenUrl(format!(
                                "https://discord.com/servers/{guild}"
                            )));
                        } else {
                            self.discovery.error = Some(error);
                        }
                    }
                    Tag::ScreeningDone { .. } => {
                        if let Some(s) = &mut self.discovery.screening {
                            s.busy = false;
                        }
                        self.chat.dialog = Some(crate::chat::Dialog::Failed(error));
                    }
                    Tag::Guilds { .. } => self.discovery.error = Some(error),
                    _ => log::warn!("discovery: {error}"),
                }
                return;
            }
        };
        match tag {
            Tag::Categories => {
                self.discovery.categories = value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| {
                        let id = c["id"].as_u64()? as u32;
                        let name = c["name"]["localizations"]["pt-BR"]
                            .as_str()
                            .or(c["name"]["default"].as_str())
                            .or(c["name"].as_str())?;
                        Some((id, name.to_owned()))
                    })
                    .collect();
            }
            Tag::Guilds { category } => {
                if self.discovery.category != category {
                    return;
                }
                let list = value["guilds"].as_array().or(value.as_array());
                self.discovery.guilds =
                    Some(list.into_iter().flatten().filter_map(Listed::parse).collect());
            }
            Tag::Joined { guild } => {
                self.discovery.joining = None;
                self.discovery.open = false;
                if self.model.guild(guild).is_some() {
                    self.actions.push(Action::OpenServer(guild));
                    self.apply_discovery(DiscoveryAction::Welcome(guild));
                } else {
                    self.admin.open_when_ready = Some(guild);
                }
            }
            Tag::Screening { guild } => {
                let form = value["form_fields"].clone();
                let rules = form
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|f| f["field_type"].as_str() == Some("TERMS"))
                    .flat_map(|f| f["values"].as_array().cloned().unwrap_or_default())
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                self.discovery.screening = Some(Screening {
                    guild,
                    description: value["description"].as_str().map(str::to_owned),
                    rules,
                    form,
                    version: value["version"].clone(),
                    agreed: false,
                    busy: false,
                });
            }
            Tag::ScreeningDone { guild } => {
                self.discovery.screening = None;
                let me = self.model.me;
                if let Some(m) = self.model.guild_mut(guild).and_then(|g| g.members.get_mut(&me)) {
                    m.pending = false;
                }
                if self
                    .model
                    .guild(guild)
                    .is_some_and(|g| g.features.iter().any(|f| f == "GUILD_ONBOARDING"))
                {
                    self.discovery_api(
                        Tag::Onboarding { guild },
                        Method::GET,
                        format!("/guilds/{guild}/onboarding"),
                        None,
                    );
                }
            }
            Tag::Onboarding { guild } => {
                if value["enabled"].as_bool() == Some(false) {
                    return;
                }
                let prompts: Vec<Prompt> = value["prompts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|p| p["in_onboarding"].as_bool().unwrap_or(true))
                    .filter_map(|p| {
                        Some(Prompt {
                            id: parse::id(&p["id"])?,
                            title: p["title"].as_str().unwrap_or_default().to_owned(),
                            single: p["single_select"].as_bool().unwrap_or(false),
                            required: p["required"].as_bool().unwrap_or(false),
                            options: p["options"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|o| {
                                    let emoji = o["emoji"]["name"]
                                        .as_str()
                                        .or(o["emoji_name"].as_str())
                                        .map(str::to_owned);
                                    let title = o["title"].as_str().unwrap_or_default();
                                    let title = match &emoji {
                                        Some(e) => format!("{e} {title}"),
                                        None => title.to_owned(),
                                    };
                                    Some((
                                        parse::id(&o["id"])?,
                                        title,
                                        o["description"].as_str().filter(|d| !d.is_empty()).map(str::to_owned),
                                    ))
                                })
                                .collect(),
                        })
                    })
                    .collect();
                if !prompts.is_empty() {
                    self.discovery.onboarding = Some(Onboarding {
                        guild,
                        prompts,
                        picked: HashMap::new(),
                        step: 0,
                    });
                }
            }
            Tag::OnboardingDone => {}
        }
    }
}
