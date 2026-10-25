//! What messages carry beyond text, and the dialogs around them: a
//! forwarded message, a poll, bots' buttons, menus and layouts, the forms
//! bots open, the forward and new poll dialogs, and a slash command being
//! filled in.

use egui::{
    Align2, Color32, CornerRadius, Frame, Key, Margin, Modifiers, Rect, RichText, Sense, Stroke,
    Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::message::{self, Row, TextStyle};
use super::widgets::{Elide, avatar, font, paint_icon};
use crate::app::{Action, App};
use crate::icons::Icon;
use crate::interact::{InteractAction, poll_time_left};
use crate::markdown;
use crate::model::{
    Button, ChannelKind, Component, CustomEmoji, Id, Message, ReactionEmoji, Select,
};

fn act(out: &mut Vec<Action>, action: InteractAction) {
    out.push(Action::Interact(action));
}

const SECONDARY: Color32 = Color32::from_rgb(0x4e, 0x50, 0x58);
const SUCCESS: Color32 = Color32::from_rgb(0x24, 0x80, 0x46);
const DANGER: Color32 = Color32::from_rgb(0xda, 0x37, 0x3c);
const BLURPLE: Color32 = Color32::from_rgb(0x58, 0x65, 0xf2);

/// A Unicode emoji or a server's own, at `rect`.
pub fn paint_emoji(ui: &Ui, app: &App, emoji: &ReactionEmoji, rect: Rect) {
    match emoji {
        ReactionEmoji::Unicode(text) => {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                font(Weight::Regular, rect.height() * 0.85),
                app.palette.text,
            );
        }
        ReactionEmoji::Custom { id, animated, .. } => {
            if let Some(uri) = app.picture(Some(&CustomEmoji::url(*id, *animated))) {
                egui::Image::new(uri).paint_at(ui, rect);
            }
        }
    }
}

/// Markdown text in the message's own style.
fn rich(row: &mut Row<'_>, ui: &mut Ui, message: &Message, text: &str) {
    let parsed = markdown::parse(text);
    let style = TextStyle::message(row.app, message, false);
    message::blocks(row, ui, message.id, &parsed, &style, false);
}

/// "Encaminhada", the message as it was, and where it came from.
pub fn forwarded(row: &mut Row<'_>, ui: &mut Ui, message: &Message) {
    let Some(forward) = &message.forward else {
        return;
    };
    let palette = row.app.palette;
    let top = ui.cursor().top();
    let left = ui.cursor().left();
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                paint_icon(ui, Icon::ReplyArrow, r.center(), 14.0, palette.muted);
                ui.label(
                    RichText::new("Encaminhada")
                        .font(font(Weight::Medium, 13.0))
                        .italics()
                        .color(palette.muted),
                );
            });
            let mut shown = Message::plain(
                message.author,
                forward.time.unwrap_or(message.time),
                forward.text.clone(),
            );
            shown.id = message.id;
            shown.attachments = forward.attachments.clone();
            shown.embeds = forward.embeds.clone();
            message::body(row, ui, &shown);
            message::media(row, ui, &shown);
            // Where it was said, which opens it there.
            if let (Some(channel), Some(id)) = (forward.channel, forward.message) {
                let place = row
                    .app
                    .model
                    .channel(channel)
                    .map(|c| match c.kind {
                        ChannelKind::Direct | ChannelKind::Group => row.app.model.direct_name(c),
                        _ => format!("#{}", c.name),
                    })
                    .unwrap_or_else(|| "Mensagem original".into());
                let place = match forward.guild.and_then(|g| row.app.model.guild(g)) {
                    Some(g) if row.guild != Some(g.id) => format!("{place}  •  {}", g.name),
                    _ => place,
                };
                let when = forward
                    .time
                    .map(|t| format!("  •  {}", super::widgets::timestamp(t)))
                    .unwrap_or_default();
                let link = ui.add(
                    egui::Label::new(
                        RichText::new(format!("{place}{when}"))
                            .font(font(Weight::Medium, 12.0))
                            .color(palette.muted),
                    )
                    .sense(Sense::click()),
                );
                if link.clicked() {
                    row.out.push(Action::Nav(crate::nav::NavAction::Jump {
                        channel,
                        message: id,
                    }));
                }
            }
        });
    });
    let bottom = ui.cursor().top();
    ui.painter().rect_filled(
        Rect::from_min_max(pos2(left, top), pos2(left + 4.0, bottom)),
        CornerRadius::same(2),
        palette.divider,
    );
}

