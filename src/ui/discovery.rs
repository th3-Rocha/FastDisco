//! "Explorar": public servers by category as cards, and joining one;
//! then a new server's rules to agree to and its onboarding questions.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{Elide, font, paint_icon};
use crate::app::{Action, App};
use crate::discovery::DiscoveryAction;
use crate::icons::Icon;

fn act(action: DiscoveryAction) -> Action {
    Action::Discovery(action)
}

/// The category list where the channels would be.
pub fn sidebar(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let mut out = Vec::new();
    egui::Panel::left("discovery-sidebar")
        .resizable(false)
        .exact_size(super::sidebar::width(app))
        .show_separator_line(false)
        .frame(Frame::new().fill(palette.sidebar).inner_margin(Margin::symmetric(8, 16)))
        .show(ui, |ui| {
            ui.label(
                RichText::new("Explorar")
                    .font(font(Weight::Bold, 20.0))
                    .color(palette.strong),
            );
            ui.add_space(12.0);
            let mut entries: Vec<(Option<u32>, String)> = vec![(None, "Página inicial".into())];
            entries.extend(app.discovery.categories.iter().map(|(id, name)| (Some(*id), name.clone())));
            for (category, name) in entries {
                let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
                let selected = app.discovery.category == category;
                if selected {
                    ui.painter().rect_filled(rect, CornerRadius::same(4), palette.accent);
                } else if response.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(4), palette.hover);
                }
                paint_icon(
                    ui,
                    if category.is_none() { Icon::Compass } else { Icon::Hash },
                    pos2(rect.left() + 18.0, rect.center().y),
                    18.0,
                    if selected { Color32::WHITE } else { palette.icon },
                );
                ui.painter().text(
                    pos2(rect.left() + 36.0, rect.center().y),
                    Align2::LEFT_CENTER,
                    name,
                    font(Weight::Medium, 15.0),
                    if selected { Color32::WHITE } else { palette.text },
                );
                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    out.push(act(DiscoveryAction::Category(category)));
                }
            }
        });
    app.actions.extend(out);
}

/// The cards of public servers.
pub fn page(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let mut out = Vec::new();
    egui::CentralPanel::default()
        .frame(Frame::new().fill(palette.chat).inner_margin(Margin::same(24)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Encontre sua comunidade no Discord")
                        .font(font(Weight::Bold, 22.0))
                        .color(palette.strong),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("✕").color(palette.muted)).frame(false))
                        .clicked()
                    {
                        out.push(act(DiscoveryAction::Close));
                    }
                });
            });
            ui.add_space(8.0);
            Frame::new()
                .fill(palette.rail)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.discovery.search)
                            .frame(Frame::NONE)
                            .hint_text(RichText::new("Filtrar comunidades").color(palette.muted))
                            .font(font(Weight::Regular, 15.0))
                            .desired_width(f32::INFINITY),
                    );
                });
            ui.add_space(12.0);
            if let Some(error) = &app.discovery.error {
                ui.label(RichText::new(error).color(palette.danger));
            }
            let Some(list) = app.discovery.guilds.clone() else {
                ui.spinner();
                return;
            };
            let filter = app.discovery.search.to_lowercase();
            let list: Vec<_> = list
                .into_iter()
                .filter(|g| {
                    filter.is_empty()
                        || g.name.to_lowercase().contains(&filter)
                        || g.description.to_lowercase().contains(&filter)
                })
                .collect();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let width = ui.available_width();
                let columns = ((width / 300.0).floor() as usize).clamp(1, 4);
                let card = (width - 16.0 * (columns as f32 - 1.0)) / columns as f32;
                for row in list.chunks(columns) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
                        for g in row {
                            listed_card(app, ui, g, card, &mut out);
                        }
                    });
                    ui.add_space(16.0);
                }
            });
        });
    app.actions.extend(out);
}

