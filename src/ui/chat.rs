//! The open channel: its header, the messages, the message field and the
//! member list.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, Sense, Stroke, Ui,
    UiBuilder, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{Elide, avatar, day, font, icon_button, long_date, paint_icon};
use super::{composer, message};
use crate::app::{Action, App, Place};
use crate::icons::Icon;
use crate::model::{ChannelKind, Id, ListItem, Message, Status};
use crate::theme::{HEADER_HEIGHT, MEMBERS_WIDTH, Palette};

/// Messages from one author this close together share one header.
const GROUP_SECONDS: i64 = 7 * 60;
/// The narrowest the messages get before the member list hides.
const MEMBERS_MIN_CHAT: f32 = 640.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::CentralPanel::default()
        .frame(Frame::new().fill(palette.chat).inner_margin(Margin::ZERO))
        .show(ui, |ui| {
            let Some(channel) = app.channel.filter(|c| app.model.channels.contains_key(c)) else {
                empty(app, ui);
                return;
            };
            header(app, ui, channel);
            // Like Discord, a narrow window gives the room to the messages.
            if app.show_members
                && app.server().is_some()
                && ui.available_width() >= MEMBERS_MIN_CHAT + MEMBERS_WIDTH
            {
                members(app, ui);
            }
            let field = composer::show(app, ui, channel);
            egui::CentralPanel::default()
                .frame(Frame::new().inner_margin(Margin::ZERO))
                .show(ui, |ui| messages(app, ui, channel));
            composer::picker(app, ui, field);
        });
    composer::dialogs(app, ui);
    composer::viewer(app, ui);
    composer::drop_zone(app, ui);
}

fn empty(app: &App, ui: &mut Ui) {
    let rect = ui.max_rect();
    let text = if app.place == Place::Home {
        "Nenhuma conversa aberta"
    } else {
        "Nenhum canal de texto que você possa ver"
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font(Weight::Medium, 16.0),
        app.palette.muted,
    );
}