/// Discord's poll: the question, the answers to pick or their results,
/// the votes and the time left, and voting.
pub fn poll(row: &mut Row<'_>, ui: &mut Ui, message: &Message) {
    let Some(poll) = &message.poll else {
        return;
    };
    let app = row.app;
    let palette = app.palette;
    let picked = app
        .interact
        .poll_choice
        .get(&message.id)
        .cloned()
        .unwrap_or_default();
    let results = poll.voted() || poll.closed() || app.interact.poll_results.contains(&message.id);
    let total = poll.total().max(1);
    let most = poll.answers.iter().map(|a| a.count).max().unwrap_or(0);
    let mut out = Vec::new();
    ui.add_space(4.0);
    Frame::new()
        .fill(palette.sidebar)
        .corner_radius(CornerRadius::same(8))
        .stroke(Stroke::new(1.0, palette.divider))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().min(440.0));
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            ui.label(
                RichText::new(&poll.question)
                    .font(font(Weight::SemiBold, 17.0))
                    .color(palette.strong),
            );
            let hint = if poll.closed() {
                "Enquete encerrada"
            } else if poll.multiple {
                "Selecione uma ou mais respostas"
            } else {
                "Selecione uma resposta"
            };
            ui.label(
                RichText::new(hint)
                    .font(font(Weight::Medium, 13.0))
                    .color(palette.muted),
            );
            for answer in &poll.answers {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
                let chosen = picked.contains(&answer.id);
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(8), palette.chat);
                if results {
                    let share = answer.count as f32 / total as f32;
                    let bar =
                        Rect::from_min_size(rect.min, vec2(rect.width() * share, rect.height()));
                    let fill = if answer.me {
                        palette.accent.gamma_multiply(0.45)
                    } else {
                        palette.selected
                    };
                    ui.painter().rect_filled(bar, CornerRadius::same(8), fill);
                } else if response.hovered() || chosen {
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(8),
                        Stroke::new(
                            1.0,
                            if chosen {
                                palette.accent
                            } else {
                                palette.muted
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                }
                let mut x = rect.left() + 14.0;
                if let Some(emoji) = &answer.emoji {
                    paint_emoji(
                        ui,
                        app,
                        emoji,
                        Rect::from_center_size(pos2(x + 10.0, rect.center().y), Vec2::splat(20.0)),
                    );
                    x += 28.0;
                }
                let right = rect.right() - 14.0;
                let mut text_right = right - 30.0;
                if results {
                    let percent = (answer.count as f32 * 100.0 / total as f32).round() as u32;
                    let detail = format!("{} votos  {percent}%", answer.count);
                    let g = ui.painter().layout_no_wrap(
                        detail,
                        font(Weight::SemiBold, 13.0),
                        palette.strong,
                    );
                    let w = g.size().x;
                    ui.painter().galley(
                        pos2(right - w, rect.center().y - g.size().y / 2.0),
                        g,
                        palette.strong,
                    );
                    text_right = right - w - 12.0;
                    if answer.me {
                        paint_icon(
                            ui,
                            Icon::Check,
                            pos2(right - w - 22.0, rect.center().y),
                            16.0,
                            palette.online,
                        );
                        text_right -= 24.0;
                    }
                    if poll.closed() && answer.count == most && most > 0 {
                        // The winner is in bold, as Discord marks it.
                    }
                } else {
                    // A round box for one answer, a square for many.
                    let center = pos2(right - 10.0, rect.center().y);
                    let corner = if poll.multiple { 4 } else { 10 };
                    let r = Rect::from_center_size(center, Vec2::splat(20.0));
                    if chosen {
                        ui.painter()
                            .rect_filled(r, CornerRadius::same(corner), palette.accent);
                        paint_icon(ui, Icon::Check, center, 14.0, Color32::WHITE);
                    } else {
                        ui.painter().rect_stroke(
                            r,
                            CornerRadius::same(corner),
                            Stroke::new(2.0, palette.muted),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                let weight = if results && poll.closed() && answer.count == most && most > 0 {
                    Weight::Bold
                } else {
                    Weight::Medium
                };
                Elide::new(ui, text_right).text(
                    pos2(x, rect.center().y),
                    Align2::LEFT_CENTER,
                    answer.text.clone(),
                    font(weight, 15.0),
                    palette.strong,
                );
                if response.clicked() && !results {
                    act(
                        &mut out,
                        InteractAction::PickAnswer {
                            message: message.id,
                            answer: answer.id,
                            multiple: poll.multiple,
                        },
                    );
                }
            }
            ui.horizontal(|ui| {
                let votes = poll.total();
                let count = if votes == 1 {
                    "1 voto".to_owned()
                } else {
                    format!("{votes} votos")
                };
                ui.label(
                    RichText::new(format!("{count}  •  {}", poll_time_left(poll)))
                        .font(font(Weight::Medium, 13.0))
                        .color(palette.muted),
                );
                let link = |ui: &mut Ui, text: &str| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(text)
                                .font(font(Weight::SemiBold, 13.0))
                                .color(palette.text),
                        )
                        .sense(Sense::click()),
                    )
                    .clicked()
                };
                if !poll.closed() {
                    if !poll.voted() {
                        ui.add_space(8.0);
                        let label = if results {
                            "Voltar a votar"
                        } else {
                            "Mostrar resultados"
                        };
                        if link(ui, label) {
                            act(&mut out, InteractAction::ShowResults(message.id));
                        }
                    } else {
                        ui.add_space(8.0);
                        if link(ui, "Remover voto") {
                            act(
                                &mut out,
                                InteractAction::RemoveVote {
                                    channel: row.channel,
                                    message: message.id,
                                },
                            );
                        }
                    }
                    if message.author == app.model.me {
                        ui.add_space(8.0);
                        if link(ui, "Encerrar enquete") {
                            act(
                                &mut out,
                                InteractAction::EndPoll {
                                    channel: row.channel,
                                    message: message.id,
                                },
                            );
                        }
                    }
                }
                if !results {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let vote = egui::Button::new(
                            RichText::new("Votar")
                                .font(font(Weight::Medium, 14.0))
                                .color(Color32::WHITE),
                        )
                        .fill(palette.accent)
                        .corner_radius(CornerRadius::same(8))
                        .min_size(vec2(72.0, 32.0));
                        if ui.add_enabled(!picked.is_empty(), vote).clicked() {
                            act(
                                &mut out,
                                InteractAction::Vote {
                                    channel: row.channel,
                                    message: message.id,
                                },
                            );
                        }
                    });
                }
            });
        });
    row.out.extend(out);
}

