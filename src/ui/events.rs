//! The "Eventos" dialog: each event with its picture, when it starts,
//! where it happens and who is interested; and the form for a new one.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, pos2, vec2};
use fastframe_fonts::Weight;

use super::widgets::{avatar, font, paint_icon, timestamp};
use crate::icons::Icon;
use crate::app::{Action, App};
use crate::events::{Draft, EventAction};
use crate::model::permission;

fn act(action: EventAction) -> Action {
    Action::Events(action)
}

pub fn dialog(app: &mut App, ui: &mut Ui) {
    let Some(guild) = app.events.guild else { return };
    if let Some(event) = app.events.detail {
        details(app, ui, guild, event);
        return;
    }
    let palette = app.palette;
    let manage = app.model.can(guild, permission::MANAGE_EVENTS);
    let mut out = Vec::new();
    let response = egui::Modal::new(egui::Id::new("events-dialog"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(Frame::new().fill(palette.chat).corner_radius(CornerRadius::same(8)))
        .show(ui.ctx(), |ui| {
            ui.set_width(560.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    let count = app.events.list.as_ref().map_or(0, Vec::len);
                    let title = match count {
                        0 => "Eventos".to_owned(),
                        1 => "1 Evento".to_owned(),
                        n => format!("{n} Eventos"),
                    };
                    ui.label(
                        RichText::new(title)
                            .font(font(Weight::SemiBold, 20.0))
                            .color(palette.strong),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("✕").color(palette.muted)).frame(false))
                            .clicked()
                        {
                            out.push(act(EventAction::Close));
                        }
                        if manage && app.events.draft.is_none() {
                            let create = egui::Button::new(
                                RichText::new("Criar evento")
                                    .font(font(Weight::Medium, 14.0))
                                    .color(Color32::WHITE),
                            )
                            .fill(palette.accent)
                            .corner_radius(CornerRadius::same(4));
                            if ui.add(create).clicked() {
                                out.push(act(EventAction::StartDraft));
                            }
                        }
                    });
                });
                ui.separator();
                if let Some(draft) = app.events.draft.clone() {
                    form(app, ui, guild, draft, &mut out);
                    return;
                }
                if let Some(error) = &app.events.error {
                    ui.label(RichText::new(error).color(palette.danger));
                }
                egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                    match app.events.list.clone() {
                        None => {
                            ui.spinner();
                        }
                        Some(list) if list.is_empty() => {
                            ui.add_space(24.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("Nenhum evento futuro.")
                                        .font(font(Weight::SemiBold, 16.0))
                                        .color(palette.strong),
                                );
                                ui.label(
                                    RichText::new("Programe um evento para qualquer atividade planejada no seu servidor.")
                                        .color(palette.muted),
                                );
                            });
                            ui.add_space(24.0);
                        }
                        Some(list) => {
                            for item in list {
                                card(app, ui, guild, manage, &item, &mut out);
                            }
                        }
                    }
                });
            });
        });
    if response.should_close() {
        out.push(act(EventAction::Close));
    }
    app.actions.extend(out);
}

