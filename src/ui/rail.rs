//! The server rail: home, the servers, then adding a server.

use egui::{
    Align2, Color32, CornerRadius, Frame, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{badge, font, paint_icon};
use crate::admin::{AdminAction, Modal};
use crate::app::{Action, App, Place};
use crate::folders::{Drop, Key};
use crate::icons::Icon;
use crate::model::{Id, permission};
use crate::nav::NavAction;
use crate::theme::RAIL_WIDTH;

/// Discord's server icons since its 2025 redesign.
const ICON: f32 = 44.0;
const GAP: f32 = 7.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::Panel::left("server-rail")
        .resizable(false)
        .exact_size(RAIL_WIDTH)
        .show_separator_line(false)
        // The window behind is already the frame colour.
        .frame(Frame::NONE)
        .show(ui, |ui| {
            // Room for the user's card, which spans the rail and sidebar.
            egui::Panel::bottom("rail-foot")
                .resizable(false)
                .exact_size(super::sidebar::card_space(app))
                .show_separator_line(false)
                .frame(Frame::NONE)
                .show(ui, |_| {});
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
                        &[],
                        false,
                        |ui, rect, lit| {
                            let fill = if lit { palette.accent } else { palette.chat };
                            ui.painter().rect_filled(rect, radius(lit), fill);
                            let face = if lit { Color32::WHITE } else { palette.text };
                            paint_snout(ui, rect.center(), 30.0, face, palette.accent);
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

                    servers(app, ui);

                    // Adding a server opens Discord's "Criar seu servidor",
                    // on a grey tile that lights up blurple.
                    {
                        let (icon, hint) = (Icon::Plus, "Adicionar um servidor");
                        let response = item(ui, app, false, false, 0, hint, &[], false, |ui, rect, lit| {
                            ui.painter().rect_filled(
                                rect,
                                radius(lit),
                                if lit { palette.accent } else { palette.chat },
                            );
                            super::widgets::paint_icon_state(
                                ui,
                                icon,
                                rect.center(),
                                22.0,
                                if lit { Color32::WHITE } else { palette.strong },
                                lit,
                            );
                        });
                        if response.clicked() {
                            app.actions.push(Action::Admin(AdminAction::Open(
                                Modal::CreateGuild {
                                    step: crate::admin::CreateStep::Start,
                                    template: 0,
                                    name: String::new(),
                                    icon: None,
                                },
                            )));
                        }
                        ui.add_space(GAP);
                    }
                    // "Explorar", the compass under the add tile.
                    {
                        let open = app.discovery.open;
                        let response = item(ui, app, open, false, 0, "Explorar", &[], false, |ui, rect, lit| {
                            ui.painter().rect_filled(
                                rect,
                                radius(lit || open),
                                if lit || open { palette.online } else { palette.chat },
                            );
                            super::widgets::paint_icon_state(
                                ui,
                                Icon::Compass,
                                rect.center(),
                                22.0,
                                if lit || open { Color32::WHITE } else { palette.online },
                                lit,
                            );
                        });
                        if response.clicked() {
                            app.actions.push(Action::Discovery(
                                crate::discovery::DiscoveryAction::Open,
                            ));
                        }
                        ui.add_space(GAP);
                    }
                });
        });
}

/// The tooltip's text colour, Discord's near-white.
const TOOLTIP_TEXT: Color32 = Color32::from_rgb(0xf0, 0xf0, 0xf1);

/// How many servers a folder's tooltip names before "...".
const FOLDER_NAMES: usize = 5;

/// The Discofast snout, drawn: a face and two nostrils tilted outwards,
/// `width` wide, as assets/icon.svg.
fn paint_snout(ui: &Ui, center: egui::Pos2, width: f32, face: Color32, nostril: Color32) {
    let unit = width / (2.0 * 236.0);
    let ellipse = |cx: f32, rx: f32, ry: f32, tilt: f32| -> Vec<egui::Pos2> {
        let (sin, cos) = tilt.to_radians().sin_cos();
        (0..48)
            .map(|i| {
                let a = i as f32 / 48.0 * std::f32::consts::TAU;
                let (x, y) = (rx * a.cos(), ry * a.sin());
                center + vec2(cx + x * cos - y * sin, x * sin + y * cos) * unit
            })
            .collect()
    };
    let painter = ui.painter();
    painter.add(Shape::convex_polygon(ellipse(0.0, 236.0, 184.0, 0.0), face, Stroke::NONE));
    for (cx, tilt) in [(-80.0, -8.0), (80.0, 8.0)] {
        painter.add(Shape::convex_polygon(ellipse(cx, 40.0, 70.0, tilt), nostril, Stroke::NONE));
    }
}

