//! The soundboard's panel, over its button as Discord's: a search, then
//! Discord's sounds and each server's, as buttons with their emoji; a
//! click plays one for everyone, the speaker beside it for the user alone.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Ui, Vec2, pos2, vec2};
use fastframe_fonts::Weight;

use super::widgets::{Elide, font, paint_icon};
use crate::app::{Action, App};
use crate::icons::Icon;
use crate::soundboard::{Sound, SoundboardAction};

const ANCHOR: &str = "soundboard-anchor";

/// Remembers where a soundboard button is, for the panel to rise from.
pub fn remember(ui: &Ui, rect: Rect) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(ANCHOR), rect));
}

pub fn panel(app: &mut App, ui: &mut Ui) {
    if !app.soundboard.open || app.call.is_none() {
        return;
    }
    let palette = app.palette;
    let anchor: Rect = ui
        .data(|d| d.get_temp(egui::Id::new(ANCHOR)))
        .unwrap_or_else(|| Rect::from_center_size(ui.ctx().content_rect().center_bottom(), Vec2::ZERO));
    let screen = ui.ctx().content_rect();
    let size = vec2(440.0, 420.0);
    let left = (anchor.center().x - size.x / 2.0).clamp(screen.left() + 8.0, screen.right() - size.x - 8.0);
    let top = (anchor.top() - size.y - 8.0).max(screen.top() + 8.0);
    let mut out: Vec<Action> = Vec::new();
    let area = egui::Area::new(egui::Id::new("soundboard"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(left, top))
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(crate::theme::floating(&palette))
                .stroke(egui::Stroke::new(1.0, crate::theme::floating_edge(&palette)))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(12))
                .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(120) })
                .show(ui, |ui| {
                    ui.set_width(size.x - 24.0);
                    ui.set_max_height(size.y - 24.0);
                    Frame::new()
                        .fill(palette.rail)
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut app.soundboard.search)
                                    .frame(Frame::NONE)
                                    .hint_text(RichText::new("Buscar sons").color(palette.muted))
                                    .font(font(Weight::Regular, 14.0))
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    ui.add_space(8.0);
                    let filter = app.soundboard.search.trim().to_lowercase();
                    let mut sections: Vec<(String, Vec<Sound>)> = Vec::new();
                    if let Some(list) = &app.soundboard.defaults {
                        sections.push(("PADRÕES".into(), list.clone()));
                    }
                    for guild in &app.model.guilds {
                        if let Some(list) = app.soundboard.guilds.get(&guild.id) {
                            sections.push((guild.name.to_uppercase(), list.clone()));
                        }
                    }
                    let playing = app
                        .soundboard
                        .playing
                        .filter(|(_, at)| at.elapsed().as_secs_f32() < 1.5)
                        .map(|(id, _)| id);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        if sections.is_empty() {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("Carregando sons…").color(palette.muted));
                            });
                        }
                        for (title, list) in sections {
                            let list: Vec<Sound> = list
                                .into_iter()
                                .filter(|s| filter.is_empty() || s.name.to_lowercase().contains(&filter))
                                .collect();
                            if list.is_empty() {
                                continue;
                            }
                            ui.label(RichText::new(title).font(font(Weight::Bold, 12.0)).color(palette.muted));
                            ui.add_space(4.0);
                            let width = ui.available_width();
                            let columns = 3usize;
                            let cell = (width - 8.0 * (columns as f32 - 1.0)) / columns as f32;
                            for row in list.chunks(columns) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                                    for sound in row {
                                        sound_button(app, ui, sound, cell, playing == Some(sound.id), &mut out);
                                    }
                                });
                                ui.add_space(8.0);
                            }
                            ui.add_space(6.0);
                        }
                    });
                });
        });
    if area.response.clicked_elsewhere() && !ui.ctx().input(|i| i.pointer.any_down()) {
        out.push(Action::Soundboard(SoundboardAction::Close));
    }
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        out.push(Action::Soundboard(SoundboardAction::Close));
    }
    app.actions.extend(out);
}

fn sound_button(app: &App, ui: &mut Ui, sound: &Sound, width: f32, playing: bool, out: &mut Vec<Action>) {
    let palette = app.palette;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    let preview = Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), Vec2::splat(24.0));
    let over_preview = response.hover_pos().is_some_and(|p| preview.contains(p));
    let fill = if playing {
        palette.accent
    } else if response.hovered() {
        crate::theme::floating_edge(&palette)
    } else {
        palette.rail
    };
    ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    let mut x = rect.left() + 10.0;
    if let Some(id) = sound.emoji_id
        && let Some(uri) = app.picture(Some(&crate::model::CustomEmoji::url(id, false)))
    {
        egui::Image::new(uri).paint_at(ui, Rect::from_center_size(pos2(x + 9.0, rect.center().y), Vec2::splat(18.0)));
        x += 24.0;
    } else if let Some(emoji) = &sound.emoji {
        ui.painter().text(pos2(x, rect.center().y), Align2::LEFT_CENTER, emoji, font(Weight::Regular, 16.0), palette.text);
        x += 24.0;
    }
    let text_right = if response.hovered() { preview.left() - 4.0 } else { rect.right() - 8.0 };
    Elide::new(ui, text_right).text(
        pos2(x, rect.center().y),
        Align2::LEFT_CENTER,
        &sound.name,
        font(Weight::Medium, 14.0),
        if playing { Color32::WHITE } else { palette.text },
    );
    if response.hovered() {
        paint_icon(ui, Icon::Volume, preview.center(), 16.0, if over_preview { palette.strong } else { palette.icon });
    }
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Clique para tocar para todos; o alto-falante, só para você");
    if response.clicked() {
        let action = if over_preview {
            SoundboardAction::Preview(sound.id, sound.volume)
        } else {
            SoundboardAction::Play(sound.id, sound.volume)
        };
        out.push(Action::Soundboard(action));
    }
}