/// A bot's buttons, menus and layout, and a failed interaction's note.
pub fn components(row: &mut Row<'_>, ui: &mut Ui, message: &Message) {
    if message.components.is_empty() {
        return;
    }
    ui.add_space(4.0);
    for component in &message.components {
        draw(row, ui, message, component);
    }
    if row
        .app
        .interact
        .failed
        .get(&message.id)
        .is_some_and(|at| at.elapsed().as_secs() < 15)
    {
        ui.add_space(4.0);
        ui.label(
            RichText::new("A interação falhou")
                .font(font(Weight::Medium, 13.0))
                .color(row.app.palette.danger),
        );
    }
}

fn draw(row: &mut Row<'_>, ui: &mut Ui, message: &Message, component: &Component) {
    let palette = row.app.palette;
    match component {
        Component::Row(items) => {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                for item in items {
                    draw(row, ui, message, item);
                }
            });
            ui.add_space(4.0);
        }
        Component::Button(button) => draw_button(row, ui, message, button),
        Component::Select(select) => draw_select(row, ui, message, select),
        Component::Text(text) => rich(row, ui, message, text),
        Component::Section { texts, accessory } => {
            ui.horizontal(|ui| {
                let width = ui.available_width() - if accessory.is_some() { 96.0 } else { 0.0 };
                ui.vertical(|ui| {
                    ui.set_width(width);
                    for text in texts {
                        rich(row, ui, message, text);
                    }
                });
                if let Some(accessory) = accessory {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        draw(row, ui, message, accessory);
                    });
                }
            });
        }
        Component::Thumbnail(url) => {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(85.0), Sense::hover());
            if let Some(uri) = row.app.picture(Some(url)) {
                egui::Image::new(uri)
                    .corner_radius(CornerRadius::same(8))
                    .paint_at(ui, rect);
            }
        }
        Component::Gallery(urls) => {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                let size = if urls.len() == 1 {
                    vec2(400.0, 300.0)
                } else {
                    Vec2::splat(196.0)
                };
                for url in urls {
                    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
                    match row.app.picture(Some(url)) {
                        Some(uri) => {
                            egui::Image::new(uri)
                                .corner_radius(CornerRadius::same(8))
                                .paint_at(ui, rect);
                        }
                        None => {
                            ui.painter()
                                .rect_filled(rect, CornerRadius::same(8), palette.sidebar);
                        }
                    }
                    if response.clicked() {
                        row.out
                            .push(Action::Chat(crate::chat::ChatAction::ViewImage(
                                url.clone(),
                            )));
                    }
                }
            });
        }
        Component::File(url) => {
            let name = url.rsplit('/').next().unwrap_or(url).to_owned();
            let link = ui.add(
                egui::Label::new(RichText::new(name).color(Color32::from_rgb(0x00, 0xa8, 0xfc)))
                    .sense(Sense::click()),
            );
            if link.clicked() {
                row.out
                    .push(Action::Chat(crate::chat::ChatAction::OpenUrl(url.clone())));
            }
        }
        Component::Separator { divider, large } => {
            let gap = if *large { 16.0 } else { 8.0 };
            ui.add_space(gap);
            if *divider {
                let (r, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter()
                    .hline(r.x_range(), r.center().y, Stroke::new(1.0, palette.divider));
            }
            ui.add_space(gap);
        }
        Component::Container { accent, children } => {
            let top = ui.cursor().top();
            let response = Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius::same(8))
                .stroke(Stroke::new(1.0, palette.divider))
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().min(520.0));
                    for child in children {
                        draw(row, ui, message, child);
                    }
                })
                .response;
            if let Some(accent) = accent {
                ui.painter().rect_filled(
                    Rect::from_min_max(
                        pos2(response.rect.left(), top),
                        pos2(response.rect.left() + 4.0, response.rect.bottom()),
                    ),
                    CornerRadius {
                        nw: 8,
                        sw: 8,
                        ne: 0,
                        se: 0,
                    },
                    *accent,
                );
            }
            ui.add_space(4.0);
        }
    }
}