/// The server being dragged, kept in egui's memory while the button is down.
const DRAG: &str = "rail-drag";

/// What a dragged server can land on or beside.
#[derive(Clone, Copy)]
enum Slot {
    /// A server alone, or a folder's head (open or not).
    Entry(Key, bool),
    /// A server in an open folder.
    Inside(Key, Id),
}

/// The servers, alone and in folders, and dragging them around.
fn servers(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let mut out: Vec<Action> = Vec::new();
    let mut slots: Vec<(Rect, Slot)> = Vec::new();
    let folders = app.model.folders.clone();
    for folder in &folders {
        let key = Key::of(folder);
        let Some(id) = folder.id else {
            let response = guild_item(app, ui, folder.guilds[0], &mut out);
            slots.push((response.rect, Slot::Entry(key, false)));
            ui.add_space(GAP);
            continue;
        };
        let open = app.nav.open_folders.contains(&id);
        let background = ui.painter().add(Shape::Noop);
        let top = ui.cursor().top();
        let (unread, mentions) = folder
            .guilds
            .iter()
            .filter_map(|g| app.model.guild(*g))
            .map(|g| app.model.guild_unread(g))
            .fold((false, 0), |(u, m), (gu, gm)| (u || gu, m + gm));
        let inside = app.server().is_some_and(|s| folder.guilds.contains(&s));
        // Its name, or its first servers' names and "...", as Discord's.
        let hint = folder.name.clone().unwrap_or_else(|| {
            let names: Vec<&str> = folder
                .guilds
                .iter()
                .filter_map(|g| app.model.guild(*g))
                .map(|g| g.name.as_str())
                .collect();
            let mut text = names.iter().take(FOLDER_NAMES).copied().collect::<Vec<_>>().join(", ");
            if names.len() > FOLDER_NAMES {
                text.push_str(", ...");
            }
            text
        });
        let color = folder_color(folder.color);
        let head = item(
            ui,
            app,
            inside && !open,
            unread && !open,
            if open { 0 } else { mentions },
            &hint,
            // A folder shows neither the call nor the event corner, as
            // Discord's: those are on its servers once it is open.
            &[],
            false,
            |ui, rect, _| {
                if open {
                    paint_icon(ui, Icon::Folder, rect.center(), 24.0, color);
                    return;
                }
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(16), color.gamma_multiply(0.4));
                for (i, guild) in folder.guilds.iter().take(4).enumerate() {
                    let dx = if i % 2 == 0 { -11.0 } else { 11.0 };
                    let dy = if i < 2 { -11.0 } else { 11.0 };
                    let mini =
                        Rect::from_center_size(rect.center() + vec2(dx, dy), Vec2::splat(18.0));
                    if let Some(g) = app.model.guild(*guild) {
                        paint_guild(ui, app, g, mini, false);
                    }
                }
            },
        );
        if head.clicked() {
            out.push(Action::Nav(NavAction::ToggleFolder(id)));
        }
        slots.push((head.rect, Slot::Entry(key, open)));
        if open {
            for guild in &folder.guilds {
                ui.add_space(GAP);
                let response = guild_item(app, ui, *guild, &mut out);
                slots.push((response.rect, Slot::Inside(key, *guild)));
            }
            // An open folder sits on a band of its own, as Discord's.
            let center = head.rect.center().x;
            let band = Rect::from_min_max(
                pos2(center - ICON / 2.0, top),
                pos2(center + ICON / 2.0, ui.cursor().top()),
            );
            ui.painter().set(
                background,
                Shape::rect_filled(band, CornerRadius::same(16), palette.sidebar),
            );
        }
        ui.add_space(GAP);
    }
    drag(app, ui, &slots, &mut out);
    app.actions.extend(out);
}

