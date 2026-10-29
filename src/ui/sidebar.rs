//! The column beside the rail: the server's channels or the direct
//! messages, and the user panel at its foot.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{Elide, avatar, badge, font, paint_icon};
use crate::app::{Action, App, Place};
use crate::icons::Icon;
use crate::model::{ChannelKind, Id, Status};
use crate::theme::{HEADER_HEIGHT, RAIL_WIDTH, SIDEBAR_RANGE};

/// The sidebar's place, for the line around it (`frame_line`).
pub fn show(app: &mut App, ui: &mut Ui) -> Rect {
    let palette = app.palette;
    let response = egui::Panel::left("sidebar")
        .resizable(true)
        .default_size(app.settings.sidebar_width)
        .size_range(SIDEBAR_RANGE)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(crate::theme::region(palette.sidebar, crate::theme::Region::Sidebar))
                .corner_radius(CORNER)
                .inner_margin(Margin::ZERO),
        )
        .show(ui, |ui| {
            // Room for the card drawn over the foot (`card`).
            egui::Panel::bottom("sidebar-foot")
                .resizable(false)
                .exact_size(card_space(app))
                .show_separator_line(false)
                .frame(Frame::NONE)
                .show(ui, |_| {});
            header(app, ui);
            egui::CentralPanel::default()
                .frame(Frame::new().inner_margin(Margin::ZERO))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("channels", app.server()))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            match app.place {
                                Place::Server(id) => channels(app, ui, id),
                                Place::Home => direct_messages(app, ui),
                            }
                        });
                });
        })
        .response;
    // Followed live, saved once the drag ends.
    let width = response.rect.width();
    let dirty = egui::Id::new("sidebar-width-dirty");
    if (width - app.settings.sidebar_width).abs() > 0.5 {
        app.settings.sidebar_width = width;
        ui.data_mut(|d| d.insert_temp(dirty, true));
    }
    if !ui.input(|i| i.pointer.any_down())
        && ui.data_mut(|d| d.remove_temp::<bool>(dirty).unwrap_or(false))
    {
        app.save_settings();
    }
    response.rect
}

/// The channel list's width now, which the user can drag.
pub fn width(app: &App) -> f32 {
    app.settings.sidebar_width.clamp(*SIDEBAR_RANGE.start(), *SIDEBAR_RANGE.end())
}

/// The sidebar's top left corner is round, as in Discord.
const CORNER: CornerRadius = CornerRadius {
    nw: 8,
    ne: 0,
    sw: 0,
    se: 0,
};

/// Discord's thin line under the title bar, rounding the sidebar's corner
/// and going down its left side.
pub fn frame_line(app: &App, ui: &Ui, sidebar: Rect, window: Rect) {
    let stroke = Stroke::new(1.0, crate::theme::mix(app.palette.rail, app.palette.selected, 0.5));
    let r = f32::from(CORNER.nw);
    let left = sidebar.left() + 0.5;
    let top = sidebar.top() + 0.5;
    let mut points = vec![pos2(left, sidebar.bottom())];
    let steps = 8;
    for i in 0..=steps {
        let angle = std::f32::consts::PI * (1.0 + 0.5 * i as f32 / steps as f32);
        points.push(pos2(left + r + r * angle.cos(), top + r + r * angle.sin()));
    }
    points.push(pos2(window.right(), top));
    ui.painter()
        .add(egui::Shape::line(points, stroke));
}

fn header(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::Panel::top("sidebar-header")
        .resizable(false)
        .exact_size(HEADER_HEIGHT)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin::ZERO))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            match app.place {
                Place::Server(id) => {
                    let response = ui.allocate_rect(rect, Sense::click());
                    if response.clicked() {
                        app.actions
                            .push(Action::Admin(crate::admin::AdminAction::ToggleMenu));
                    }
                    let open = app.admin.menu;
                    if response.hovered() || open {
                        ui.painter().rect_filled(rect, CORNER, palette.hover);
                    }
                    let name = app
                        .model
                        .guild(id)
                        .map(|g| g.name.clone())
                        .unwrap_or_default();
                    // The name with its chevron right after it, and
                    // "Convidar" at the far right, as Discord's.
                    let invite = app.model.can(id, crate::model::permission::CREATE_INVITE);
                    let text = Elide::new(ui, rect.right() - if invite { 64.0 } else { 36.0 })
                        .text(
                            pos2(rect.left() + 16.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            name,
                            font(Weight::Bold, 16.0),
                            palette.strong,
                        );
                    // An X while the menu is open, as Discord.
                    paint_icon(
                        ui,
                        if open { Icon::Close } else { Icon::ChevronDown },
                        pos2(text.right() + 12.0, rect.center().y),
                        18.0,
                        palette.strong,
                    );
                    if invite {
                        let button = Rect::from_center_size(
                            pos2(rect.right() - 22.0, rect.center().y),
                            Vec2::splat(28.0),
                        );
                        let button = ui.interact(button, ui.id().with("invite"), Sense::click());
                        paint_icon(
                            ui,
                            Icon::UserPlus,
                            button.rect.center(),
                            20.0,
                            if button.hovered() {
                                palette.strong
                            } else {
                                palette.icon
                            },
                        );
                        if button.on_hover_text("Convidar pessoas").clicked() {
                            app.actions
                                .push(Action::Admin(crate::admin::AdminAction::Open(
                                    crate::admin::Modal::Invite { link: None },
                                )));
                        }
                    }
                    super::admin::menu(app, ui, rect);
                }
                Place::Home => {
                    let field =
                        Rect::from_center_size(rect.center(), vec2(rect.width() - 20.0, 32.0));
                    ui.painter()
                        .rect_filled(field, CornerRadius::same(8), palette.composer);
                    ui.painter().text(
                        pos2(field.left() + 12.0, field.center().y),
                        Align2::LEFT_CENTER,
                        "Encontre ou comece uma conversa",
                        font(Weight::Medium, 14.0),
                        palette.muted,
                    );
                }
            }
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, palette.divider),
            );
        });
}