fn draw_button(row: &mut Row<'_>, ui: &mut Ui, message: &Message, button: &Button) {
    let app = row.app;
    let palette = app.palette;
    let loading = button.custom_id.as_ref().is_some_and(|id| {
        app.interact
            .pending
            .values()
            .any(|(m, c)| *m == message.id && c == id)
    });
    let label = if loading {
        "…".to_owned()
    } else {
        button.label.clone().unwrap_or_default()
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label, font(Weight::Medium, 14.0), Color32::WHITE);
    let emoji = if button.emoji.is_some() { 22.0 } else { 0.0 };
    let link = button.style == 5;
    let width = (galley.size().x + emoji + if link { 22.0 } else { 0.0 } + 32.0).max(60.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let base = match button.style {
        1 => BLURPLE,
        3 => SUCCESS,
        4 => DANGER,
        6 => palette.accent,
        _ => SECONDARY,
    };
    let enabled = !button.disabled && !loading;
    let fill = if !enabled {
        base.gamma_multiply(0.5)
    } else if response.hovered() {
        base.gamma_multiply(0.85)
    } else {
        base
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    let mut x = rect.left() + 16.0;
    if let Some(e) = &button.emoji {
        paint_emoji(
            ui,
            app,
            e,
            Rect::from_center_size(pos2(x + 9.0, rect.center().y), Vec2::splat(18.0)),
        );
        x += emoji;
    }
    let text = if enabled {
        Color32::WHITE
    } else {
        Color32::from_white_alpha(140)
    };
    ui.painter().galley(
        pos2(x, rect.center().y - galley.size().y / 2.0),
        galley.clone(),
        text,
    );
    if link {
        paint_icon(
            ui,
            Icon::External,
            pos2(x + galley.size().x + 12.0, rect.center().y),
            14.0,
            text,
        );
    }
    if response.clicked() && enabled {
        match (&button.url, &button.custom_id) {
            (Some(url), _) => row
                .out
                .push(Action::Chat(crate::chat::ChatAction::OpenUrl(url.clone()))),
            (None, Some(custom_id)) => act(
                row.out,
                InteractAction::Button {
                    channel: row.channel,
                    message: message.id,
                    custom_id: custom_id.clone(),
                },
            ),
            _ => {}
        }
    }
}

/// A bot's menu: the choices of a text menu, or people, roles or channels
/// for the others. One pick sends at once; several wait for "Confirmar".
fn draw_select(row: &mut Row<'_>, ui: &mut Ui, message: &Message, select: &Select) {
    let app = row.app;
    let palette = app.palette;
    let key = egui::Id::new(("select", message.id, &select.custom_id));
    let mut picked: Vec<String> = ui.data(|d| d.get_temp(key)).unwrap_or_else(|| {
        select
            .options
            .iter()
            .filter(|o| o.default)
            .map(|o| o.value.clone())
            .collect()
    });
    // The other menus list what FastDisco knows of the server.
    let options: Vec<(String, String, Option<String>)> = match select.kind {
        3 => select
            .options
            .iter()
            .map(|o| {
                let label = match &o.emoji {
                    Some(ReactionEmoji::Unicode(e)) => format!("{e} {}", o.label),
                    _ => o.label.clone(),
                };
                (label, o.value.clone(), o.description.clone())
            })
            .collect(),
        kind => {
            let guild = row.guild.and_then(|g| app.model.guild(g));
            let mut list = Vec::new();
            if matches!(kind, 5 | 7) {
                let people: Vec<Id> = guild
                    .map(|g| g.members.keys().copied().collect())
                    .unwrap_or_default();
                for user in people.into_iter().take(50) {
                    list.push((app.model.name_in(row.guild, user), user.to_string(), None));
                }
            }
            if matches!(kind, 6 | 7) {
                for role in guild.map(|g| g.roles.clone()).unwrap_or_default() {
                    list.push((format!("@{}", role.name), role.id.to_string(), None));
                }
            }
            if kind == 8 {
                for id in guild.map(|g| g.channels.clone()).unwrap_or_default() {
                    if let Some(c) = app
                        .model
                        .channel(id)
                        .filter(|c| c.kind != ChannelKind::Category)
                    {
                        list.push((format!("#{}", c.name), id.to_string(), None));
                    }
                }
            }
            list
        }
    };
    let shown = if picked.is_empty() {
        select
            .placeholder
            .clone()
            .unwrap_or_else(|| "Faça uma seleção".into())
    } else {
        options
            .iter()
            .filter(|(_, v, _)| picked.contains(v))
            .map(|(l, _, _)| l.clone())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let width = ui.available_width().min(400.0);
    let multiple = select.max > 1;
    let mut send = None;
    let response = egui::ComboBox::from_id_salt(key)
        .width(width - 16.0)
        .selected_text(RichText::new(shown).color(if picked.is_empty() {
            palette.muted
        } else {
            palette.text
        }))
        .show_ui(ui, |ui| {
            ui.set_min_width(width);
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (label, value, description) in &options {
                        let on = picked.contains(value);
                        let text = match description {
                            Some(d) => format!("{label}\n{d}"),
                            None => label.clone(),
                        };
                        if ui.selectable_label(on, text).clicked() {
                            if multiple {
                                if on {
                                    picked.retain(|v| v != value);
                                } else if picked.len() < select.max as usize {
                                    picked.push(value.clone());
                                }
                            } else {
                                picked = vec![value.clone()];
                                send = Some(picked.clone());
                                ui.close();
                            }
                        }
                    }
                });
            if multiple {
                ui.separator();
                let enough = picked.len() >= select.min as usize;
                if ui
                    .add_enabled(enough, egui::Button::new("Confirmar"))
                    .clicked()
                {
                    send = Some(picked.clone());
                    ui.close();
                }
            }
        });
    if select.disabled {
        response.response.on_hover_text("Indisponível");
    }
    ui.data_mut(|d| d.insert_temp(key, picked));
    if let Some(values) = send
        && !select.disabled
    {
        act(
            row.out,
            InteractAction::Select {
                channel: row.channel,
                message: message.id,
                custom_id: select.custom_id.clone(),
                kind: select.kind,
                values,
            },
        );
    }
}

fn dialog_frame(app: &App) -> Frame {
    Frame::new()
        .fill(app.palette.chat)
        .corner_radius(CornerRadius::same(8))
}

fn field_label(ui: &mut Ui, app: &App, text: &str, required: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.label(
            RichText::new(text.to_uppercase())
                .font(font(Weight::Bold, 12.0))
                .color(app.palette.muted),
        );
        if required {
            ui.label(
                RichText::new("*")
                    .font(font(Weight::Bold, 12.0))
                    .color(app.palette.danger),
            );
        }
    });
}