/// One server in the rail: its picture or initials, opening it on a
/// click, its menu, and picking it up to drag.
fn guild_item(app: &App, ui: &mut Ui, id: Id, out: &mut Vec<Action>) -> egui::Response {
    let Some(guild) = app.model.guild(id) else {
        let (rect, response) = ui.allocate_exact_size(vec2(RAIL_WIDTH, ICON), Sense::hover());
        let _ = rect;
        return response;
    };
    let (unread, mentions) = app.model.guild_unread(guild);
    let selected = app.place == Place::Server(id);
    let voice = in_voice(app, id);
    let response = item(
        ui,
        app,
        selected,
        unread,
        mentions,
        &guild.name,
        &voice,
        !guild.events.is_empty(),
        |ui, rect, lit| {
            paint_guild(ui, app, guild, rect, lit);
        },
    );
    if response.drag_started() {
        ui.data_mut(|d| d.insert_temp(egui::Id::new(DRAG), id));
    }
    if response.clicked() {
        out.push(Action::OpenServer(id));
    }
    let can_settings = [
        permission::MANAGE_GUILD,
        permission::MANAGE_ROLES,
        permission::BAN,
        permission::VIEW_AUDIT_LOG,
    ]
    .iter()
    .any(|bit| app.model.can(id, *bit));
    let owner = guild.owner == app.model.me;
    response.context_menu(|ui| {
        ui.set_min_width(180.0);
        if ui
            .add_enabled(
                unread || mentions > 0,
                egui::Button::new("Marcar como lido"),
            )
            .clicked()
        {
            out.push(Action::MarkGuildRead(id));
            ui.close();
        }
        ui.separator();
        if app.model.can(id, permission::CREATE_INVITE) && ui.button("Convidar pessoas").clicked() {
            out.push(Action::OpenServer(id));
            out.push(Action::Admin(AdminAction::Open(Modal::Invite {
                link: None,
            })));
            ui.close();
        }
        if can_settings && ui.button("Configurações do servidor").clicked() {
            out.push(Action::Admin(AdminAction::OpenSettings(id)));
            ui.close();
        }
        if !owner {
            let leave = egui::Button::new(
                egui::RichText::new("Sair do servidor").color(app.palette.danger),
            );
            if ui.add(leave).clicked() {
                out.push(Action::Admin(AdminAction::Open(Modal::Leave { guild: id })));
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Copiar ID do servidor").clicked() {
            out.push(Action::Chat(crate::chat::ChatAction::Copy(id.to_string())));
            ui.close();
        }
    });
    response
}

/// A server's picture, or its initials on a plain tile.
pub(super) fn paint_guild(ui: &Ui, app: &App, guild: &crate::model::Guild, rect: Rect, lit: bool) {
    let palette = app.palette;
    let small = rect.width() < 30.0;
    let corner = if small {
        CornerRadius::same(5)
    } else {
        radius(lit)
    };
    match app.picture(guild.icon.as_deref()) {
        Some(uri) => {
            egui::Image::new(uri)
                .corner_radius(corner)
                .paint_at(ui, rect);
        }
        None => {
            let fill = if lit { palette.accent } else { palette.chat };
            ui.painter().rect_filled(rect, corner, fill);
            let text = acronym(&guild.name);
            let size = match (small, text.chars().count()) {
                (true, _) => 7.0,
                (false, 0..=2) => 18.0,
                (false, 3) => 14.0,
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
    }
}

/// A folder's colour, Discord's blurple when it has none.
fn folder_color(color: Option<u32>) -> Color32 {
    let c = color.filter(|c| *c != 0).unwrap_or(0x5865f2);
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// While a server is dragged: it follows the pointer, a line or a ring
/// shows where it would land, and letting go moves it there.
fn drag(app: &App, ui: &mut Ui, slots: &[(Rect, Slot)], out: &mut Vec<Action>) {
    let key = egui::Id::new(DRAG);
    let Some(guild) = ui.data(|d| d.get_temp::<Id>(key)) else {
        return;
    };
    let released = ui.input(|i| i.pointer.any_released() || !i.pointer.any_down());
    let Some(pointer) = ui.ctx().pointer_latest_pos() else {
        ui.data_mut(|d| d.remove::<Id>(key));
        return;
    };
    let palette = app.palette;
    let next_in = |folder: Key, after: Id| -> Option<Id> {
        let f = app.model.folders.iter().find(|f| Key::of(f) == folder)?;
        let at = f.guilds.iter().position(|g| *g == after)?;
        f.guilds.get(at + 1).copied()
    };
    let first_in = |folder: Key| -> Option<Id> {
        app.model
            .folders
            .iter()
            .find(|f| Key::of(f) == folder)
            .and_then(|f| f.guilds.first().copied())
    };
    enum Mark {
        Line(f32),
        Ring(Rect),
    }
    let target = slots
        .iter()
        .find(|(r, _)| pointer.y >= r.top() - GAP / 2.0 && pointer.y < r.bottom() + GAP / 2.0)
        .and_then(|(r, slot)| {
            let rel = (pointer.y - r.top()) / r.height();
            let above = r.top() - GAP / 2.0;
            let below = r.bottom() + GAP / 2.0;
            match *slot {
                Slot::Entry(Key::Guild(g), _) if g == guild => None,
                Slot::Entry(k, _) if rel < 0.25 => Some((Drop::Before(k), Mark::Line(above))),
                Slot::Entry(k, true) if rel > 0.75 => {
                    Some((Drop::Into(k, first_in(k)), Mark::Line(below)))
                }
                Slot::Entry(k, false) if rel > 0.75 => Some((Drop::After(k), Mark::Line(below))),
                Slot::Entry(k, _) => Some((Drop::Into(k, None), Mark::Ring(*r))),
                Slot::Inside(_, g) if g == guild => None,
                Slot::Inside(k, g) if rel < 0.5 => {
                    Some((Drop::Into(k, Some(g)), Mark::Line(above)))
                }
                Slot::Inside(k, g) => Some((Drop::Into(k, next_in(k, g)), Mark::Line(below))),
            }
        });
    let layer = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        key.with("mark"),
    ));
    let center = slots
        .first()
        .map_or(RAIL_WIDTH / 2.0, |(r, _)| r.center().x);
    match target.as_ref().map(|(_, m)| m) {
        Some(Mark::Line(y)) => {
            layer.rect_filled(
                Rect::from_center_size(pos2(center, *y), vec2(40.0, 3.0)),
                CornerRadius::same(2),
                palette.online,
            );
        }
        Some(Mark::Ring(rect)) => {
            let ring = Rect::from_center_size(rect.center(), Vec2::splat(ICON + 4.0));
            layer.rect_stroke(
                ring,
                CornerRadius::same(18),
                Stroke::new(2.0, palette.online),
                egui::StrokeKind::Outside,
            );
        }
        None => {}
    }
    if let Some(g) = app.model.guild(guild) {
        egui::Area::new(key.with("ghost"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer - Vec2::splat(ICON / 2.0))
            .interactable(false)
            .show(ui.ctx(), |ui| {
                ui.set_opacity(0.8);
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(ICON), Sense::hover());
                paint_guild(ui, app, g, rect, true);
            });
    }
    ui.ctx().request_repaint();
    if released {
        ui.data_mut(|d| d.remove::<Id>(key));
        if let Some((drop, _)) = target {
            out.push(Action::Nav(NavAction::MoveGuild { guild, drop }));
        }
    }
}

/// A circle that becomes a rounded square under the pointer or when chosen.
fn radius(lit: bool) -> CornerRadius {
    // Discord's icons are rounded squares now, a little rounder at rest.
    CornerRadius::same(if lit { 13 } else { 15 })
}

/// One rail slot with the pill on its left: a dot for unread, taller under
/// the pointer, tallest for the open server.
#[allow(clippy::too_many_arguments)]
fn item(
    ui: &mut Ui,
    app: &App,
    selected: bool,
    unread: bool,
    mentions: u32,
    hint: &str,
    voice: &[Id],
    event: bool,
    paint: impl FnOnce(&Ui, Rect, bool),
) -> egui::Response {
    let palette = app.palette;
    let (slot, response) = ui.allocate_exact_size(vec2(RAIL_WIDTH, ICON), Sense::click_and_drag());
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
    // Someone is in a call here, or an event is on: Discord's little
    // dark tile at the top right corner, the call first.
    let corner = if !voice.is_empty() {
        Some(Icon::Speaker)
    } else if event {
        Some(Icon::Calendar)
    } else {
        None
    };
    if let Some(icon) = corner {
        let tile = Rect::from_center_size(pos2(rect.right() - 3.0, rect.top() + 3.0), Vec2::splat(20.0));
        ui.painter()
            .rect_filled(tile.expand(2.0), CornerRadius::same(7), palette.frame);
        ui.painter()
            .rect_filled(tile, CornerRadius::same(6), Color32::from_rgb(0x11, 0x12, 0x14));
        paint_icon(ui, icon, tile.center(), 13.0, palette.text);
    }
    if mentions > 0 {
        badge(
            ui,
            &palette,
            pos2(rect.right() + 2.0, rect.bottom() - 6.0),
            mentions,
            Some(palette.frame),
        );
    }
    if hovered && !ui.data(|d| d.get_temp::<Id>(egui::Id::new(DRAG)).is_some()) {
        rail_tooltip(ui, app, rect, hint, voice);
    }
    response
}

/// Who is in a call on a server, by channel then name.
fn in_voice(app: &App, guild: Id) -> Vec<Id> {
    let mut list: Vec<(Id, String, Id)> = app
        .model
        .voice
        .iter()
        .filter(|((g, _), _)| *g == Some(guild))
        .map(|((_, user), v)| {
            let name = app
                .model
                .user(*user)
                .map(|u| u.name.to_lowercase())
                .unwrap_or_default();
            (v.channel, name, *user)
        })
        .collect();
    list.sort();
    list.into_iter().map(|(_, _, user)| user).collect()
}

/// "Rust Brasil" is "RB", as the rail shows a server without an icon.
/// Discord's own rule: each run of ASCII letters, digits and _ becomes
/// its first character, anything else (accents, symbols) stays, and the
/// spaces go; "'s " counts as a space.
fn acronym(name: &str) -> String {
    let name = name.replace("'s ", " ");
    let mut out = String::new();
    let mut in_word = false;
    for c in name.chars() {
        let word = c.is_ascii_alphanumeric() || c == '_';
        if word && !in_word {
            out.push(c);
        } else if !word && !c.is_whitespace() {
            out.push(c);
        }
        in_word = word;
    }
    out
}


/// Discord's tooltip beside the rail: the name in a grey bubble with a
/// thin border to the right, pointing at the icon.
fn rail_tooltip(ui: &Ui, app: &App, icon: Rect, text: &str, voice: &[Id]) {
    // Discord's 2025 floating surface and its edge.
    let floating = Color32::from_rgb(0x39, 0x3a, 0x41);
    let edge = Color32::from_rgb(0x43, 0x45, 0x4c);
    let at = egui::pos2(icon.right() + 14.0, icon.center().y);
    egui::Area::new(egui::Id::new("rail-tooltip"))
        .order(egui::Order::Tooltip)
        .pivot(egui::Align2::LEFT_CENTER)
        .fixed_pos(at)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::new()
                .fill(floating)
                .stroke(Stroke::new(1.0, edge))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    // Laid out at Discord's widest, breaking between words:
                    // a new Area starts narrow, and a label would wrap
                    // every letter.
                    let galley = ui.painter().layout(
                        super::widgets::hyphens(text).into_owned(),
                        font(Weight::SemiBold, 15.0),
                        TOOLTIP_TEXT,
                        196.0,
                    );
                    let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
                    ui.painter().galley(rect.min, galley, TOOLTIP_TEXT);
                    if !voice.is_empty() {
                        voice_row(ui, app, voice, floating);
                    }
                });
            let r = frame.response.rect;
            // The arrow, its two outer sides in the border's colour.
            let (tip, top, bottom) = (
                egui::pos2(r.left() - 5.0, r.center().y),
                egui::pos2(r.left() + 0.5, r.center().y - 5.0),
                egui::pos2(r.left() + 0.5, r.center().y + 5.0),
            );
            ui.painter().add(egui::Shape::convex_polygon(
                vec![tip, top, bottom],
                floating,
                egui::Stroke::NONE,
            ));
            ui.painter()
                .add(egui::Shape::line(vec![top, tip, bottom], Stroke::new(1.0, edge)));
        });
}

