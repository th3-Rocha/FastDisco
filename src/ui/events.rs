//! The "Eventos" dialog: each event with its picture, when it starts,
//! where it happens and who is interested; and the form for a new one.

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Ui, vec2};
use fastframe_fonts::Weight;

use super::widgets::{font, timestamp};
use crate::app::{Action, App};
use crate::events::{Draft, EventAction};
use crate::model::permission;

fn act(action: EventAction) -> Action {
    Action::Events(action)
}

pub fn dialog(app: &mut App, ui: &mut Ui) {
    let Some(guild) = app.events.guild else { return };
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
            ui.label(
                RichText::new(&item.name)
                    .font(font(Weight::Bold, 17.0))
                    .color(palette.strong),
            );
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
