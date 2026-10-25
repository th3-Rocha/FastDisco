//! The column beside the rail: the server's channels or the direct
//! messages, and the user panel at its foot.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{Elide, avatar, badge, font, icon_button, paint_icon};
use crate::app::{Action, App, Place};
use crate::icons::Icon;
use crate::model::{ChannelKind, Id, Status};
use crate::theme::{HEADER_HEIGHT, SIDEBAR_WIDTH, USER_PANEL_HEIGHT};

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::Panel::left("sidebar")
        .resizable(false)
        .exact_size(SIDEBAR_WIDTH)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.sidebar)
                .inner_margin(Margin::ZERO),
        )
        .show(ui, |ui| {
            user_panel(app, ui);
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
        });
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
                        ui.painter()
                            .rect_filled(rect, CornerRadius::ZERO, palette.hover);
                    }
                    let name = app
                        .model
                        .guild(id)
                        .map(|g| g.name.clone())
                        .unwrap_or_default();
                    let clip =
                        Rect::from_min_max(rect.min, pos2(rect.right() - 36.0, rect.bottom()));
                    Elide::new(ui, clip.right()).text(
                        pos2(rect.left() + 16.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        name,
                        font(Weight::SemiBold, 15.0),
                        palette.strong,
                    );
                    // An X while the menu is open, as Discord.
                    paint_icon(
                        ui,
                        if open { Icon::Close } else { Icon::ChevronDown },
                        pos2(rect.right() - 22.0, rect.center().y),
                        18.0,
                        palette.strong,
                    );
                    super::admin::menu(app, ui, rect);
                }
                Place::Home => {
                    let field =
                        Rect::from_center_size(rect.center(), vec2(rect.width() - 20.0, 28.0));
                    ui.painter()
                        .rect_filled(field, CornerRadius::same(4), palette.rail);
                    ui.painter().text(
                        pos2(field.left() + 8.0, field.center().y),
                        Align2::LEFT_CENTER,
                        "Encontre ou comece uma conversa",
                        font(Weight::Medium, 13.0),
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
    for id in loose {
        let rect = channel_row(app, ui, id, movable);
        rows.push((rect, None, Some(id), false));
    }
    for (category, list) in categories {
        ui.add_space(16.0);
        let collapsed = app.collapsed.contains(&category);
        let name = app
            .model
            .channel(category)
            .map(|c| c.name.to_uppercase())
            .unwrap_or_default();
        let (rect, response) = ui.allocate_exact_size(vec2(SIDEBAR_WIDTH, 24.0), Sense::click());
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
        paint_icon(
            ui,
            chevron,
            pos2(rect.left() + 10.0, rect.center().y),
            12.0,
            tint,
        );
        let clip = Rect::from_min_max(rect.min, pos2(rect.right() - 32.0, rect.bottom()));
        Elide::new(ui, clip.right()).text(
            pos2(rect.left() + 18.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            font(Weight::SemiBold, 12.0),
            tint,
        );
        paint_icon(
            ui,
            Icon::Plus,
            pos2(rect.right() - 18.0, rect.center().y),
            16.0,
            palette.muted,
        );
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
                pos2(left + SIDEBAR_WIDTH - 16.0, y + 1.0),
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
    let (slot, response) = ui.allocate_exact_size(vec2(SIDEBAR_WIDTH, 34.0), sense);
    if response.drag_started() {
        ui.data_mut(|d| d.insert_temp(egui::Id::new(CHANNEL_DRAG), id));
    }
    let rect = Rect::from_min_max(
        pos2(slot.left() + 8.0, slot.top() + 1.0),
        pos2(slot.right() - 8.0, slot.bottom() - 1.0),
    );
    let hovered = response.hovered();
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
    let tint = if selected || unread {
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
        pos2(rect.left() + 18.0, rect.center().y),
        20.0,
        palette.muted,
    );
    let weight = if unread || selected {
        Weight::SemiBold
    } else {
        Weight::Medium
    };
    let reserve = if channel.mentions > 0 || hovered || selected {
        52.0
    } else {
        8.0
    };
    let clip = Rect::from_min_max(rect.min, pos2(rect.right() - reserve, rect.bottom()));
    Elide::new(ui, clip.right()).text(
        pos2(rect.left() + 36.0, rect.center().y),
        Align2::LEFT_CENTER,
        &channel.name,
        font(weight, 15.0),
        tint,
    );
    if channel.mentions > 0 {
        badge(
            ui,
            &palette,
            pos2(rect.right() - 8.0, rect.center().y),
            channel.mentions,
            None,
        );
    } else if hovered || selected {
        if channel.kind == ChannelKind::Text {
            paint_icon(
                ui,
                Icon::UserPlus,
                pos2(rect.right() - 32.0, rect.center().y),
                16.0,
                palette.icon,
            );
        }
        paint_icon(
            ui,
            Icon::Settings,
            pos2(rect.right() - 12.0, rect.center().y),
            16.0,
            palette.icon,
        );
    }
    let unread = channel.unread() || channel.mentions > 0;
    let guild = channel.guild;
    let textual = channel.kind.is_textual();
    if response.clicked() && textual {
        app.actions.push(Action::OpenChannel(id));
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
    let (slot, response) = ui.allocate_exact_size(vec2(SIDEBAR_WIDTH, 44.0), Sense::click());
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
    let (rect, _) = ui.allocate_exact_size(vec2(SIDEBAR_WIDTH, 24.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 18.0, rect.center().y),
        Align2::LEFT_CENTER,
        "MENSAGENS DIRETAS",
        font(Weight::SemiBold, 12.0),
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
        let (slot, response) = ui.allocate_exact_size(vec2(SIDEBAR_WIDTH, 44.0), Sense::click());
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
                ui.painter().circle_filled(center, 16.0, palette.accent);
                paint_icon(ui, Icon::Users, center, 18.0, Color32::WHITE);
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
fn user_panel(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::Panel::bottom("user-panel")
        .resizable(false)
        .exact_size(USER_PANEL_HEIGHT)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.user_panel)
                .inner_margin(Margin::symmetric(8, 0)),
        )
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let identity = Rect::from_min_max(
                pos2(rect.left() - 2.0, rect.top() + 8.0),
                pos2(rect.right() - 102.0, rect.bottom() - 8.0),
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
                    .rect_filled(identity, CornerRadius::same(4), behind);
            }
            if let Some(me) = app.model.user(app.model.me) {
                avatar(
                    ui,
                    app,
                    me,
                    pos2(identity.left() + 18.0, identity.center().y),
                    16.0,
                    Some(behind),
                );
                let clip = Rect::from_min_max(identity.min, identity.max);
                let painter = Elide::new(ui, clip.right());
                painter.text(
                    pos2(identity.left() + 42.0, identity.center().y - 8.0),
                    Align2::LEFT_CENTER,
                    &me.name,
                    font(Weight::SemiBold, 14.0),
                    palette.strong,
                );
                let status = match me.status {
                    Status::Online => "Disponível",
                    Status::Idle => "Ausente",
                    Status::DoNotDisturb => "Não perturbar",
                    Status::Offline => "Invisível",
                };
                painter.text(
                    pos2(identity.left() + 42.0, identity.center().y + 9.0),
                    Align2::LEFT_CENTER,
                    if response.hovered() {
                        me.username.as_str()
                    } else {
                        status
                    },
                    font(Weight::Regular, 12.0),
                    palette.muted,
                );
            }
            let buttons =
                Rect::from_min_max(pos2(rect.right() - 98.0, rect.top()), rect.right_bottom());
            let (muted, deafened) = (app.settings.muted, app.settings.deafened);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(buttons)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(1.0, 0.0);
                    let mic = icon_button(
                        ui,
                        &palette,
                        Icon::Mic,
                        32.0,
                        false,
                        if muted {
                            "Desativar silêncio"
                        } else {
                            "Silenciar"
                        },
                    );
                    if muted {
                        strike(ui, mic.rect, palette.dnd);
                    }
                    if mic.clicked() {
                        app.actions.push(Action::ToggleMute);
                    }
                    let ears = icon_button(
                        ui,
                        &palette,
                        Icon::Headphones,
                        32.0,
                        false,
                        if deafened {
                            "Desativar ensurdecer"
                        } else {
                            "Ensurdecer"
                        },
                    );
                    if deafened {
                        strike(ui, ears.rect, palette.dnd);
                    }
                    if ears.clicked() {
                        app.actions.push(Action::ToggleDeafen);
                    }
                    let settings = icon_button(
                        ui,
                        &palette,
                        Icon::Settings,
                        32.0,
                        false,
                        "Configurações de usuário",
                    );
                    if settings.clicked() {
                        app.actions
                            .push(Action::OpenSettings(crate::app::SettingsPage::Account));
                    }
                },
            );
        });
}

/// The red slash over a muted microphone or deafened headphones.
fn strike(ui: &Ui, rect: Rect, color: Color32) {
    let c = rect.center();
    ui.painter().line_segment(
        [c + vec2(-9.0, -9.0), c + vec2(9.0, 9.0)],
        Stroke::new(2.0, color),
    );
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