/// The tooltip's call line: a speaker and the faces in the call,
/// overlapping, as Discord's.
fn voice_row(ui: &mut Ui, app: &App, voice: &[Id], behind: Color32) {
    const SHOWN: usize = 6;
    const RADIUS: f32 = 12.0;
    const STEP: f32 = 20.0;
    ui.add_space(6.0);
    let faces = voice.len().min(SHOWN);
    let more = voice.len() - faces;
    let width = 24.0 + faces as f32 * STEP + 4.0 + if more > 0 { 32.0 } else { 0.0 };
    let (row, _) = ui.allocate_exact_size(vec2(width, RADIUS * 2.0 + 4.0), Sense::hover());
    let y = row.center().y;
    paint_icon(ui, Icon::Speaker, pos2(row.left() + 9.0, y), 18.0, app.palette.icon);
    let mut x = row.left() + 26.0 + RADIUS;
    for user in voice.iter().take(SHOWN) {
        let center = pos2(x, y);
        ui.painter().circle_filled(center, RADIUS + 2.0, behind);
        match app.model.user(*user) {
            Some(user) => super::widgets::avatar(ui, app, user, center, RADIUS, None),
            None => {
                ui.painter()
                    .circle_filled(center, RADIUS, app.palette.selected);
            }
        }
        x += STEP;
    }
    if more > 0 {
        let center = pos2(x + 4.0, y);
        ui.painter().circle_filled(center, RADIUS + 2.0, behind);
        ui.painter()
            .circle_filled(center, RADIUS, app.palette.selected);
        ui.painter().text(
            center,
            Align2::CENTER_CENTER,
            format!("+{more}"),
            font(Weight::SemiBold, 11.0),
            app.palette.strong,
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn acronyms_follow_discord() {
        assert_eq!(super::acronym("Rust Brasil"), "RB");
        assert_eq!(super::acronym("Dós d rên"), "Dósdrên");
        assert_eq!(super::acronym("Bob's Place"), "BP");
        assert_eq!(super::acronym("C++ Devs"), "C++D");
    }
}