fn input(
    ui: &mut Ui,
    app: &App,
    text: &mut String,
    hint: &str,
    multiline: bool,
    max: usize,
) -> egui::Response {
    let palette = app.palette;
    Frame::new()
        .fill(palette.rail)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            let edit = if multiline {
                egui::TextEdit::multiline(text).desired_rows(4)
            } else {
                egui::TextEdit::singleline(text)
            };
            ui.add(
                edit.frame(Frame::NONE)
                    .hint_text(RichText::new(hint).color(palette.muted))
                    .font(font(Weight::Regular, 15.0))
                    .text_color(palette.text)
                    .char_limit(max)
                    .desired_width(f32::INFINITY),
            )
        })
        .inner
}

/// The bottom bar of a dialog: "Cancelar" and the main button.
fn dialog_buttons(ui: &mut Ui, app: &App, label: &str, enabled: bool) -> (bool, bool) {
    let palette = app.palette;
    let mut main = false;
    let mut cancel = false;
    Frame::new()
        .fill(palette.sidebar)
        .corner_radius(CornerRadius {
            nw: 0,
            ne: 0,
            sw: 8,
            se: 8,
        })
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                let button = egui::Button::new(
                    RichText::new(label)
                        .font(font(Weight::Medium, 14.0))
                        .color(Color32::WHITE),
                )
                .fill(palette.accent)
                .corner_radius(CornerRadius::same(4))
                .min_size(vec2(96.0, 38.0));
                main = ui.add_enabled(enabled, button).clicked();
                let back = egui::Button::new(
                    RichText::new("Cancelar")
                        .font(font(Weight::Medium, 14.0))
                        .color(palette.strong),
                )
                .frame(false)
                .min_size(vec2(80.0, 38.0));
                cancel = ui.add(back).clicked();
            });
        });
    (main, cancel)
}