fn listed_card(app: &App, ui: &mut Ui, g: &crate::discovery::Listed, width: f32, out: &mut Vec<Action>) {
    let palette = app.palette;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 300.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(8),
        if response.hovered() { palette.hover } else { palette.sidebar },
    );
    let splash = Rect::from_min_size(rect.min, vec2(width, 136.0));
    match g.splash.as_deref().and_then(|s| app.picture(Some(s))) {
        Some(uri) => {
            egui::Image::new(uri)
                .corner_radius(CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 })
                .paint_at(ui, splash);
        }
        None => {
            ui.painter().rect_filled(
                splash,
                CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 },
                palette.accent.gamma_multiply(0.6),
            );
        }
    }
    let icon = Rect::from_center_size(pos2(rect.left() + 36.0, splash.bottom()), Vec2::splat(44.0));
    ui.painter().rect_filled(icon.expand(4.0), CornerRadius::same(14), palette.sidebar);
    match g.icon.as_deref().and_then(|i| app.picture(Some(i))) {
        Some(uri) => {
            egui::Image::new(uri)
                .corner_radius(CornerRadius::same(12))
                .paint_at(ui, icon);
        }
        None => {
            ui.painter().rect_filled(icon, CornerRadius::same(12), palette.accent);
        }
    }
    Elide::new(ui, rect.right() - 12.0).text(
        pos2(rect.left() + 14.0, splash.bottom() + 40.0),
        Align2::LEFT_CENTER,
        &g.name,
        font(Weight::Bold, 16.0),
        palette.strong,
    );
    let text = ui.painter().layout(
        g.description.clone(),
        font(Weight::Regular, 13.0),
        palette.muted,
        width - 28.0,
    );
    let clip = Rect::from_min_size(pos2(rect.left() + 14.0, splash.bottom() + 54.0), vec2(width - 28.0, 54.0));
    ui.painter().with_clip_rect(clip).galley(clip.min, text, palette.muted);
    let y = rect.bottom() - 18.0;
    ui.painter().circle_filled(pos2(rect.left() + 18.0, y), 4.0, palette.online);
    ui.painter().text(
        pos2(rect.left() + 26.0, y),
        Align2::LEFT_CENTER,
        format!("{} online", g.online),
        font(Weight::Regular, 12.0),
        palette.muted,
    );
    ui.painter().circle_filled(pos2(rect.left() + 118.0, y), 4.0, palette.muted);
    ui.painter().text(
        pos2(rect.left() + 126.0, y),
        Align2::LEFT_CENTER,
        format!("{} membros", g.members),
        font(Weight::Regular, 12.0),
        palette.muted,
    );
    if app.discovery.joining == Some(g.id) {
        ui.painter().text(
            pos2(rect.right() - 14.0, y),
            Align2::RIGHT_CENTER,
            "Entrando…",
            font(Weight::Medium, 12.0),
            palette.text,
        );
    }
    if response.clicked() {
        out.push(act(DiscoveryAction::Join(g.id)));
    }
}

