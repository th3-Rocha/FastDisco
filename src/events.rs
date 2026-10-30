//! A server's scheduled events, as Discord's "Eventos" dialog: the list
//! with when and where each happens, "Tenho interesse", and for those
//! allowed, creating, starting, ending and cancelling events.

use reqwest::Method;
use serde_json::{Value, json};

use crate::app::App;
use crate::backend::Command;
use crate::discord::parse;
use crate::model::Id;

/// What an API answer is for.
#[derive(Clone, Debug)]
pub enum Tag {
    List { guild: Id },
    /// Created or changed; the list is asked for again.
    Changed { guild: Id },
    Interest { event: Id, on: bool },
    /// The events the user is interested in, which the list does not say.
    Mine,
    /// Who is interested in an event, for its details.
    Users { event: Id },
}

/// One event, as the dialog shows it.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: Id,
    pub name: String,
    pub description: Option<String>,
    pub start: Option<jiff::Timestamp>,
    pub end: Option<jiff::Timestamp>,
    /// How it repeats, for a series.
    pub recurrence: Option<Recurrence>,
    /// 1 scheduled, 2 active.
    pub status: u8,
    /// A voice or stage channel, or a place outside Discord.
    pub channel: Option<Id>,
    pub location: Option<String>,
    pub interested: u32,
    pub me: bool,
    pub creator: Option<Id>,
    pub image: Option<String>,
}

impl Item {
    fn parse(value: &Value) -> Option<Self> {
        let id = parse::id(&value["id"])?;
        Some(Self {
            id,
            name: value["name"].as_str().unwrap_or_default().to_owned(),
            description: value["description"]
                .as_str()
                .filter(|d| !d.is_empty())
                .map(str::to_owned),
            start: value["scheduled_start_time"].as_str().and_then(|t| t.parse().ok()),
            end: value["scheduled_end_time"].as_str().and_then(|t| t.parse().ok()),
            recurrence: Recurrence::parse(&value["recurrence_rule"]),
            status: value["status"].as_u64().unwrap_or(1) as u8,
            channel: parse::id(&value["channel_id"]),
            location: value["entity_metadata"]["location"].as_str().map(str::to_owned),
            interested: value["user_count"].as_u64().unwrap_or(0) as u32,
            me: false,
            creator: parse::id(&value["creator_id"]),
            image: value["image"].as_str().map(|hash| {
                format!("{}/guild-events/{id}/{hash}.png?size=512", crate::discord::CDN)
            }),
        })
    }
}

/// An event series' rule, as Discord's `recurrence_rule`: every so many
/// days, weeks (on some weekdays), months or years from its first start.
#[derive(Clone, Debug)]
pub struct Recurrence {
    pub start: jiff::Timestamp,
    pub end: Option<jiff::Timestamp>,
    /// 0 yearly, 1 monthly, 2 weekly, 3 daily.
    pub frequency: u8,
    pub interval: u32,
    /// Monday is 0, as Discord counts.
    pub weekdays: Vec<u8>,
    /// "The second Tuesday": (week of the month, weekday).
    pub nth_weekday: Option<(i8, u8)>,
    pub count: Option<u32>,
}

impl Recurrence {
    fn parse(value: &Value) -> Option<Self> {
        Some(Self {
            start: value["start"].as_str()?.parse().ok()?,
            end: value["end"].as_str().and_then(|t| t.parse().ok()),
            frequency: value["frequency"].as_u64()? as u8,
            interval: value["interval"].as_u64().unwrap_or(1).max(1) as u32,
            weekdays: value["by_weekday"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| d.as_u64().map(|d| d as u8))
                .collect(),
            nth_weekday: value["by_n_weekday"]
                .as_array()
                .and_then(|l| l.first())
                .and_then(|n| Some((n["n"].as_i64()? as i8, n["day"].as_u64()? as u8))),
            count: value["count"].as_u64().map(|c| c as u32),
        })
    }