fn channels(app: &mut App, ui: &mut Ui, guild: Id) {
    let palette = app.palette;
    let Some(g) = app.model.guild(guild) else {
        return;
    };
    let (loose, categories) = app.model.layout(g);
    let movable = app
        .model
        .can(guild, crate::model::permission::MANAGE_CHANNELS);
    // Where a dragged channel can go: each row, and each category's name.
    let mut rows: Vec<Row> = Vec::new();
    ui.add_space(8.0);
    events_row(app, ui, guild);
    for id in loose {
        let rect = channel_row(app, ui, id, movable);
        rows.push((rect, None, Some(id), false));
        super::call::members(app, ui, id);
    }
    for (category, list) in categories {
        ui.add_space(16.0);
        let collapsed = app.collapsed.contains(&category);
        // The name as written, the chevron after it, as Discord's
        // current layout; the plus only under the pointer.
        let name = app
            .model
            .channel(category)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
        let tint = if response.hovered() {
            palette.text
        } else {
            palette.muted
        };
        let chevron = if collapsed {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        };
        let text = Elide::new(ui, rect.right() - 52.0).text(
            pos2(rect.left() + 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            font(Weight::Medium, 14.0),
            tint,
        );
        paint_icon(
            ui,
            chevron,
            pos2(text.right() + 9.0, rect.center().y),
            12.0,
            tint,
        );
        if response.hovered() {
            paint_icon(
                ui,
                Icon::Plus,
                pos2(rect.right() - 18.0, rect.center().y),
                16.0,
                palette.muted,
            );
        }
        if response.clicked() {
            app.actions.push(Action::ToggleCategory(category));
        }
        // Dropped on the name: first in the category.
        let first = list.first().copied();
        rows.push((rect, Some(category), first, true));
        for id in list {
            // A collapsed category still shows the open channel and unread ones.
            let unread = app.model.channel(id).is_some_and(|c| c.unread());
            if collapsed && Some(id) != app.channel && !unread {
                continue;
            }
            let rect = channel_row(app, ui, id, movable);
            rows.push((rect, Some(category), Some(id), false));
            super::call::members(app, ui, id);
        }
    }
    ui.add_space(8.0);
    if movable {
        drag_channel(app, ui, &rows);
    }
}

const CHANNEL_DRAG: &str = "channel-drag";

/// A place in the channel list: where it is drawn, its category, the
/// channel there (a category's first, for its name), and whether it is
/// the category's name.
type Row = (Rect, Option<Id>, Option<Id>, bool);

/// While a channel is dragged: a line where it would land, and letting
/// go moves it there.
fn drag_channel(app: &mut App, ui: &mut Ui, rows: &[Row]) {
    let key = egui::Id::new(CHANNEL_DRAG);
    let Some(channel) = ui.data(|d| d.get_temp::<Id>(key)) else {
        return;
    };
    let released = ui.input(|i| i.pointer.any_released() || !i.pointer.any_down());
    let Some(pointer) = ui.ctx().pointer_latest_pos() else {
        ui.data_mut(|d| d.remove::<Id>(key));
        return;
    };
    // The row under the pointer: its top half puts the channel before it,
    // the bottom half before the next row of the same category.
    let target = rows
        .iter()
        .enumerate()
        .find(|(_, (r, ..))| pointer.y >= r.top() && pointer.y < r.bottom())
        .map(|(i, (r, parent, id, header))| {
            if *header || pointer.y < r.center().y {
                (*parent, *id, if *header { r.bottom() } else { r.top() })
            } else {
                let next = rows
                    .get(i + 1)
                    .filter(|n| n.1 == *parent && !n.3)
                    .and_then(|n| n.2);
                (*parent, next, r.bottom())
            }
        });
    if let Some((_, _, y)) = target {
        let layer = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            key.with("mark"),
        ));
        let left = ui.max_rect().left() + 8.0;
        layer.rect_filled(
            Rect::from_min_max(
                pos2(left, y - 1.0),
                pos2(ui.max_rect().right() - 8.0, y + 1.0),
            ),
            CornerRadius::same(1),
            app.palette.online,
        );
    }
    ui.ctx().request_repaint();
    if released {
        ui.data_mut(|d| d.remove::<Id>(key));
        if let Some((parent, before, _)) = target
            && before != Some(channel)
        {
            app.actions
                .push(Action::Nav(crate::nav::NavAction::MoveChannel {
                    channel,
                    parent,
                    before,
                }));
        }
    }
}