/// A form a bot opened.
pub fn modal(app: &mut App, ui: &mut Ui) {
    let Some(modal) = app.interact.modal.clone() else {
        return;
    };
    let palette = app.palette;
    let app_name = app
        .interact
        .commands
        .values()
        .find_map(|i| i.apps.get(&modal.application).cloned());
    let mut fields = modal.fields.clone();
    let mut submit = false;
    let mut close = false;
    let response = egui::Modal::new(egui::Id::new(("bot-modal", &modal.id)))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(dialog_frame(app))
        .show(ui.ctx(), |ui| {
            ui.set_width(460.0);
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
                if let Some(name) = &app_name {
                    ui.label(
                        RichText::new(name)
                            .font(font(Weight::Medium, 13.0))
                            .color(palette.muted),
                    );
                }
                ui.label(
                    RichText::new(&modal.title)
                        .font(font(Weight::SemiBold, 20.0))
                        .color(palette.strong),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(ui.ctx().content_rect().height() * 0.6)
                    .show(ui, |ui| {
                        for field in &mut fields {
                            field_label(ui, app, &field.label, field.required);
                            if let Some(d) = &field.description {
                                ui.label(
                                    RichText::new(d)
                                        .font(font(Weight::Regular, 13.0))
                                        .color(palette.muted),
                                );
                            }
                            input(
                                ui,
                                app,
                                &mut field.value,
                                field.placeholder.as_deref().unwrap_or(""),
                                field.style == 2,
                                field.max,
                            );
                            ui.add_space(8.0);
                        }
                    });
                if let Some(error) = &modal.error {
                    ui.label(RichText::new(error).color(palette.danger));
                }
            });
            let (main, cancel) = dialog_buttons(ui, app, "Enviar", true);
            submit = main;
            close = cancel;
        });
    if let Some(m) = &mut app.interact.modal {
        m.fields = fields;
    }
    if submit {
        app.push_interact(InteractAction::SubmitModal);
    } else if close || response.should_close() {
        app.push_interact(InteractAction::CloseModal);
    }
}

/// "Encaminhar para": up to five places, and an optional note.
pub fn forward_dialog(app: &mut App, ui: &mut Ui) {
    let Some(state) = app.interact.forward.clone() else {
        return;
    };
    let palette = app.palette;
    let query = state.query.to_lowercase();
    // Recent conversations first, then the servers' text channels.
    let mut places: Vec<(Id, String, Option<String>)> = Vec::new();
    for id in app.model.sorted_direct() {
        if let Some(c) = app.model.channel(id) {
            places.push((id, app.model.direct_name(c), None));
        }
    }
    for guild in &app.model.guilds {
        for id in &guild.channels {
            if let Some(c) = app.model.channel(*id)
                && c.kind == ChannelKind::Text
                && app.model.can_view(c)
            {
                places.push((*id, format!("#{}", c.name), Some(guild.name.clone())));
            }
        }
    }
    places.retain(|(_, name, server)| {
        query.is_empty()
            || name.to_lowercase().contains(&query)
            || server
                .as_ref()
                .is_some_and(|s| s.to_lowercase().contains(&query))
    });
    places.truncate(60);
    let mut search = state.query.clone();
    let mut note = state.note.clone();
    let mut toggled = None;
    let mut send = false;
    let mut close = false;
    let response = egui::Modal::new(egui::Id::new("forward"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(dialog_frame(app))
        .show(ui.ctx(), |ui| {
            ui.set_width(480.0);
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
                ui.label(
                    RichText::new("Encaminhar para")
                        .font(font(Weight::SemiBold, 20.0))
                        .color(palette.strong),
                );
                ui.label(
                    RichText::new("Selecione onde você quer compartilhar esta mensagem.")
                        .font(font(Weight::Regular, 14.0))
                        .color(palette.muted),
                );
                input(ui, app, &mut search, "Buscar", false, 100).request_focus();
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        for (id, name, server) in &places {
                            let (rect, row) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 44.0),
                                Sense::click(),
                            );
                            if row.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    CornerRadius::same(4),
                                    palette.hover,
                                );
                            }
                            let icon = pos2(rect.left() + 20.0, rect.center().y);
                            let channel = app.model.channel(*id);
                            match channel.map(|c| c.kind) {
                                Some(ChannelKind::Direct) => {
                                    if let Some(user) = channel
                                        .and_then(|c| c.recipients.first())
                                        .and_then(|u| app.model.user(*u))
                                    {
                                        avatar(ui, app, user, icon, 14.0, None);
                                    }
                                }
                                Some(ChannelKind::Group) => {
                                    paint_icon(ui, Icon::Users, icon, 20.0, palette.muted)
                                }
                                _ => paint_icon(ui, Icon::Hash, icon, 20.0, palette.muted),
                            }
                            let text_right = rect.right() - 40.0;
                            Elide::new(ui, text_right).text(
                                pos2(
                                    rect.left() + 44.0,
                                    rect.center().y - if server.is_some() { 8.0 } else { 0.0 },
                                ),
                                Align2::LEFT_CENTER,
                                name.clone(),
                                font(Weight::Medium, 15.0),
                                palette.strong,
                            );
                            if let Some(server) = server {
                                Elide::new(ui, text_right).text(
                                    pos2(rect.left() + 44.0, rect.center().y + 9.0),
                                    Align2::LEFT_CENTER,
                                    server.clone(),
                                    font(Weight::Regular, 12.0),
                                    palette.muted,
                                );
                            }
                            let on = state.chosen.contains(id);
                            let check = Rect::from_center_size(
                                pos2(rect.right() - 20.0, rect.center().y),
                                Vec2::splat(20.0),
                            );
                            if on {
                                ui.painter().rect_filled(
                                    check,
                                    CornerRadius::same(4),
                                    palette.accent,
                                );
                                paint_icon(ui, Icon::Check, check.center(), 14.0, Color32::WHITE);
                            } else {
                                ui.painter().rect_stroke(
                                    check,
                                    CornerRadius::same(4),
                                    Stroke::new(2.0, palette.muted),
                                    egui::StrokeKind::Inside,
                                );
                            }
                            if row.clicked() {
                                toggled = Some(*id);
                            }
                        }
                    });
                input(
                    ui,
                    app,
                    &mut note,
                    "Adicionar uma mensagem opcional...",
                    false,
                    2000,
                );
            });
            let label = if state.chosen.len() > 1 {
                format!("Enviar ({})", state.chosen.len())
            } else {
                "Enviar".into()
            };
            let (main, cancel) = dialog_buttons(ui, app, &label, !state.chosen.is_empty());
            send = main;
            close = cancel;
        });
    if let Some(f) = &mut app.interact.forward {
        f.query = search;
        f.note = note;
    }
    if let Some(id) = toggled {
        app.push_interact(InteractAction::ToggleForwardTarget(id));
    }
    if send {
        app.push_interact(InteractAction::SendForward);
    } else if close || response.should_close() {
        app.push_interact(InteractAction::CloseForward);
    }
}