fn card(
    app: &App,
    ui: &mut Ui,
    guild: crate::model::Id,
    manage: bool,
    item: &crate::events::Item,
    out: &mut Vec<Action>,
) {
    let palette = app.palette;
    Frame::new()
        .fill(palette.sidebar)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            if let Some(uri) = item.image.as_deref().and_then(|i| app.picture(Some(i))) {
                let width = ui.available_width();
                ui.add(
                    egui::Image::new(uri)
                        .fit_to_exact_size(vec2(width, width * 0.4))
                        .corner_radius(CornerRadius::same(6)),
                );
            }
            let when = if item.status == 2 {
                RichText::new("● Acontecendo agora").color(palette.online)
            } else {
                RichText::new(item.start.map(timestamp).unwrap_or_default()).color(palette.muted)
            };
            ui.label(when.font(font(Weight::SemiBold, 13.0)));
            // The name opens the event's details, as a click on Discord's card.
            let title = ui
                .add(
                    egui::Label::new(
                        RichText::new(&item.name)
                            .font(font(Weight::Bold, 17.0))
                            .color(palette.strong),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Detalhes do evento");
            if title.clicked() {
                out.push(act(EventAction::Details { guild, event: item.id }));
            }
            if let Some(description) = &item.description {
                ui.label(RichText::new(description).color(palette.text));
            }
            let place = match (item.channel.and_then(|c| app.model.channel(c)), &item.location) {
                (Some(channel), _) => format!("🔊 {}", channel.name),
                (None, Some(location)) => format!("📍 {location}"),
                (None, None) => String::new(),
            };
            if !place.is_empty() {
                ui.label(RichText::new(place).color(palette.muted));
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} interessados", item.interested))
                        .font(font(Weight::Medium, 13.0))
                        .color(palette.muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (label, fill, text) = if item.me {
                        ("✓ Interessado", palette.selected, palette.text)
                    } else {
                        ("Tenho interesse", palette.accent, Color32::WHITE)
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(label).color(text))
                                .fill(fill)
                                .corner_radius(CornerRadius::same(4)),
                        )
                        .clicked()
                    {
                        out.push(act(EventAction::Interest(item.id, !item.me)));
                    }
                    let mine = item.creator == Some(app.model.me);
                    if manage || mine {
                        if item.status == 1 {
                            if ui.button("Iniciar").clicked() {
                                out.push(act(EventAction::SetStatus(item.id, 2)));
                            }
                            if ui.button("Cancelar evento").clicked() {
                                out.push(act(EventAction::SetStatus(item.id, 4)));
                            }
                        } else if ui.button("Encerrar evento").clicked() {
                            out.push(act(EventAction::SetStatus(item.id, 3)));
                        }
                    }
                    if ui.button("Copiar link").clicked() {
                        out.push(Action::Chat(crate::chat::ChatAction::Copy(format!(
                            "https://discord.com/events/{guild}/{}",
                            item.id
                        ))));
                    }
                });
            });
        });
    ui.add_space(4.0);
}

/// "Criar evento": where, what and when.
fn form(app: &mut App, ui: &mut Ui, guild: crate::model::Id, draft: Draft, out: &mut Vec<Action>) {
    let palette = app.palette;
    let mut edited = draft.clone();
    let channels: Vec<(crate::model::Id, String)> = app
        .model
        .guild(guild)
        .map(|g| {
            g.channels
                .iter()
                .filter_map(|c| app.model.channel(*c))
                .filter(|c| c.kind.is_voice() && app.model.can_view(c))
                .map(|c| (c.id, c.name.clone()))
                .collect()
        })
        .unwrap_or_default();
    let label = |ui: &mut Ui, text: &str| {
        ui.label(RichText::new(text).font(font(Weight::Bold, 12.0)).color(palette.muted));
    };
    label(ui, "ONDE É O SEU EVENTO?");
    let shown = edited
        .channel
        .and_then(|c| channels.iter().find(|(id, _)| *id == c))
        .map_or_else(|| "Outro lugar".to_owned(), |(_, n)| format!("🔊 {n}"));
    egui::ComboBox::from_id_salt("event-place")
        .selected_text(shown)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for (id, name) in &channels {
                ui.selectable_value(&mut edited.channel, Some(*id), format!("🔊 {name}"));
            }
            ui.selectable_value(&mut edited.channel, None, "Outro lugar (um link ou endereço)");
        });
    let field = |ui: &mut Ui, text: &mut String, hint: &str, multiline: bool| {
        Frame::new()
            .fill(palette.rail)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                let edit = if multiline {
                    egui::TextEdit::multiline(text).desired_rows(3)
                } else {
                    egui::TextEdit::singleline(text)
                };
                ui.add(
                    edit.frame(Frame::NONE)
                        .hint_text(RichText::new(hint).color(palette.muted))
                        .font(font(Weight::Regular, 15.0))
                        .text_color(palette.text)
                        .desired_width(f32::INFINITY),
                );
            });
    };
    if edited.channel.is_none() {
        label(ui, "LOCAL");
        field(ui, &mut edited.location, "Adicione um local, link ou algo do tipo.", false);
    }
    label(ui, "TEMA DO EVENTO");
    field(ui, &mut edited.name, "Do que se trata o seu evento?", false);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
        ui.vertical(|ui| {
            ui.set_width(180.0);
            label(ui, "DATA DE INÍCIO");
            field(ui, &mut edited.date, "28/10/2026", false);
        });
        ui.vertical(|ui| {
            ui.set_width(120.0);
            label(ui, "HORA DE INÍCIO");
            field(ui, &mut edited.time, "20:30", false);
        });
        if edited.channel.is_none() {
            ui.vertical(|ui| {
                ui.set_width(120.0);
                label(ui, "DURAÇÃO (HORAS)");
                ui.add(egui::DragValue::new(&mut edited.hours).range(1..=72));
            });
        }
    });
    label(ui, "DESCRIÇÃO");
    field(ui, &mut edited.description, "Diga a todos um pouco mais sobre o seu evento.", true);
    if let Some(error) = &edited.error {
        ui.label(RichText::new(error).color(palette.danger));
    }
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let create = egui::Button::new(
                RichText::new(if edited.busy { "Criando…" } else { "Criar evento" })
                    .font(font(Weight::Medium, 14.0))
                    .color(Color32::WHITE),
            )
            .fill(palette.accent)
            .corner_radius(CornerRadius::same(4))
            .min_size(vec2(120.0, 36.0));
            if ui.add_enabled(!edited.busy, create).clicked() {
                out.push(act(EventAction::SubmitDraft));
            }
            if ui
                .add(egui::Button::new(RichText::new("Voltar").color(palette.text)).frame(false))
                .clicked()
            {
                out.push(act(EventAction::CancelDraft));
            }
        });
    });
    if let Some(d) = &mut app.events.draft {
        d.channel = edited.channel;
        d.location = edited.location;
        d.name = edited.name;
        d.description = edited.description;
        d.date = edited.date;
        d.time = edited.time;
        d.hours = edited.hours;
    }
}