/// "Eventos" at the top of the channel list, with a line under it, as
/// Discord; the count when there are some, and a click opens the next.
fn events_row(app: &mut App, ui: &mut Ui, guild: Id) {
    let palette = app.palette;
    let Some(g) = app.model.guild(guild) else {
        return;
    };
    let count = g.events.len();
    let first = g.events.iter().find(|e| e.live).or(g.events.first());
    let hint = first.map(|e| e.name.clone());
    let first = first.map(|e| e.id);
    let (slot, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let rect = Rect::from_min_max(
        pos2(slot.left() + 8.0, slot.top() + 1.0),
        pos2(slot.right() - 8.0, slot.bottom() - 1.0),
    );
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.hover);
    }
    let tint = if hovered || count > 0 {
        palette.text
    } else {
        palette.muted
    };
    paint_icon(ui, Icon::Calendar, pos2(rect.left() + 18.0, rect.center().y), 18.0, tint);
    let label = match count {
        0 => "Eventos".to_owned(),
        1 => "1 Evento".to_owned(),
        n => format!("{n} Eventos"),
    };
    ui.painter().text(
        pos2(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        font(Weight::Medium, 16.0),
        tint,
    );
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(
        (line.left() + 16.0)..=(line.right() - 16.0),
        line.center().y,
        Stroke::new(1.0, palette.selected),
    );
    let response = match hint {
        Some(name) => response.on_hover_text(name),
        None => response,
    };
    if response.clicked() {
        let url = match first {
            Some(event) => format!("https://discord.com/events/{guild}/{event}"),
            None => format!("https://discord.com/channels/{guild}"),
        };
        app.actions
            .push(Action::Chat(crate::chat::ChatAction::OpenUrl(url)));
    }
}