    /// The starts after `after`, at most `limit` of them, in the local
    /// time zone so a weekly event keeps its hour across clock changes.
    pub fn next(&self, after: jiff::Timestamp, limit: usize) -> Vec<jiff::Timestamp> {
        let tz = jiff::tz::TimeZone::system();
        let first = self.start.to_zoned(tz.clone());
        let mut out = Vec::new();
        let mut index = 0u32;
        // Days for daily and weekly rules, months for the rest.
        for step in 0..(3 * 366) {
            let candidates: Vec<jiff::Zoned> = match self.frequency {
                3 => {
                    if step % self.interval != 0 {
                        continue;
                    }
                    first.checked_add(jiff::Span::new().days(i64::from(step))).ok().into_iter().collect()
                }
                2 => {
                    let Ok(day) = first.checked_add(jiff::Span::new().days(i64::from(step))) else {
                        break;
                    };
                    let week = step / 7;
                    let weekday = day.weekday().to_monday_zero_offset() as u8;
                    let on = if self.weekdays.is_empty() {
                        step % 7 == 0
                    } else {
                        self.weekdays.contains(&weekday)
                    };
                    if !on || week % self.interval != 0 {
                        continue;
                    }
                    vec![day]
                }
                1 | 0 => {
                    let months = if self.frequency == 0 { 12 } else { 1 } * self.interval;
                    if step > 600 {
                        break;
                    }
                    let Ok(month) = first.checked_add(jiff::Span::new().months(i64::from(step * months))) else {
                        break;
                    };
                    match self.nth_weekday {
                        Some((n, weekday)) => {
                            let target = jiff::civil::Weekday::from_monday_zero_offset(weekday as i8)
                                .unwrap_or(jiff::civil::Weekday::Monday);
                            month
                                .date()
                                .nth_weekday_of_month(n, target)
                                .ok()
                                .and_then(|d| d.to_datetime(month.time()).to_zoned(tz.clone()).ok())
                                .into_iter()
                                .collect()
                        }
                        None => vec![month],
                    }
                }
                _ => break,
            };
            for start in candidates {
                let at = start.timestamp();
                if at < self.start {
                    continue;
                }
                index += 1;
                if self.count.is_some_and(|c| index > c) || self.end.is_some_and(|e| at > e) {
                    return out;
                }
                if at > after {
                    out.push(at);
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
        }
        out
    }
}

/// "ter nov 3º · 14:30", as Discord writes an event's time in Portuguese.
pub fn when(at: jiff::Timestamp) -> String {
    const DAYS: [&str; 7] = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"];
    const MONTHS: [&str; 12] = [
        "jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez",
    ];
    let local = at.to_zoned(jiff::tz::TimeZone::system());
    format!(
        "{} {} {}º · {}",
        DAYS[local.weekday().to_monday_zero_offset() as usize],
        MONTHS[(local.month() - 1) as usize],
        local.day(),
        local.strftime("%H:%M"),
    )
}

/// The form for a new event.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    /// A voice or stage channel; none is somewhere else.
    pub channel: Option<Id>,
    pub location: String,
    pub name: String,
    pub description: String,
    /// "28/10/2026" and "20:30", in the local time zone.
    pub date: String,
    pub time: String,
    /// How long an event elsewhere lasts, in hours: Discord needs an end.
    pub hours: u32,
    pub busy: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Events {
    /// The server whose events the dialog shows.
    pub guild: Option<Id>,
    pub list: Option<Vec<Item>>,
    pub draft: Option<Draft>,
    pub error: Option<String>,
    /// The events the user said they are interested in.
    mine: Vec<Id>,
    /// The event whose details show, over the list when `from_list`.
    pub detail: Option<Id>,
    pub from_list: bool,
    /// The details' tab: false the information, true who is interested.
    pub people_tab: bool,
    /// More of a series' coming dates.
    pub more_dates: bool,
    /// Who is interested, by event, once asked for.
    pub people: std::collections::HashMap<Id, Vec<Id>>,
    /// Live events whose banner the user closed, this session.
    pub dismissed: Vec<Id>,
}

#[derive(Clone, Debug)]
pub enum EventAction {
    Open(Id),
    /// One event's details: from the banner, or from the list.
    Details { guild: Id, event: Id },
    PeopleTab(bool),
    MoreDates,
    /// The banner of a live event is closed.
    Dismiss(Id),
    Close,
    Interest(Id, bool),
    StartDraft,
    CancelDraft,
    SubmitDraft,
    /// 2 starts it, 3 ends it, 4 cancels it.
    SetStatus(Id, u8),
}

impl App {
    fn events_api(&self, tag: Tag, method: Method, path: String, body: Option<Value>) {
        self.backend().send(Command::Api {
            tag: crate::backend::Tag::Events(tag),
            method,
            path,
            body,
        });
    }

    fn load_events(&self, guild: Id) {
        self.events_api(
            Tag::Mine,
            Method::GET,
            format!("/users/@me/scheduled-events?guild_ids={guild}"),
            None,
        );
        self.events_api(
            Tag::List { guild },
            Method::GET,
            format!("/guilds/{guild}/scheduled-events?with_user_count=true"),
            None,
        );
    }

    pub fn apply_events(&mut self, action: EventAction) {
        match action {
            EventAction::Open(guild) => {
                let dismissed = std::mem::take(&mut self.events.dismissed);
                self.events = Events {
                    guild: Some(guild),
                    dismissed,
                    ..Events::default()
                };
                self.load_events(guild);
            }
            EventAction::Details { guild, event } => {
                let from_list = self.events.guild == Some(guild) && self.events.detail.is_none();
                if !from_list {
                    let dismissed = std::mem::take(&mut self.events.dismissed);
                    self.events = Events {
                        guild: Some(guild),
                        dismissed,
                        ..Events::default()
                    };
                    self.load_events(guild);
                }
                self.events.detail = Some(event);
                self.events.from_list = from_list;
                self.events.people_tab = false;
                self.events.more_dates = false;
                self.events_api(
                    Tag::Users { event },
                    Method::GET,
                    format!("/guilds/{guild}/scheduled-events/{event}/users?limit=100&with_member=true"),
                    None,
                );
            }
            EventAction::PeopleTab(on) => self.events.people_tab = on,
            EventAction::MoreDates => self.events.more_dates = true,
            EventAction::Dismiss(event) => self.events.dismissed.push(event),
            // Closing the details goes back to the list they came from.
            EventAction::Close if self.events.detail.is_some() && self.events.from_list => {
                self.events.detail = None;
            }
            EventAction::Close => {
                let dismissed = std::mem::take(&mut self.events.dismissed);
                self.events = Events {
                    dismissed,
                    ..Events::default()
                };
            }
            EventAction::Interest(event, on) => {
                let Some(guild) = self.events.guild else { return };
                if let Some(item) = self
                    .events
                    .list
                    .as_mut()
                    .and_then(|l| l.iter_mut().find(|i| i.id == event))
                {
                    item.me = on;
                    item.interested = if on {
                        item.interested + 1
                    } else {
                        item.interested.saturating_sub(1)
                    };
                }
                self.events_api(
                    Tag::Interest { event, on },
                    if on { Method::PUT } else { Method::DELETE },
                    format!("/guilds/{guild}/scheduled-events/{event}/users/@me"),
                    None,
                );
            }
            EventAction::StartDraft => {
                let now = jiff::Zoned::now();
                let start = now.checked_add(jiff::SignedDuration::from_hours(1)).unwrap_or(now);
                let channel = self.events.guild.and_then(|g| {
                    let guild = self.model.guild(g)?;
                    guild
                        .channels
                        .iter()
                        .filter_map(|c| self.model.channel(*c))
                        .find(|c| c.kind.is_voice() && self.model.can_view(c))
                        .map(|c| c.id)
                });
                self.events.draft = Some(Draft {
                    channel,
                    date: start.strftime("%d/%m/%Y").to_string(),
                    time: start.strftime("%H:00").to_string(),
                    hours: 1,
                    ..Draft::default()
                });
            }
            EventAction::CancelDraft => self.events.draft = None,
            EventAction::SubmitDraft => self.submit_event(),
            EventAction::SetStatus(event, status) => {
                let Some(guild) = self.events.guild else { return };
                self.events_api(
                    Tag::Changed { guild },
                    Method::PATCH,
                    format!("/guilds/{guild}/scheduled-events/{event}"),
                    Some(json!({ "status": status })),
                );
            }
        }
    }

    fn submit_event(&mut self) {
        let Some(guild) = self.events.guild else { return };
        let Some(draft) = &mut self.events.draft else {
            return;
        };
        let start = match local_time(&draft.date, &draft.time) {
            Some(start) if start > jiff::Timestamp::now() => start,
            Some(_) => {
                draft.error = Some("O início precisa estar no futuro.".into());
                return;
            }
            None => {
                draft.error = Some("Use a data como 28/10/2026 e a hora como 20:30.".into());
                return;
            }
        };
        if draft.name.trim().is_empty() {
            draft.error = Some("Dê um nome ao evento.".into());
            return;
        }
        let mut body = json!({
            "name": draft.name.trim(),
            "privacy_level": 2,
            "scheduled_start_time": start.to_string(),
            "description": draft.description.trim(),
        });
        match draft.channel {
            Some(channel) => {
                let stage = self
                    .model
                    .channel(channel)
                    .is_some_and(|c| c.kind == crate::model::ChannelKind::Stage);
                body["entity_type"] = json!(if stage { 1 } else { 2 });
                body["channel_id"] = json!(channel.to_string());
            }
            None => {
                if draft.location.trim().is_empty() {
                    draft.error = Some("Diga onde o evento acontece.".into());
                    return;
                }
                let end = start + jiff::SignedDuration::from_hours(i64::from(draft.hours.max(1)));
                body["entity_type"] = json!(3);
                body["entity_metadata"] = json!({ "location": draft.location.trim() });
                body["scheduled_end_time"] = json!(end.to_string());
            }
        }
        draft.busy = true;
        draft.error = None;
        self.events_api(
            Tag::Changed { guild },
            Method::POST,
            format!("/guilds/{guild}/scheduled-events"),
            Some(body),
        );
    }

    pub fn events_result(&mut self, tag: Tag, result: Result<Value, String>) {
        let value = match result {
            Ok(value) => value,
            Err(error) => {
                if let Tag::Mine | Tag::Users { .. } = tag {
                    log::debug!("event interests: {error}");
                    return;
                }
                if let Some(draft) = &mut self.events.draft {
                    draft.busy = false;
                    draft.error = Some(error);
                } else {
                    self.events.error = Some(error);
                }
                return;
            }
        };
        match tag {
            Tag::List { guild } => {
                if self.events.guild != Some(guild) {
                    return;
                }
                let mut list: Vec<Item> = value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Item::parse)
                    .filter(|e| e.status <= 2)
                    .collect();
                list.sort_by_key(|e| (e.status != 2, e.start));
                for item in &mut list {
                    item.me = self.events.mine.contains(&item.id);
                }
                self.events.list = Some(list);
            }
            Tag::Changed { guild } => {
                self.events.draft = None;
                self.load_events(guild);
            }
            Tag::Interest { event, on } => {
                self.events.mine.retain(|e| *e != event);
                if on {
                    self.events.mine.push(event);
                }
            }
            Tag::Users { event } => {
                let mut people = Vec::new();
                for entry in value.as_array().into_iter().flatten() {
                    if let Some(user) = parse::user(&entry["user"]) {
                        people.push(user.id);
                        self.model.users.entry(user.id).or_insert(user);
                    }
                }
                self.events.people.insert(event, people);
            }
            Tag::Mine => {
                self.events.mine = value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| parse::id(&r["guild_scheduled_event_id"]))
                    .collect();
                let mine = self.events.mine.clone();
                for item in self.events.list.iter_mut().flatten() {
                    item.me = mine.contains(&item.id);
                }
            }
        }
    }
}