/// One event as Discord's "Detalhes do evento": its time, place, who made
/// it and how many are interested, the coming dates of a series, and a tab
/// with the interested people.
fn details(app: &mut App, ui: &mut Ui, guild: crate::model::Id, event: crate::model::Id) {
    let palette = app.palette;
    let manage = app.model.can(guild, permission::MANAGE_EVENTS);
    let item = app
        .events
        .list
        .as_ref()
        .and_then(|l| l.iter().find(|i| i.id == event))
        .cloned();
    let people = app.events.people.get(&event).cloned();
    let mut out = Vec::new();
    let response = egui::Modal::new(egui::Id::new("event-details"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(Frame::new().fill(palette.chat).corner_radius(CornerRadius::same(12)))
        .show(ui.ctx(), |ui| {
            ui.set_width(560.0);
            Frame::new().inner_margin(Margin::same(24)).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 10.0);
                let Some(item) = item else {
                    ui.vertical_centered(|ui| ui.spinner());
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&item.name)
                            .font(font(Weight::Bold, 20.0))
                            .color(palette.strong),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, close) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                        let tint = if close.hovered() { palette.strong } else { palette.muted };
                        paint_icon(ui, Icon::Close, rect.center(), 22.0, tint);
                        if close.clicked() {
                            out.push(act(EventAction::Close));
                        }
                    });
                });
                // The two tabs, underlined as Discord's.
                let count = people.as_ref().map_or(item.interested as usize, Vec::len);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 32.0;
                    for (on, label) in [
                        (false, "Informações do evento".to_owned()),
                        (true, format!("{count} Têm interesse")),
                    ] {
                        let lit = app.events.people_tab == on;
                        let response = ui.add(
                            egui::Label::new(
                                RichText::new(label)
                                    .font(font(Weight::Medium, 16.0))
                                    .color(if lit { palette.strong } else { palette.muted }),
                            )
                            .sense(Sense::click()),
                        );
                        if lit {
                            let r = response.rect;
                            ui.painter().line_segment(
                                [pos2(r.left(), r.bottom() + 8.0), pos2(r.right(), r.bottom() + 8.0)],
                                Stroke::new(2.0, palette.accent),
                            );
                        }
                        if response.clicked() {
                            out.push(act(EventAction::PeopleTab(on)));
                        }
                    }
                });
                ui.add_space(4.0);
                ui.separator();
                egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if app.events.people_tab {
                        people_tab(app, ui, guild, people.as_deref());
                    } else {
                        info_tab(app, ui, guild, &item, count, &mut out);
                    }
                });
                ui.add_space(8.0);
                // The foot: the menu and "Copiar link", and interest for
                // what has not started.
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let button = |text: &str, fill: Color32, color: Color32| {
                            egui::Button::new(RichText::new(text).font(font(Weight::Medium, 14.0)).color(color))
                                .fill(fill)
                                .corner_radius(CornerRadius::same(8))
                                .min_size(vec2(0.0, 38.0))
                        };
                        let link = format!("https://discord.com/events/{guild}/{}", item.id);
                        if ui.add(button("🔗 Copiar link", palette.selected, palette.strong)).clicked() {
                            out.push(Action::Chat(crate::chat::ChatAction::Copy(link)));
                        }
                        if item.status == 1 {
                            let (label, fill, color) = if item.me {
                                ("✓ Interessado", palette.selected, palette.strong)
                            } else {
                                ("Tenho interesse", palette.accent, Color32::WHITE)
                            };
                            if ui.add(button(label, fill, color)).clicked() {
                                out.push(act(EventAction::Interest(item.id, !item.me)));
                            }
                        }
                        let more = ui.add(button("···", palette.selected, palette.strong));
                        egui::Popup::menu(&more).show(|ui| {
                            ui.set_min_width(200.0);
                            let mine = item.creator == Some(app.model.me);
                            if manage || mine {
                                if item.status == 1 {
                                    if ui.button("Iniciar evento").clicked() {
                                        out.push(act(EventAction::SetStatus(item.id, 2)));
                                    }
                                    if ui.button("Cancelar evento").clicked() {
                                        out.push(act(EventAction::SetStatus(item.id, 4)));
                                    }
                                } else if ui.button("Encerrar evento").clicked() {
                                    out.push(act(EventAction::SetStatus(item.id, 3)));
                                }
                            }
                            if ui.button("Copiar ID do evento").clicked() {
                                out.push(Action::Chat(crate::chat::ChatAction::Copy(item.id.to_string())));
                            }
                        });
                    });
                });
            });
        });
    if response.should_close() {
        out.push(act(EventAction::Close));
    }
    app.actions.extend(out);
}