fn header(app: &mut App, ui: &mut Ui, id: Id) {
    let palette = app.palette;
    egui::Panel::top("chat-header")
        .resizable(false)
        .exact_size(HEADER_HEIGHT)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin::symmetric(8, 0)))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let full = rect.expand2(vec2(8.0, 0.0));
            ui.painter().hline(
                full.x_range(),
                full.bottom() - 0.5,
                Stroke::new(1.0, palette.divider),
            );
            let Some(channel) = app.model.channel(id) else {
                return;
            };
            let mut x = rect.left() + 8.0;
            let y = rect.center().y;
            let title = match channel.kind {
                ChannelKind::Direct => {
                    if let Some(user) = channel.recipients.first().and_then(|u| app.model.user(*u))
                    {
                        avatar(ui, app, user, pos2(x + 12.0, y), 12.0, Some(palette.chat));
                    }
                    x += 32.0;
                    app.model.direct_name(channel)
                }
                ChannelKind::Group => {
                    paint_icon(ui, Icon::Users, pos2(x + 12.0, y), 22.0, palette.muted);
                    x += 32.0;
                    app.model.direct_name(channel)
                }
                _ => {
                    paint_icon(ui, Icon::Hash, pos2(x + 12.0, y), 24.0, palette.muted);
                    x += 30.0;
                    channel.name.clone()
                }
            };
            let buttons_left = rect.right() - 420.0;
            let title =
                ui.painter()
                    .layout_no_wrap(title, font(Weight::SemiBold, 16.0), palette.strong);
            let width = title.size().x.min((buttons_left - x - 8.0).max(0.0));
            ui.painter()
                .with_clip_rect(Rect::from_min_max(
                    pos2(x, rect.top()),
                    pos2(x + width, rect.bottom()),
                ))
                .galley(pos2(x, y - title.size().y / 2.0), title, palette.strong);
            x += width;
            if let Some(topic) = channel
                .topic
                .as_ref()
                .map(|t| t.lines().next().unwrap_or_default().to_owned())
                && x + 40.0 < buttons_left
            {
                x += 12.0;
                ui.painter().vline(
                    x,
                    (y - 12.0)..=(y + 12.0),
                    Stroke::new(1.0, palette.selected),
                );
                x += 12.0;
                let mut job = LayoutJob::single_section(
                    topic,
                    TextFormat {
                        font_id: font(Weight::Medium, 14.0),
                        color: palette.muted,
                        ..Default::default()
                    },
                );
                job.wrap.max_width = buttons_left - 8.0 - x;
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
                let galley = ui.painter().layout_job(job);
                ui.painter()
                    .galley(pos2(x, y - galley.size().y / 2.0), galley, palette.muted);
            }

            let right = Rect::from_min_max(pos2(buttons_left, rect.top()), rect.right_bottom());
            let show_members = app.show_members;
            let in_server = app.server().is_some();
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(right)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                    icon_button(ui, &palette, Icon::Help, 24.0, false, "Ajuda");
                    icon_button(ui, &palette, Icon::Inbox, 24.0, false, "Caixa de entrada");
                    search(ui, &palette);
                    if in_server {
                        let hint = if show_members {
                            "Ocultar lista de membros"
                        } else {
                            "Mostrar lista de membros"
                        };
                        if icon_button(ui, &palette, Icon::Users, 24.0, show_members, hint)
                            .clicked()
                        {
                            app.actions.push(Action::ToggleMembers);
                        }
                        icon_button(ui, &palette, Icon::Pin, 24.0, false, "Mensagens fixadas");
                        icon_button(
                            ui,
                            &palette,
                            Icon::Bell,
                            24.0,
                            false,
                            "Configurações de notificação",
                        );
                        icon_button(ui, &palette, Icon::Threads, 24.0, false, "Tópicos");
                    } else {
                        icon_button(
                            ui,
                            &palette,
                            Icon::User,
                            24.0,
                            false,
                            "Mostrar perfil do usuário",
                        );
                        icon_button(
                            ui,
                            &palette,
                            Icon::UserPlus,
                            24.0,
                            false,
                            "Adicionar amigos à DM",
                        );
                        icon_button(ui, &palette, Icon::Pin, 24.0, false, "Mensagens fixadas");
                    }
                },
            );
        });
}

/// The search field in the header. Searching arrives later.
fn search(ui: &mut Ui, palette: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(144.0, 24.0), Sense::click());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), palette.rail);
    ui.painter().text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Buscar",
        font(Weight::Medium, 14.0),
        palette.muted,
    );
    paint_icon(
        ui,
        Icon::Search,
        pos2(rect.right() - 12.0, rect.center().y),
        16.0,
        palette.muted,
    );
}