/// One channel; `movable` lets it be dragged. Returns where it was drawn.
fn channel_row(app: &mut App, ui: &mut Ui, id: Id, movable: bool) -> Rect {
    let palette = app.palette;
    let Some(channel) = app.model.channel(id) else {
        return Rect::NOTHING;
    };
    let selected = app.channel == Some(id);
    let unread = channel.unread();
    let sense = if movable {
        Sense::click_and_drag()
    } else {
        Sense::click()
    };
    let joined = app
        .call
        .as_ref()
        .filter(|c| c.channel == id)
        .map(|c| c.since);
    // A voice channel's status under its name; the user's own channel
    // offers to set one, as Discord.
    let status = match (&channel.voice_status, joined) {
        (Some(status), _) => Some((status.clone(), false)),
        (None, Some(_)) => Some(("Definir um status do canal".to_owned(), true)),
        (None, None) => None,
    };
    let height = if status.is_some() { 50.0 } else { 34.0 };
    let (slot, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), sense);
    if response.drag_started() {
        ui.data_mut(|d| d.insert_temp(egui::Id::new(CHANNEL_DRAG), id));
    }
    let rect = Rect::from_min_max(
        pos2(slot.left() + 8.0, slot.top() + 1.0),
        pos2(slot.right() - 8.0, slot.bottom() - 1.0),
    );
    // Still the row's while the pointer is on one of its buttons.
    let hovered = response.hovered() || response.contains_pointer();
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.selected);
    } else if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.hover);
    }
    if unread && !selected {
        let pill = Rect::from_center_size(pos2(slot.left(), slot.center().y), vec2(8.0, 8.0));
        ui.painter()
            .rect_filled(pill, CornerRadius::same(4), palette.strong);
    }
    // The voice channel the user is in: its speaker green and its name
    // lit, as Discord. With a status the name sits in the top line.
    let line = Rect::from_min_size(rect.min, vec2(rect.width(), 32.0));
    let tint = if selected || unread || joined.is_some() {
        palette.strong
    } else if hovered {
        palette.text
    } else {
        palette.muted
    };
    let icon = match channel.kind {
        ChannelKind::Voice => Icon::Speaker,
        ChannelKind::Other => Icon::Threads,
        _ => Icon::Hash,
    };
    paint_icon(
        ui,
        icon,
        pos2(line.left() + 18.0, line.center().y),
        20.0,
        if joined.is_some() {
            super::call::TALKING
        } else {
            palette.muted
        },
    );
    let weight = if unread || selected || joined.is_some() {
        Weight::SemiBold
    } else {
        Weight::Medium
    };
    let voice = channel.kind == ChannelKind::Voice;
    let buttons = hovered || selected;
    let reserve = if buttons {
        if voice { 72.0 } else { 52.0 }
    } else if channel.mentions > 0 || joined.is_some() {
        52.0
    } else {
        8.0
    };
    Elide::new(ui, line.right() - reserve).text(
        pos2(line.left() + 36.0, line.center().y),
        Align2::LEFT_CENTER,
        &channel.name,
        font(weight, 16.0),
        tint,
    );
    if let Some(since) = joined
        && !buttons
    {
        // How long the call has gone on, as Discord counts it; what the
        // user has been in it for until Discord says.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let seconds = channel
            .voice_start
            .map_or_else(|| since.elapsed().as_secs(), |start| now.saturating_sub(start));
        let time = if seconds >= 3600 {
            format!(
                "{}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            )
        } else {
            format!("{}:{:02}", seconds / 60, seconds % 60)
        };
        ui.painter().text(
            pos2(line.right() - 8.0, line.center().y),
            Align2::RIGHT_CENTER,
            time,
            egui::FontId::monospace(12.0),
            super::call::TALKING,
        );
    }
    if let Some((text, offer)) = status {
        let y = line.bottom() + 6.0;
        let left = line.left() + 36.0;
        let hit = Rect::from_min_max(pos2(left, y - 8.0), pos2(rect.right() - 8.0, y + 8.0));
        let edit = ui.interact(hit, ui.id().with(("voice-status", id)), Sense::click());
        let tint = if edit.hovered() {
            palette.text
        } else {
            palette.muted
        };
        let shown = Elide::new(ui, rect.right() - 28.0).text(
            pos2(left, y),
            Align2::LEFT_CENTER,
            text.clone(),
            font(Weight::Medium, 12.0),
            tint,
        );
        if offer || edit.hovered() {
            paint_icon(ui, Icon::Pencil, pos2(shown.right() + 9.0, y), 12.0, tint);
        }
        if joined.is_some()
            && edit
                .on_hover_text("Definir um status do canal")
                .clicked()
        {
            app.actions
                .push(Action::Admin(crate::admin::AdminAction::Open(
                    crate::admin::Modal::VoiceStatus {
                        channel: id,
                        text: if offer { String::new() } else { text },
                    },
                )));
        }
    }
    let Some(channel) = app.model.channel(id) else {
        return slot;
    };
    if channel.mentions > 0 && !buttons {
        badge(
            ui,
            &palette,
            pos2(line.right() - 8.0, line.center().y),
            channel.mentions,
            None,
        );
    } else if buttons {
        let guild = channel.guild;
        let mut x = line.right() - 12.0;
        let mut button = |icon: Icon, hint: &str| {
            let at = Rect::from_center_size(pos2(x, line.center().y), Vec2::splat(18.0));
            x -= 20.0;
            let response = ui.interact(at, ui.id().with((hint, id)), Sense::click());
            let tint = if response.hovered() {
                palette.strong
            } else {
                palette.icon
            };
            paint_icon(ui, icon, at.center(), 16.0, tint);
            response.on_hover_text(hint).clicked()
        };
        let settings = guild.is_some_and(|g| {
            app.model.can(g, crate::model::permission::MANAGE_GUILD)
        });
        // The gear only for who can change the server, as Discord.
        if settings
            && button(Icon::Settings, "Editar canal")
            && let Some(guild) = guild
        {
            app.actions
                .push(Action::Admin(crate::admin::AdminAction::OpenSettings(guild)));
        }
        if matches!(channel.kind, ChannelKind::Text | ChannelKind::Voice)
            && button(Icon::UserPlus, "Criar convite")
        {
            app.actions
                .push(Action::Admin(crate::admin::AdminAction::InviteTo(id)));
        }
        if voice && button(Icon::MessageCircle, "Abrir chat") {
            app.actions.push(Action::OpenChannel(id));
        }
    }
    let unread = channel.unread() || channel.mentions > 0;
    let guild = channel.guild;
    let textual = channel.kind.is_textual();
    if response.clicked() && textual {
        app.actions.push(Action::OpenChannel(id));
    }
    // A click on a voice channel joins it, as Discord.
    if response.clicked() && channel.kind == ChannelKind::Voice {
        app.actions.push(Action::JoinVoice(id));
    }
    let mut chosen = None;
    response.context_menu(|ui| {
        ui.set_min_width(180.0);
        if ui
            .add_enabled(unread, egui::Button::new("Marcar como lido"))
            .clicked()
        {
            chosen = Some(Action::MarkRead(id));
            ui.close();
        }
        if let Some(guild) = guild
            && ui.button("Copiar link").clicked()
        {
            chosen = Some(Action::Chat(crate::chat::ChatAction::Copy(format!(
                "https://discord.com/channels/{guild}/{id}"
            ))));
            ui.close();
        }
        ui.separator();
        if ui.button("Copiar ID do canal").clicked() {
            chosen = Some(Action::Chat(crate::chat::ChatAction::Copy(id.to_string())));
            ui.close();
        }
    });
    if let Some(action) = chosen {
        app.actions.push(action);
    }
    slot
}