/// A line of the details: an icon on the left and what follows it.
fn line(ui: &mut Ui, icon: Icon, tint: Color32, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
        paint_icon(ui, icon, rect.center(), 20.0, tint);
        add(ui);
    });
}

fn info_tab(
    app: &App,
    ui: &mut Ui,
    guild: crate::model::Id,
    item: &crate::events::Item,
    interested: usize,
    out: &mut Vec<Action>,
) {
    let palette = app.palette;
    let now = jiff::Timestamp::now();
    ui.add_space(4.0);
    if item.status == 2 {
        line(ui, Icon::Calendar, palette.online, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(RichText::new("Acontecendo agora").font(font(Weight::SemiBold, 14.0)).color(palette.online));
            if let Some(end) = item.end {
                ui.label(
                    RichText::new(format!(" — Termina em {}", crate::events::when(end)))
                        .font(font(Weight::SemiBold, 14.0))
                        .color(palette.strong),
                );
            }
        });
    } else if let Some(start) = item.start {
        line(ui, Icon::Calendar, palette.muted, |ui| {
            ui.label(
                RichText::new(crate::events::when(start))
                    .font(font(Weight::SemiBold, 14.0))
                    .color(palette.text),
            );
        });
    }
    ui.label(RichText::new(&item.name).font(font(Weight::Bold, 18.0)).color(palette.strong));
    if let Some(description) = &item.description {
        ui.label(RichText::new(description).color(palette.text));
    }
    if let Some(g) = app.model.guild(guild) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
            super::rail::paint_guild(ui, app, g, rect, false);
            ui.label(RichText::new(&g.name).color(palette.text));
        });
    }
    match (item.channel.and_then(|c| app.model.channel(c)), &item.location) {
        (Some(channel), _) => {
            let icon = if channel.kind == crate::model::ChannelKind::Stage { Icon::Stage } else { Icon::Speaker };
            line(ui, icon, palette.muted, |ui| {
                ui.label(RichText::new(&channel.name).color(palette.text));
            });
        }
        (None, Some(location)) => {
            line(ui, Icon::MapPin, palette.muted, |ui| {
                if location.starts_with("http://") || location.starts_with("https://") {
                    let link = ui.add(
                        egui::Label::new(RichText::new(location).color(Color32::from_rgb(0x00, 0xa8, 0xfc)))
                            .sense(Sense::click()),
                    );
                    if link.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        out.push(Action::Chat(crate::chat::ChatAction::OpenUrl(location.clone())));
                    }
                } else {
                    ui.label(RichText::new(location).color(palette.text));
                }
            });
        }
        (None, None) => {}
    }
    line(ui, Icon::Users, palette.muted, |ui| {
        let text = if interested == 1 {
            "1 pessoa está interessada".to_owned()
        } else {
            format!("{interested} pessoas estão interessadas")
        };
        ui.label(RichText::new(text).color(palette.text));
    });
    if let Some(user) = item.creator.and_then(|c| app.model.user(c)) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
            avatar(ui, app, user, rect.center(), 10.0, None);
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.label(RichText::new("Criada por:").color(palette.text));
            let color = app.model.name_color(Some(guild), user.id).unwrap_or(palette.strong);
            ui.label(RichText::new(app.model.name_in(Some(guild), user.id)).color(color));
        });
    }
    // A series: its next dates after the one shown, more on request.
    if let Some(rule) = &item.recurrence {
        let after = item.start.filter(|s| *s > now).unwrap_or(now).max(now);
        let limit = if app.events.more_dates { 12 } else { 4 };
        let dates = rule.next(after, limit);
        if !dates.is_empty() {
            ui.add_space(4.0);
            ui.separator();
            Frame::new().inner_margin(Margin::symmetric(16, 4)).show(ui, |ui| {
                ui.label(
                    RichText::new("Eventos em série")
                        .font(font(Weight::SemiBold, 14.0))
                        .color(palette.strong),
                );
                for date in &dates {
                    line(ui, Icon::Calendar, palette.accent, |ui| {
                        ui.label(
                            RichText::new(crate::events::when(*date))
                                .font(font(Weight::SemiBold, 14.0))
                                .color(palette.strong),
                        );
                    });
                }
                if !app.events.more_dates {
                    ui.vertical_centered(|ui| {
                        let more = ui.add(
                            egui::Label::new(
                                RichText::new("Visualizar eventos futuros")
                                    .font(font(Weight::Medium, 15.0))
                                    .color(palette.accent),
                            )
                            .sense(Sense::click()),
                        );
                        if more.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            out.push(act(EventAction::MoreDates));
                        }
                    });
                }
            });
        }
    }
}

fn people_tab(app: &App, ui: &mut Ui, guild: crate::model::Id, people: Option<&[crate::model::Id]>) {
    let palette = app.palette;
    let Some(people) = people else {
        ui.vertical_centered(|ui| ui.spinner());
        return;
    };
    if people.is_empty() {
        ui.label(RichText::new("Ninguém marcou interesse ainda.").color(palette.muted));
        return;
    }
    for id in people {
        let Some(user) = app.model.user(*id) else { continue };
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
        avatar(ui, app, user, pos2(rect.left() + 20.0, rect.center().y), 16.0, None);
        let color = app.model.name_color(Some(guild), user.id).unwrap_or(palette.strong);
        ui.painter().text(
            pos2(rect.left() + 46.0, rect.center().y - 8.0),
            Align2::LEFT_CENTER,
            app.model.name_in(Some(guild), user.id),
            font(Weight::SemiBold, 15.0),
            color,
        );
        ui.painter().text(
            pos2(rect.left() + 46.0, rect.center().y + 10.0),
            Align2::LEFT_CENTER,
            &user.username,
            font(Weight::Regular, 13.0),
            palette.muted,
        );
    }
}
