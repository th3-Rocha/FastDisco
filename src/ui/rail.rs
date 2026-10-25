//! The server rail: home, the servers, then adding and discovering servers.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Ui, Vec2, pos2, vec2};
use fastframe_fonts::Weight;

use super::widgets::{badge, font, paint_icon};
use crate::app::{Action, App, Place};
use crate::icons::Icon;
use crate::theme::RAIL_WIDTH;

const ICON: f32 = 48.0;
const GAP: f32 = 8.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::Panel::left("server-rail")
        .resizable(false)
        .exact_size(RAIL_WIDTH)
        .show_separator_line(false)
        .frame(Frame::new().fill(palette.rail).inner_margin(Margin::ZERO))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("server-rail")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.add_space(12.0);
                    let unread_dms: u32 = app
                        .model
                        .direct
                        .iter()
                        .filter_map(|id| app.model.channel(*id))
                        .map(|c| c.mentions)
                        .sum();
                    let home = item(
                        ui,
                        app,
                        app.place == Place::Home,
                        false,
                        unread_dms,
                        "Mensagens diretas",
                        |ui, rect, lit| {
                            let fill = if lit { palette.accent } else { palette.chat };
                            ui.painter().rect_filled(rect, radius(lit), fill);
                            let tint = if lit { Color32::WHITE } else { palette.text };
                            paint_icon(ui, Icon::Threads, rect.center(), 26.0, tint);
                        },
                    );
                    if home.clicked() {
                        app.actions.push(Action::GoHome);
                    }
                    ui.add_space(GAP);
                    let (line, _) = ui.allocate_exact_size(vec2(RAIL_WIDTH, 2.0), Sense::hover());
                    ui.painter().rect_filled(
                        Rect::from_center_size(line.center(), vec2(32.0, 2.0)),
                        CornerRadius::same(1),
                        palette.selected,
                    );
                    ui.add_space(GAP);

                    let mut open = None;
                    let mut menu: Option<Action> = None;
                    for guild in &app.model.guilds {
                        let (unread, mentions) = app.model.guild_unread(guild);
                        let selected = app.place == Place::Server(guild.id);
                        let icon = app.picture(guild.icon.as_deref());
                        let response = item(
                            ui,
                            app,
                            selected,
                            unread,
                            mentions,
                            &guild.name,
                            |ui, rect, lit| match icon {
                                Some(uri) => {
                                    egui::Image::new(uri)
                                        .corner_radius(radius(lit))
                                        .paint_at(ui, rect);
                                }
                                None => {
                                    let fill = if lit { palette.accent } else { palette.chat };
                                    ui.painter().rect_filled(rect, radius(lit), fill);
                                    let text = acronym(&guild.name);
                                    let size = match text.chars().count() {
                                        0..=2 => 18.0,
                                        3 => 14.0,
                                        _ => 11.0,
                                    };
                                    let tint = if lit { Color32::WHITE } else { palette.text };
                                    ui.painter().text(
                                        rect.center(),
                                        Align2::CENTER_CENTER,
                                        text,
                                        font(Weight::Medium, size),
                                        tint,
                                    );
                                }
                            },
                        );
                        if response.clicked() {
                            open = Some(guild.id);
                        }
                        let id = guild.id;
                        response.context_menu(|ui| {
                            ui.set_min_width(180.0);
                            if ui
                                .add_enabled(unread, egui::Button::new("Marcar como lido"))
                                .clicked()
                            {
                                menu = Some(Action::MarkGuildRead(id));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Copiar ID do servidor").clicked() {
                                menu = Some(Action::Chat(crate::chat::ChatAction::Copy(
                                    id.to_string(),
                                )));
                                ui.close();
                            }
                        });
                        ui.add_space(GAP);
                    }
                    if let Some(id) = open {
                        app.actions.push(Action::OpenServer(id));
                    }
                    if let Some(action) = menu {
                        app.actions.push(action);
                    }

                    // Creating and joining servers open Discord's own pages
                    // for now.
                    for (icon, hint, url) in [
                        (
                            Icon::Plus,
                            "Adicionar um servidor",
                            "https://discord.com/channels/@me",
                        ),
                        (
                            Icon::Compass,
                            "Explorar servidores",
                            "https://discord.com/discovery",
                        ),
                        (
                            Icon::Download,
                            "Baixar apps",
                            "https://discord.com/download",
                        ),
                    ] {
                        let response = item(ui, app, false, false, 0, hint, |ui, rect, lit| {
                            ui.painter().rect_filled(
                                rect,
                                radius(lit),
                                if lit { palette.online } else { palette.chat },
                            );
                            paint_icon(
                                ui,
                                icon,
                                rect.center(),
                                24.0,
                                if lit { Color32::WHITE } else { palette.online },
                            );
                        });
                        if response.clicked() {
                            app.actions
                                .push(Action::Chat(crate::chat::ChatAction::OpenUrl(url.into())));
                        }
                        ui.add_space(GAP);
                    }
                });
        });
}