fn direct_messages(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    ui.add_space(8.0);
    let (slot, response) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    let rect = Rect::from_min_max(
        pos2(slot.left() + 8.0, slot.top() + 1.0),
        pos2(slot.right() - 8.0, slot.bottom() - 1.0),
    );
    let selected = super::nav::is_friends(app);
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.selected);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.hover);
    }
    let lit = selected || response.hovered();
    paint_icon(
        ui,
        Icon::Users,
        pos2(rect.left() + 22.0, rect.center().y),
        22.0,
        if lit { palette.strong } else { palette.icon },
    );
    ui.painter().text(
        pos2(rect.left() + 48.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Amigos",
        font(Weight::Medium, 16.0),
        if lit { palette.strong } else { palette.muted },
    );
    // Requests waiting, as the red badge on Discord's friends row.
    let pending = app
        .model
        .relationships
        .values()
        .filter(|k| **k == crate::nav::relationship::INCOMING)
        .count() as u32;
    if pending > 0 {
        badge(
            ui,
            &palette,
            pos2(rect.right() - 8.0, rect.center().y),
            pending,
            None,
        );
    }
    if response.clicked() {
        app.actions
            .push(Action::Nav(crate::nav::NavAction::Friends));
    }

    ui.add_space(16.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 18.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Mensagens diretas",
        font(Weight::Medium, 14.0),
        palette.muted,
    );
    paint_icon(
        ui,
        Icon::Plus,
        pos2(rect.right() - 20.0, rect.center().y),
        16.0,
        palette.muted,
    );
    ui.add_space(4.0);

    let mut open = None;
    for id in app.model.sorted_direct() {
        let Some(channel) = app.model.channel(id) else {
            continue;
        };
        let selected = app.channel == Some(id);
        let (slot, response) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
        if !ui.is_rect_visible(slot) {
            continue;
        }
        let rect = Rect::from_min_max(
            pos2(slot.left() + 8.0, slot.top() + 1.0),
            pos2(slot.right() - 8.0, slot.bottom() - 1.0),
        );
        let hovered = response.hovered();
        let behind = if selected {
            palette.selected
        } else if hovered {
            palette.hover
        } else {
            palette.sidebar
        };
        if selected || hovered {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), behind);
        }
        let center = pos2(rect.left() + 24.0, rect.center().y);
        let (name, activity) = match channel.kind {
            ChannelKind::Direct => {
                let user = channel.recipients.first().and_then(|u| app.model.user(*u));
                if let Some(user) = user {
                    avatar(ui, app, user, center, 16.0, Some(behind));
                }
                (
                    user.map_or_else(|| "Desconhecido".into(), |u| u.name.clone()),
                    user.and_then(|u| u.activity.clone()),
                )
            }
            _ => {
                super::widgets::group_icon(ui, app, channel, center, 16.0);
                let members = channel.recipients.len() + 1;
                (
                    app.model.direct_name(channel),
                    Some(format!("{members} membros")),
                )
            }
        };
        let unread = channel.mentions > 0 || channel.unread();
        let tint = if selected || unread {
            palette.strong
        } else if hovered {
            palette.text
        } else {
            palette.muted
        };
        let clip = Rect::from_min_max(rect.min, pos2(rect.right() - 28.0, rect.bottom()));
        let painter = Elide::new(ui, clip.right());
        let name_y = if activity.is_some() {
            rect.center().y - 8.0
        } else {
            rect.center().y
        };
        painter.text(
            pos2(rect.left() + 50.0, name_y),
            Align2::LEFT_CENTER,
            name,
            font(Weight::Medium, 15.0),
            tint,
        );
        if let Some(activity) = activity {
            painter.text(
                pos2(rect.left() + 50.0, rect.center().y + 9.0),
                Align2::LEFT_CENTER,
                activity,
                font(Weight::Regular, 12.0),
                palette.muted,
            );
        }
        if channel.mentions > 0 {
            badge(
                ui,
                &palette,
                pos2(rect.right() - 8.0, rect.center().y),
                channel.mentions,
                None,
            );
        } else if hovered {
            paint_icon(
                ui,
                Icon::Close,
                pos2(rect.right() - 14.0, rect.center().y),
                16.0,
                palette.icon,
            );
        }
        if response.clicked() {
            open = Some(id);
        }
    }
    if let Some(id) = open {
        app.actions.push(Action::OpenChannel(id));
    }
}