fn messages(app: &mut App, ui: &mut Ui, id: Id) {
    let palette = app.palette;
    let Some(channel) = app.model.channel(id) else {
        return;
    };
    let guild = channel.guild;
    let mut out = Vec::new();
    // Older history put in front moves everything down: keep the view on
    // the same messages by scrolling the same distance.
    let anchor_id = egui::Id::new(("scroll-anchor", id));
    let anchor: Option<(Id, f32)> = ui.data(|d| d.get_temp(anchor_id));
    let first = channel.messages.first().map(|m| m.id);
    let mut area = egui::ScrollArea::vertical()
        .id_salt(("messages", id))
        .stick_to_bottom(true)
        .auto_shrink([false, false]);
    let shift = ui.data(|d| d.get_temp::<f32>(egui::Id::new(("scroll-shift", id))));
    if let Some(offset) = shift {
        area = area.vertical_scroll_offset(offset);
        ui.data_mut(|d| d.remove::<f32>(egui::Id::new(("scroll-shift", id))));
    }
    let output = area.show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let width = ui.available_width();
        if channel.more && channel.messages.len() >= 50 {
            ui.add_space(24.0);
            let (rect, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
            if channel.loading {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Carregando mensagens…",
                    font(Weight::Medium, 14.0),
                    palette.muted,
                );
            }
            ui.add_space(8.0);
        } else {
            welcome(app, ui, id, width);
        }
        if channel.requested && channel.messages.is_empty() && channel.last_message.is_some() {
            ui.add_space(24.0);
            let (rect, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Carregando mensagens…",
                font(Weight::Medium, 14.0),
                palette.muted,
            );
        }
        let mut row = message::Row {
            app,
            guild,
            channel: id,
            out: &mut out,
        };
        let mut previous: Option<&Message> = None;
        for m in &channel.messages {
            let new_day = previous.is_none_or(|p| day(p.time) != day(m.time));
            if new_day {
                divider(ui, &palette, width, &long_date(day(m.time)));
            }
            let continues = !new_day
                && m.reply.is_none()
                && previous.is_some_and(|p| {
                    p.author == m.author
                        && p.kind == m.kind
                        && m.time.as_second() - p.time.as_second() <= GROUP_SECONDS
                });
            if !continues {
                ui.add_space(16.0);
            }
            message::show(&mut row, ui, m, continues);
            previous = Some(m);
        }
        ui.add_space(8.0);
    });
    let offset = output.state.offset.y;
    let height = output.content_size.y;
    if let (Some((old_first, old_height)), Some(first)) = (anchor, first)
        && old_first != first
        && height > old_height
    {
        ui.data_mut(|d| {
            d.insert_temp(
                egui::Id::new(("scroll-shift", id)),
                offset + height - old_height,
            )
        });
    }
    if let Some(first) = first {
        ui.data_mut(|d| d.insert_temp(anchor_id, (first, height)));
    }
    if offset < 400.0 && channel.more && !channel.loading && channel.messages.len() >= 50 {
        out.push(Action::Chat(crate::chat::ChatAction::LoadOlder(id)));
    }
    app.actions.extend(out);
    // The inline editor's text, written back now that nothing borrows it.
    if let Some(text) = ui.data_mut(|d| d.remove_temp::<String>(egui::Id::new("edit-message-text")))
        && let Some(editing) = &mut app.chat.editing
    {
        editing.text = text;
        editing.focus = false;
    }
}

/// The start of a channel: its big icon and a welcome.
fn welcome(app: &App, ui: &mut Ui, id: Id, width: f32) {
    let palette = app.palette;
    let Some(channel) = app.model.channel(id) else {
        return;
    };
    let free = ui.available_height() - 400.0;
    if free > 0.0 {
        ui.add_space(free);
    }
    let (rect, _) = ui.allocate_exact_size(vec2(width, 150.0), Sense::hover());
    let left = rect.left() + 16.0;
    let (title, line) = match channel.kind {
        ChannelKind::Direct | ChannelKind::Group => {
            let name = app.model.direct_name(channel);
            let center = pos2(left + 40.0, rect.top() + 40.0);
            match channel.recipients.first().and_then(|u| app.model.user(*u)) {
                Some(user) if channel.kind == ChannelKind::Direct => {
                    avatar(ui, app, user, center, 40.0, None)
                }
                _ => {
                    ui.painter().circle_filled(center, 40.0, palette.accent);
                    paint_icon(ui, Icon::Users, center, 40.0, Color32::WHITE);
                }
            }
            let line = format!("Este é o começo do seu histórico de mensagens diretas com {name}.");
            (name, line)
        }
        _ => {
            let center = pos2(left + 34.0, rect.top() + 34.0);
            ui.painter().circle_filled(center, 34.0, palette.selected);
            paint_icon(ui, Icon::Hash, center, 42.0, palette.strong);
            (
                format!("Boas\u{2010}vindas a #{}!", channel.name),
                format!("Este é o começo do canal #{}.", channel.name),
            )
        }
    };
    let painter = ui.painter().with_clip_rect(rect);
    painter.text(
        pos2(left, rect.top() + 96.0),
        Align2::LEFT_CENTER,
        title,
        font(Weight::Bold, 30.0),
        palette.strong,
    );
    painter.text(
        pos2(left, rect.top() + 130.0),
        Align2::LEFT_CENTER,
        line,
        font(Weight::Regular, 16.0),
        palette.muted,
    );
}