/// A circle that becomes a rounded square under the pointer or when chosen.
fn radius(lit: bool) -> CornerRadius {
    CornerRadius::same(if lit { 16 } else { 24 })
}

/// One rail slot with the pill on its left: a dot for unread, taller under
/// the pointer, tallest for the open server.
fn item(
    ui: &mut Ui,
    app: &App,
    selected: bool,
    unread: bool,
    mentions: u32,
    hint: &str,
    paint: impl FnOnce(&Ui, Rect, bool),
) -> egui::Response {
    let palette = app.palette;
    let (slot, response) = ui.allocate_exact_size(vec2(RAIL_WIDTH, ICON), Sense::click());
    let hovered = response.hovered();
    let rect = Rect::from_center_size(slot.center(), Vec2::splat(ICON));
    paint(ui, rect, selected || hovered);

    let pill = if selected {
        40.0
    } else if hovered {
        20.0
    } else if unread {
        8.0
    } else {
        0.0
    };
    let pill = ui
        .ctx()
        .animate_value_with_time(response.id.with("pill"), pill, 0.12);
    if pill > 0.5 {
        let bar = Rect::from_center_size(pos2(slot.left(), slot.center().y), vec2(8.0, pill));
        ui.painter()
            .rect_filled(bar, CornerRadius::same(4), palette.strong);
    }
    if mentions > 0 {
        badge(
            ui,
            &palette,
            pos2(rect.right() + 2.0, rect.bottom() - 6.0),
            mentions,
            Some(palette.rail),
        );
    }
    if hovered {
        rail_tooltip(ui, rect, hint);
    }
    response
}

/// "Rust Brasil" is "RB", as the rail shows a server without an icon.
fn acronym(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(5)
        .collect()
}

/// Discord's tooltip beside the rail: the name in a dark bubble to the
/// right, pointing at the icon.
fn rail_tooltip(ui: &Ui, icon: Rect, text: &str) {
    let floating = Color32::from_rgb(0x11, 0x12, 0x14);
    let at = egui::pos2(icon.right() + 14.0, icon.center().y);
    egui::Area::new(egui::Id::new("rail-tooltip"))
        .order(egui::Order::Tooltip)
        .pivot(egui::Align2::LEFT_CENTER)
        .fixed_pos(at)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::new()
                .fill(floating)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    // Laid out at Discord's widest, breaking between words:
                    // a new Area starts narrow, and a label would wrap
                    // every letter.
                    let galley = ui.painter().layout(
                        super::widgets::hyphens(text).into_owned(),
                        font(Weight::SemiBold, 16.0),
                        Color32::WHITE,
                        196.0,
                    );
                    let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
                    ui.painter().galley(rect.min, galley, Color32::WHITE);
                });
            let r = frame.response.rect;
            let tip = [
                egui::pos2(r.left() - 5.0, r.center().y),
                egui::pos2(r.left(), r.center().y - 5.0),
                egui::pos2(r.left(), r.center().y + 5.0),
            ];
            ui.painter().add(egui::Shape::convex_polygon(
                tip.to_vec(),
                floating,
                egui::Stroke::NONE,
            ));
        });
}