/// Your avatar and name, with mute, deafen and settings.
/// The card at the foot of the rail and the sidebar, as Discord's: the
/// call above, when there is one, and the user with their buttons.
const CARD_MARGIN: f32 = 8.0;
const USER_ROW: f32 = 56.0;

/// How much the card covers at the foot of the rail and the sidebar.
pub fn card_space(app: &App) -> f32 {
    let call = if app.call.is_some() {
        super::call::SECTION + 1.0
    } else {
        0.0
    };
    USER_ROW + call + CARD_MARGIN * 2.0
}

/// Draws the card over the foot of the rail and the sidebar. `window` is
/// the whole window, for its bottom left corner.
pub fn card(app: &mut App, ui: &Ui, window: Rect) {
    let palette = app.palette;
    let height = card_space(app) - CARD_MARGIN * 2.0;
    let rect = Rect::from_min_size(
        pos2(
            window.left() + CARD_MARGIN,
            window.bottom() - CARD_MARGIN - height,
        ),
        vec2(RAIL_WIDTH + width(app) - CARD_MARGIN * 2.0, height),
    );
    egui::Area::new(egui::Id::new("user-card"))
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            ui.allocate_exact_size(rect.size(), Sense::hover());
            ui.painter().rect(
                rect,
                CornerRadius::same(8),
                palette.user_panel,
                Stroke::new(1.0, palette.divider),
                egui::StrokeKind::Inside,
            );
            let mut top = rect.top();
            if app.call.is_some() {
                let section =
                    Rect::from_min_size(rect.min, vec2(rect.width(), super::call::SECTION));
                super::call::section(app, ui, section);
                top = section.bottom();
                ui.painter().hline(
                    rect.x_range().shrink(1.0),
                    top + 0.5,
                    Stroke::new(1.0, palette.divider),
                );
                top += 1.0;
            }
            user_row(
                app,
                ui,
                Rect::from_min_max(pos2(rect.left(), top), rect.max),
            );
        });
}

/// An icon button with a chevron beside it that opens its menu, as
/// Discord's microphone and headphones. Red behind both while `off`.
fn split_button(
    ui: &mut Ui,
    palette: &crate::theme::Palette,
    at: Rect,
    icon: Icon,
    off: bool,
    hint: &str,
) -> (egui::Response, egui::Response) {
    let main = Rect::from_min_size(at.min, vec2(32.0, at.height()));
    let more = Rect::from_min_max(pos2(main.right(), at.top()), at.max);
    let main = ui.interact(main, ui.id().with((hint, "main")), Sense::click());
    let more = ui.interact(more, ui.id().with((hint, "more")), Sense::click());
    if off {
        ui.painter().rect_filled(
            at,
            CornerRadius::same(6),
            palette.danger.gamma_multiply(0.18),
        );
    }
    for part in [&main, &more] {
        if part.hovered() {
            ui.painter().rect_filled(
                part.rect,
                CornerRadius::same(6),
                if off {
                    palette.danger.gamma_multiply(0.18)
                } else {
                    palette.hover
                },
            );
        }
    }
    let tint = |r: &egui::Response| {
        if off {
            palette.danger
        } else if r.hovered() {
            palette.strong
        } else {
            palette.icon
        }
    };
    paint_icon(ui, icon, main.rect.center(), 20.0, tint(&main));
    paint_icon(ui, Icon::ChevronDown, more.rect.center(), 14.0, tint(&more));
    (main.on_hover_text(hint), more)
}