fn divider(ui: &mut Ui, palette: &Palette, width: f32, label: &str) {
    ui.add_space(12.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 12.0), Sense::hover());
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        font(Weight::SemiBold, 12.0),
        palette.muted,
    );
    let mid = rect.center();
    let half = galley.size().x / 2.0 + 6.0;
    let stroke = Stroke::new(1.0, palette.selected);
    ui.painter()
        .hline((rect.left() + 16.0)..=(mid.x - half), mid.y, stroke);
    ui.painter()
        .hline((mid.x + half)..=(rect.right() - 16.0), mid.y, stroke);
    ui.painter()
        .galley(mid - galley.size() / 2.0, galley, palette.muted);
}

fn members(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let Place::Server(id) = app.place else { return };
    egui::Panel::right("members")
        .resizable(false)
        .exact_size(MEMBERS_WIDTH)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.sidebar)
                .inner_margin(Margin::ZERO),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("members", id))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let Some(guild) = app.model.guild(id) else {
                        return;
                    };
                    ui.add_space(8.0);
                    if guild.list.is_empty() {
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(MEMBERS_WIDTH, 40.0), Sense::hover());
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Carregando membros…",
                            font(Weight::Medium, 13.0),
                            palette.muted,
                        );
                        return;
                    }
                    for item in &guild.list {
                        match item {
                            ListItem::Group { id: group, count } => {
                                let name = match group.as_str() {
                                    "online" => "Online".to_owned(),
                                    "offline" => "Offline".to_owned(),
                                    role => role
                                        .parse::<Id>()
                                        .ok()
                                        .and_then(|r| guild.roles.iter().find(|x| x.id == r))
                                        .map_or_else(|| role.to_owned(), |r| r.name.clone()),
                                };
                                ui.add_space(16.0);
                                let (rect, _) = ui
                                    .allocate_exact_size(vec2(MEMBERS_WIDTH, 20.0), Sense::hover());
                                ui.painter().text(
                                    pos2(rect.left() + 16.0, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    format!("{} — {}", name.to_uppercase(), count),
                                    font(Weight::SemiBold, 12.0),
                                    palette.muted,
                                );
                            }
                            ListItem::Member(user) => member(app, ui, id, *user),
                        }
                    }
                });
        });
}

fn member(app: &App, ui: &mut Ui, guild: Id, id: Id) {
    let palette = app.palette;
    let Some(user) = app.model.user(id) else {
        return;
    };
    let (slot, response) = ui.allocate_exact_size(vec2(MEMBERS_WIDTH, 44.0), Sense::click());
    if !ui.is_rect_visible(slot) {
        return;
    }
    let rect = Rect::from_min_max(
        pos2(slot.left() + 8.0, slot.top() + 1.0),
        pos2(slot.right() - 8.0, slot.bottom() - 1.0),
    );
    let hovered = response.hovered();
    let behind = if hovered {
        palette.hover
    } else {
        palette.sidebar
    };
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), behind);
    }
    let offline = user.status == Status::Offline && !hovered;
    let fade = |c: Color32| if offline { c.gamma_multiply(0.3) } else { c };
    ui.scope(|ui| {
        ui.set_opacity(if offline { 0.3 } else { 1.0 });
        avatar(
            ui,
            app,
            user,
            pos2(rect.left() + 24.0, rect.center().y),
            16.0,
            (!offline).then_some(behind),
        );
    });
    let color = app
        .model
        .name_color(Some(guild), id)
        .unwrap_or(palette.text);
    let painter = Elide::new(ui, rect.right() - 4.0);
    let name_y = if user.activity.is_some() && !offline {
        rect.center().y - 8.0
    } else {
        rect.center().y
    };
    painter.text(
        pos2(rect.left() + 50.0, name_y),
        Align2::LEFT_CENTER,
        app.model.name_in(Some(guild), id),
        font(Weight::Medium, 15.0),
        fade(color),
    );
    if let Some(activity) = user.activity.as_ref().filter(|_| !offline) {
        painter.text(
            pos2(rect.left() + 50.0, rect.center().y + 9.0),
            Align2::LEFT_CENTER,
            activity,
            font(Weight::Regular, 12.0),
            palette.muted,
        );
    }
}