/// "Criar uma enquete": the question, up to ten answers, how long it runs
/// and whether one may pick several.
pub fn poll_dialog(app: &mut App, ui: &mut Ui) {
    let Some(mut draft) = app.interact.poll.clone() else {
        return;
    };
    let palette = app.palette;
    let mut send = false;
    let mut close = false;
    let response = egui::Modal::new(egui::Id::new("new-poll"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(dialog_frame(app))
        .show(ui.ctx(), |ui| {
            ui.set_width(480.0);
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
                ui.label(
                    RichText::new("Criar uma enquete")
                        .font(font(Weight::SemiBold, 20.0))
                        .color(palette.strong),
                );
                field_label(ui, app, "Pergunta", false);
                input(
                    ui,
                    app,
                    &mut draft.question,
                    "Que pergunta você quer fazer?",
                    false,
                    300,
                );
                field_label(ui, app, "Respostas", false);
                let mut remove = None;
                let count = draft.answers.len();
                for (i, answer) in draft.answers.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width() - if count > 2 { 36.0 } else { 0.0 });
                            input(ui, app, answer, "Digite sua resposta", false, 55);
                        });
                        if count > 2 {
                            let (r, b) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
                            paint_icon(
                                ui,
                                Icon::Trash,
                                r.center(),
                                18.0,
                                if b.hovered() {
                                    palette.danger
                                } else {
                                    palette.icon
                                },
                            );
                            if b.clicked() {
                                remove = Some(i);
                            }
                        }
                    });
                }
                if let Some(i) = remove {
                    draft.answers.remove(i);
                }
                if draft.answers.len() < 10
                    && ui
                        .add(
                            egui::Button::new(
                                RichText::new("+ Adicionar outra resposta").color(palette.text),
                            )
                            .frame(false),
                        )
                        .clicked()
                {
                    draft.answers.push(String::new());
                }
                field_label(ui, app, "Duração", false);
                let lengths = [
                    (1, "1 hora"),
                    (4, "4 horas"),
                    (8, "8 horas"),
                    (24, "24 horas"),
                    (72, "3 dias"),
                    (168, "1 semana"),
                    (336, "2 semanas"),
                ];
                let current = lengths
                    .iter()
                    .find(|(h, _)| *h == draft.hours)
                    .map_or("24 horas", |(_, l)| l);
                egui::ComboBox::from_id_salt("poll-length")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (hours, label) in lengths {
                            ui.selectable_value(&mut draft.hours, hours, label);
                        }
                    });
                ui.checkbox(&mut draft.multiple, "Permitir várias respostas");
                if let Some(error) = &draft.error {
                    ui.label(RichText::new(error).color(palette.danger));
                }
            });
            let (main, cancel) = dialog_buttons(ui, app, "Publicar", true);
            send = main;
            close = cancel;
        });
    app.interact.poll = Some(draft);
    if send {
        app.push_interact(InteractAction::SendPoll);
    } else if close || response.should_close() {
        app.push_interact(InteractAction::ClosePollDraft);
    }
}