/// The menu on the microphone's or the headphones' arrow, as Discord's:
/// the device (another one to choose under it), the input profile for the
/// microphone, the volume, and the voice settings.
fn sound_menu(app: &mut App, more: &egui::Response, input: bool) {
    let palette = app.palette;
    let mut chosen: Vec<Action> = Vec::new();
    egui::Popup::menu(more)
        .align(egui::RectAlign::TOP_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(240.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            let devices = super::settings::devices(app, ui.ctx());
            let (list, default, current) = if input {
                (&devices.inputs, devices.default_input.as_deref(), app.settings.input_device.as_deref())
            } else {
                (&devices.outputs, devices.default_output.as_deref(), app.settings.output_device.as_deref())
            };
            let label = |name: Option<&str>| {
                name.and_then(|n| crate::devices::Devices::label(list, n))
                    .or_else(|| name.map(str::to_owned))
            };
            let shown = label(current)
                .or_else(|| label(default).map(|d| format!("Padrão do sistema: {d}")))
                .unwrap_or_else(|| "Padrão do sistema".to_owned());
            let title = if input { "Dispositivo de entrada" } else { "Dispositivo de saída" };
            submenu(ui, &palette, title, &shown, |ui| {
                ui.set_min_width(240.0);
                let default_label = label(default)
                    .map_or_else(|| "Padrão do sistema".to_owned(), |d| format!("Padrão do sistema: {d}"));
                if super::call::menu_item(ui, &palette, &default_label, palette.text).clicked() {
                    chosen.push(if input { Action::SetInputDevice(None) } else { Action::SetOutputDevice(None) });
                    ui.close();
                }
                for device in list {
                    let color = if current == Some(device.name.as_str()) { palette.strong } else { palette.text };
                    if super::call::menu_item(ui, &palette, &device.label, color).clicked() {
                        let name = Some(device.name.clone());
                        chosen.push(if input { Action::SetInputDevice(name) } else { Action::SetOutputDevice(name) });
                        ui.close();
                    }
                }
            });
            if input {
                let profile = app.settings.input_profile;
                let name = match profile {
                    0 => "Isolamento de Voz",
                    1 => "Estúdio",
                    _ => "Personalizado",
                };
                submenu(ui, &palette, "Perfil de entrada", name, |ui| {
                    ui.set_min_width(200.0);
                    for (value, label) in [(0, "Isolamento de Voz"), (1, "Estúdio"), (2, "Personalizado")] {
                        let color = if profile == value { palette.strong } else { palette.text };
                        if super::call::menu_item(ui, &palette, label, color).clicked() {
                            chosen.push(Action::SetInputProfile(value));
                            ui.close();
                        }
                    }
                });
            }
            ui.separator();
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(if input { "Volume de entrada" } else { "Volume de saída" })
                    .font(font(Weight::Medium, 15.0))
                    .color(palette.strong),
            );
            let volume = if input { app.settings.input_volume } else { app.settings.output_volume };
            if let Some(v) = super::settings::percent_slider(ui, &palette, volume, 228.0) {
                chosen.push(if input { Action::SetInputVolume(v) } else { Action::SetOutputVolume(v) });
            }
            ui.separator();
            let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(row, CornerRadius::same(4), palette.hover);
            }
            paint_icon(ui, Icon::Settings, pos2(row.left() + 14.0, row.center().y), 20.0, palette.icon);
            ui.painter().text(
                pos2(row.left() + 34.0, row.center().y),
                Align2::LEFT_CENTER,
                "Configurações de voz",
                font(Weight::Medium, 15.0),
                palette.strong,
            );
            if response.clicked() {
                chosen.push(Action::OpenSettings(crate::app::SettingsPage::Voice));
                ui.close();
            }
        });
    app.actions.extend(chosen);
}

/// A row with a title and its current value under it, and a chevron; the
/// choices open beside it.
fn submenu(
    ui: &mut Ui,
    palette: &crate::theme::Palette,
    title: &str,
    value: &str,
    add: impl FnOnce(&mut Ui),
) {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(row, CornerRadius::same(4), palette.hover);
    }
    ui.painter().text(
        pos2(row.left() + 6.0, row.top() + 13.0),
        Align2::LEFT_CENTER,
        title,
        font(Weight::Medium, 15.0),
        palette.strong,
    );
    Elide::new(ui, row.right() - 28.0).text(
        pos2(row.left() + 6.0, row.top() + 31.0),
        Align2::LEFT_CENTER,
        value,
        font(Weight::Regular, 12.0),
        palette.muted,
    );
    paint_icon(ui, Icon::ChevronRight, pos2(row.right() - 12.0, row.center().y), 16.0, palette.icon);
    egui::Popup::menu(&response)
        .align(egui::RectAlign::RIGHT_START)
        .show(add);
}