/// A new server's rules ("Termine as etapas") and its onboarding
/// questions, one at a time.
pub fn dialogs(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let mut out = Vec::new();
    if let Some(s) = app.discovery.screening.clone() {
        let name = app.model.guild(s.guild).map(|g| g.name.clone()).unwrap_or_default();
        let response = egui::Modal::new(egui::Id::new("screening"))
            .backdrop_color(Color32::from_black_alpha(180))
            .frame(Frame::new().fill(palette.chat).corner_radius(CornerRadius::same(8)))
            .show(ui.ctx(), |ui| {
                ui.set_width(460.0);
                Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
                    ui.label(RichText::new(format!("Antes de conversar em {name}")).font(font(Weight::SemiBold, 20.0)).color(palette.strong));
                    if let Some(d) = &s.description {
                        ui.label(RichText::new(d).color(palette.muted));
                    }
                    ui.label(RichText::new("REGRAS DO SERVIDOR").font(font(Weight::Bold, 12.0)).color(palette.muted));
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for (i, rule) in s.rules.iter().enumerate() {
                            Frame::new().fill(palette.sidebar).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(10)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new(format!("{}. {rule}", i + 1)).color(palette.text));
                            });
                        }
                    });
                    let mut agreed = s.agreed;
                    if ui.checkbox(&mut agreed, RichText::new("Li e concordo com as regras").color(palette.text)).changed() {
                        out.push(act(DiscoveryAction::Agree(agreed)));
                    }
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let send = egui::Button::new(RichText::new(if s.busy { "Enviando…" } else { "Enviar" }).color(Color32::WHITE))
                                .fill(palette.accent)
                                .min_size(vec2(100.0, 34.0));
                            if ui.add_enabled(s.agreed && !s.busy, send).clicked() {
                                out.push(act(DiscoveryAction::SubmitScreening));
                            }
                            if ui.add(egui::Button::new(RichText::new("Depois").color(palette.text)).frame(false)).clicked() {
                                out.push(act(DiscoveryAction::CloseScreening));
                            }
                        });
                    });
                });
            });
        if response.should_close() {
            out.push(act(DiscoveryAction::CloseScreening));
        }
    } else if let Some(o) = app.discovery.onboarding.clone() {
        let name = app.model.guild(o.guild).map(|g| g.name.clone()).unwrap_or_default();
        let Some(prompt) = o.prompts.get(o.step).cloned() else {
            app.actions.extend(out);
            return;
        };
        let last = o.step + 1 >= o.prompts.len();
        let picked = o.picked.get(&prompt.id).cloned().unwrap_or_default();
        let response = egui::Modal::new(egui::Id::new("onboarding"))
            .backdrop_color(Color32::from_black_alpha(180))
            .frame(Frame::new().fill(palette.chat).corner_radius(CornerRadius::same(8)))
            .show(ui.ctx(), |ui| {
                ui.set_width(520.0);
                Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    ui.label(RichText::new(format!("Boas-vindas a {name}")).font(font(Weight::Medium, 13.0)).color(palette.muted));
                    ui.label(RichText::new(&prompt.title).font(font(Weight::SemiBold, 20.0)).color(palette.strong));
                    ui.label(
                        RichText::new(format!(
                            "Pergunta {} de {}{}",
                            o.step + 1,
                            o.prompts.len(),
                            if prompt.single { " · escolha uma" } else { " · escolha quantas quiser" }
                        ))
                        .color(palette.muted),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for (id, title, description) in &prompt.options {
                            let on = picked.contains(id);
                            let mut button = egui::Button::new(RichText::new(title).font(font(Weight::Medium, 14.0)).color(if on { Color32::WHITE } else { palette.text }))
                                .fill(if on { palette.accent } else { palette.sidebar })
                                .min_size(vec2(150.0, 40.0));
                            if let Some(d) = description {
                                button = button.right_text(RichText::new(d).font(font(Weight::Regular, 12.0)));
                            }
                            if ui.add(button).clicked() {
                                out.push(act(DiscoveryAction::Pick { prompt: prompt.id, option: *id }));
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if o.step > 0 && ui.button("Voltar").clicked() {
                            out.push(act(DiscoveryAction::Step(o.step - 1)));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let ready = !prompt.required || !picked.is_empty();
                            let label = if last { "Concluir" } else { "Próximo" };
                            let next = egui::Button::new(RichText::new(label).color(Color32::WHITE))
                                .fill(palette.accent)
                                .min_size(vec2(100.0, 34.0));
                            if ui.add_enabled(ready, next).clicked() {
                                out.push(act(if last {
                                    DiscoveryAction::FinishOnboarding
                                } else {
                                    DiscoveryAction::Step(o.step + 1)
                                }));
                            }
                            if !prompt.required && ui.add(egui::Button::new(RichText::new("Pular").color(palette.text)).frame(false)).clicked() {
                                out.push(act(if last {
                                    DiscoveryAction::FinishOnboarding
                                } else {
                                    DiscoveryAction::Step(o.step + 1)
                                }));
                            }
                        });
                    });
                });
            });
        if response.should_close() {
            out.push(act(DiscoveryAction::SkipOnboarding));
        }
    }
    app.actions.extend(out);
}