/// The composer while a slash command is filled in: the command, a field
/// per option, Enter to run it and Escape to drop it.
pub fn command_bar(app: &mut App, ui: &mut Ui) {
    let Some(active) = app.interact.active.clone() else {
        return;
    };
    let palette = app.palette;
    let Some(index) = app.interact.commands.get(&active.scope) else {
        return;
    };
    let Some(entry) = index.entries.get(active.entry).cloned() else {
        return;
    };
    let Some(command) = index.commands.get(entry.command).cloned() else {
        return;
    };
    let app_name = index.apps.get(&command.application).cloned();
    let mut values = active.values.clone();
    let (run, cancel) = ui.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape),
        )
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let name = std::iter::once(command.name.as_str())
            .chain(entry.path.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        Frame::new()
            .fill(palette.accent.gamma_multiply(0.3))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("/{name}"))
                        .font(font(Weight::SemiBold, 15.0))
                        .color(palette.strong),
                );
            });
        if let Some(app_name) = app_name {
            ui.label(
                RichText::new(app_name)
                    .font(font(Weight::Medium, 13.0))
                    .color(palette.muted),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (r, close) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
            paint_icon(
                ui,
                Icon::Close,
                r.center(),
                18.0,
                if close.hovered() {
                    palette.strong
                } else {
                    palette.icon
                },
            );
            if close.on_hover_text("Cancelar (Esc)").clicked() {
                app.actions
                    .push(Action::Interact(InteractAction::CancelCommand));
            }
        });
    });
    if !entry.description.is_empty() {
        ui.label(
            RichText::new(&entry.description)
                .font(font(Weight::Regular, 13.0))
                .color(palette.muted),
        );
    }
    ui.add_space(6.0);
    let mut first = true;
    for (option, value) in entry.options.iter().zip(values.iter_mut()) {
        ui.horizontal(|ui| {
            ui.set_min_height(30.0);
            let name = format!("{}{}", option.name, if option.required { "*" } else { "" });
            ui.add_sized(
                vec2(120.0, 24.0),
                egui::Label::new(
                    RichText::new(name)
                        .font(font(Weight::SemiBold, 14.0))
                        .color(palette.text),
                )
                .truncate(),
            )
            .on_hover_text(&option.description);
            if !option.choices.is_empty() {
                egui::ComboBox::from_id_salt(("command-choice", &option.name))
                    .width(ui.available_width() - 16.0)
                    .selected_text(if value.is_empty() {
                        option.description.clone()
                    } else {
                        value.clone()
                    })
                    .show_ui(ui, |ui| {
                        for (choice, _) in &option.choices {
                            ui.selectable_value(value, choice.clone(), choice);
                        }
                    });
            } else if option.kind == 5 {
                let mut on = value == "true";
                if ui.checkbox(&mut on, &option.description).changed() {
                    *value = if on { "true".into() } else { "false".into() };
                }
            } else {
                let hint = match option.kind {
                    6 => "@pessoa ou ID",
                    7 => "#canal ou ID",
                    8 => "@cargo ou ID",
                    9 => "@pessoa, @cargo ou ID",
                    4 => "número inteiro",
                    10 => "número",
                    11 => "anexos ainda não são suportados",
                    _ => option.description.as_str(),
                };
                let edit = ui.add(
                    egui::TextEdit::singleline(value)
                        .hint_text(RichText::new(hint).color(palette.muted))
                        .font(font(Weight::Regular, 15.0))
                        .desired_width(f32::INFINITY),
                );
                if first && std::mem::take(&mut app.chat.focus_composer) {
                    edit.request_focus();
                }
                first = false;
            }
        });
    }
    if entry.options.is_empty() {
        ui.label(
            RichText::new("Pressione Enter para usar o comando.")
                .font(font(Weight::Regular, 13.0))
                .color(palette.muted),
        );
    }
    if let Some(error) = &active.error {
        ui.label(RichText::new(error).color(palette.danger));
    }
    ui.add_space(10.0);
    if let Some(a) = &mut app.interact.active {
        a.values = values;
    }
    if cancel {
        app.actions
            .push(Action::Interact(InteractAction::CancelCommand));
    } else if run {
        app.actions
            .push(Action::Interact(InteractAction::RunCommand));
    }
}