/// The user: picture, name and status (or "Em voz"), then microphone,
/// headphones and settings.
fn user_row(app: &mut App, ui: &mut Ui, rect: Rect) {
    let palette = app.palette;
    let rect = rect.shrink2(vec2(8.0, 0.0));
    let identity = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 8.0),
        pos2(rect.right() - 152.0, rect.bottom() - 8.0),
    );
    let response = ui.interact(identity, ui.id().with("me"), Sense::click());
    if response.clicked() {
        let me = app.model.me;
        app.actions
            .push(Action::Chat(crate::chat::ChatAction::ShowProfile {
                user: me,
                anchor: identity,
                side: crate::chat::Side::Above,
            }));
    }
    let behind = if response.hovered() {
        palette.selected
    } else {
        palette.user_panel
    };
    if response.hovered() {
        ui.painter()
            .rect_filled(identity, CornerRadius::same(6), behind);
    }
    let in_voice = app.call.is_some();
    if let Some(me) = app.model.user(app.model.me) {
        avatar(
            ui,
            app,
            me,
            pos2(identity.left() + 20.0, identity.center().y),
            16.0,
            Some(behind),
        );
        let painter = Elide::new(ui, identity.right() - 4.0);
        painter.text(
            pos2(identity.left() + 44.0, identity.center().y - 9.0),
            Align2::LEFT_CENTER,
            &me.name,
            font(Weight::SemiBold, 15.0),
            palette.strong,
        );
        let below = pos2(identity.left() + 44.0, identity.center().y + 9.0);
        if response.hovered() {
            painter.text(
                below,
                Align2::LEFT_CENTER,
                me.username.as_str(),
                font(Weight::Regular, 12.0),
                palette.muted,
            );
        } else if in_voice {
            paint_icon(
                ui,
                Icon::Speaker,
                below + vec2(6.0, 0.0),
                12.0,
                super::call::TALKING,
            );
            painter.text(
                below + vec2(15.0, 0.0),
                Align2::LEFT_CENTER,
                "Em voz",
                font(Weight::Regular, 12.0),
                super::call::TALKING,
            );
        } else {
            let status = match me.status {
                Status::Online => "Disponível",
                Status::Idle => "Ausente",
                Status::DoNotDisturb => "Não perturbar",
                Status::Offline => "Invisível",
            };
            painter.text(
                below,
                Align2::LEFT_CENTER,
                status,
                font(Weight::Regular, 12.0),
                palette.muted,
            );
        }
    }
    let (muted, deafened) = (app.settings.muted, app.settings.deafened);
    let y = rect.center().y;
    let mic_at = Rect::from_min_size(pos2(rect.right() - 148.0, y - 16.0), vec2(52.0, 32.0));
    let (mic, mic_more) = split_button(
        ui,
        &palette,
        mic_at,
        if muted { Icon::MicOff } else { Icon::Mic },
        muted,
        if muted {
            "Desativar silêncio"
        } else {
            "Silenciar"
        },
    );
    if mic.clicked() {
        app.actions.push(Action::ToggleMute);
    }
    let ears_at = Rect::from_min_size(pos2(mic_at.right() + 4.0, y - 16.0), vec2(52.0, 32.0));
    let (ears, ears_more) = split_button(
        ui,
        &palette,
        ears_at,
        if deafened {
            Icon::HeadphonesOff
        } else {
            Icon::Headphones
        },
        deafened,
        if deafened {
            "Desativar ensurdecer"
        } else {
            "Ensurdecer"
        },
    );
    if ears.clicked() {
        app.actions.push(Action::ToggleDeafen);
    }
    sound_menu(app, &mic_more, true);
    sound_menu(app, &ears_more, false);
    let gear = Rect::from_center_size(pos2(rect.right() - 18.0, y), Vec2::splat(32.0));
    let gear = ui.interact(gear, ui.id().with("settings"), Sense::click());
    if gear.hovered() {
        ui.painter()
            .rect_filled(gear.rect, CornerRadius::same(6), palette.hover);
    }
    paint_icon(
        ui,
        Icon::Settings,
        gear.rect.center(),
        20.0,
        if gear.hovered() {
            palette.strong
        } else {
            palette.icon
        },
    );
    if gear.on_hover_text("Configurações de usuário").clicked() {
        app.actions
            .push(Action::OpenSettings(crate::app::SettingsPage::Account));
    }
}

/// Discord's "Log Out" confirmation.
pub fn logout_dialog(app: &mut App, ui: &mut Ui) {
    if !app.confirm_logout {
        return;
    }
    let palette = app.palette;
    let modal = egui::Modal::new(egui::Id::new("logout"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(
            Frame::new()
                .fill(palette.chat)
                .corner_radius(CornerRadius::same(8)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(440.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.label(
                    egui::RichText::new("Sair")
                        .font(font(Weight::SemiBold, 20.0))
                        .color(palette.strong),
                );
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new("Tem certeza que deseja sair?")
                        .font(font(Weight::Regular, 16.0))
                        .color(palette.text),
                );
                ui.add_space(8.0);
            });
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
                        let (rect, out) = ui.allocate_exact_size(vec2(96.0, 38.0), Sense::click());
                        let fill = if out.hovered() {
                            Color32::from_rgb(0xa1, 0x28, 0x29)
                        } else {
                            palette.danger
                        };
                        ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Sair",
                            font(Weight::Medium, 14.0),
                            Color32::WHITE,
                        );
                        if out.clicked() {
                            app.actions.push(Action::Logout);
                        }
                        let (rect, cancel) =
                            ui.allocate_exact_size(vec2(96.0, 38.0), Sense::click());
                        let text = ui.painter().layout_no_wrap(
                            "Cancelar".into(),
                            font(Weight::Medium, 14.0),
                            palette.strong,
                        );
                        let width = text.size().x;
                        ui.painter().galley(
                            rect.center() - text.size() / 2.0,
                            text,
                            palette.strong,
                        );
                        if cancel.hovered() {
                            ui.painter().hline(
                                (rect.center().x - width / 2.0)..=(rect.center().x + width / 2.0),
                                rect.center().y + 9.0,
                                Stroke::new(1.0, palette.strong),
                            );
                        }
                        if cancel.clicked() {
                            app.actions.push(Action::CancelLogout);
                        }
                    });
                });
        });
    if modal.should_close() {
        app.actions.push(Action::CancelLogout);
    }
}