/// "28/10/2026" and "20:30" in the local time zone.
fn local_time(date: &str, time: &str) -> Option<jiff::Timestamp> {
    let mut d = date.trim().split('/').map(|p| p.trim().parse::<i32>().ok());
    let (day, month, year) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.trim().split(':').map(|p| p.trim().parse::<i32>().ok());
    let (hour, minute) = (t.next()??, t.next().flatten().unwrap_or(0));
    let civil = jiff::civil::DateTime::new(
        i16::try_from(year).ok()?,
        i8::try_from(month).ok()?,
        i8::try_from(day).ok()?,
        i8::try_from(hour).ok()?,
        i8::try_from(minute).ok()?,
        0,
        0,
    )
    .ok()?;
    civil
        .to_zoned(jiff::tz::TimeZone::system())
        .ok()
        .map(|z| z.timestamp())
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates_are_read_day_first() {
        assert!(super::local_time("28/10/2026", "20:30").is_some());
        assert!(super::local_time("2026-10-28", "20:30").is_none());
        assert!(super::local_time("31/02/2026", "10:00").is_none());
    }

    /// Weekly on Tuesdays, as the screenshot's series: the next four.
    #[test]
    fn a_weekly_series_comes_back_each_tuesday() {
        let rule = super::Recurrence::parse(&serde_json::json!({
            "start": "2026-10-06T17:30:00Z",
            "frequency": 2,
            "interval": 1,
            "by_weekday": [1],
        }))
        .unwrap();
        let after: jiff::Timestamp = "2026-10-30T12:00:00Z".parse().unwrap();
        let next = rule.next(after, 4);
        assert_eq!(next.len(), 4);
        let days: Vec<_> = next
            .iter()
            .map(|t| t.to_zoned(jiff::tz::TimeZone::UTC).weekday())
            .collect();
        assert!(days.iter().all(|d| *d == jiff::civil::Weekday::Tuesday), "{days:?}");
        assert!(next[0] > after);
        assert_eq!(next[1].duration_since(next[0]).as_hours(), 24 * 7);
    }
}
