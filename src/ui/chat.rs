//! The open channel: its header, the messages, the message field and the
//! member list.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, Sense, Stroke, Ui,
    UiBuilder, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{Elide, avatar, day, font, icon_button, long_date, paint_icon};
use super::{composer, message, profile};
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
        .frame(Frame::new().fill(crate::theme::region(palette.chat, crate::theme::Region::Chat)).inner_margin(Margin::ZERO))
        .show(ui, |ui| {
            if app.call_view && app.call.is_some() {
                super::call::view(app, ui);
                return;
            }
            if super::nav::is_friends(app) {
                super::friends::show(app, ui);
                return;
            }
            let Some(channel) = app.channel.filter(|c| app.model.channels.contains_key(c)) else {
                empty(app, ui);
                return;
            };
            header(app, ui, channel);
            // Search results take the member list's place, as in Discord.
            if app.nav.search.is_some() {
                super::nav::search_panel(app, ui);
            } else if app.settings.show_members
                && app.server().is_some()
                && ui.available_width() >= MEMBERS_MIN_CHAT + MEMBERS_WIDTH
            {
                members(app, ui);
            }
            let direct = app
                .model
                .channel(channel)
                .filter(|c| c.kind == ChannelKind::Direct)
                .and_then(|c| c.recipients.first().copied());
            if let Some(user) = direct
                && app.nav.search.is_none()
                && app.settings.dm_profile
                && ui.available_width() >= MEMBERS_MIN_CHAT + profile::PANEL_WIDTH
            {
                app.load_profile(user);
                profile::panel(app, ui, user);
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
                    super::widgets::group_icon(ui, app, channel, pos2(x + 12.0, y), 12.0);
                    x += 32.0;
                    app.model.direct_name(channel)
                }
                ChannelKind::Voice => {
                    paint_icon(ui, Icon::Speaker, pos2(x + 12.0, y), 22.0, palette.muted);
                    x += 30.0;
                    channel.name.clone()
                }
                _ => {
                    paint_icon(ui, Icon::Hash, pos2(x + 12.0, y), 24.0, palette.muted);
                    x += 30.0;
                    channel.name.clone()
                }
            };
            // Room for the buttons at the right: the search field (wider
            // while in use) and three or four icons.
            let searching = !app.nav.search_text.is_empty()
                || ui.memory(|m| m.has_focus(egui::Id::new("header-search")));
            let buttons = if app.server().is_some() { 170.0 } else { 130.0 }
                + if searching { 250.0 } else { 155.0 };
            let buttons_left = rect.right() - buttons;
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
            let show_members = app.settings.show_members;
            let in_server = app.server().is_some();
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(right)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                    super::nav::search_field(app, ui);
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
                        super::nav::popout_button(app, ui, crate::nav::Popout::Pins);
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
                        let shown = app.settings.dm_profile;
                        let hint = if shown {
                            "Ocultar perfil do usuário"
                        } else {
                            "Mostrar perfil do usuário"
                        };
                        if icon_button(ui, &palette, Icon::User, 24.0, shown, hint).clicked() {
                            app.actions
                                .push(Action::Chat(crate::chat::ChatAction::ToggleDmProfile));
                        }
                        icon_button(
                            ui,
                            &palette,
                            Icon::UserPlus,
                            24.0,
                            false,
                            "Adicionar amigos à DM",
                        );
                        super::nav::popout_button(app, ui, crate::nav::Popout::Pins);
                    }
                },
            );
        });
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
    let detached = app.nav.detached.contains(&id);
    let mut jump_scrolled = false;
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
        if channel.age_gated {
            age_gate(&palette, &mut out, ui, width);
        } else if channel.requested && channel.messages.is_empty() && channel.last_message.is_some()
        {
            skeleton(ui, &palette, width);
        }
        let mut row = message::Row {
            app,
            guild,
            channel: id,
            out: &mut out,
            found: false,
        };
        let unread_from = app.chat.unread_from.get(&id).copied();
        let jump = app.nav.jump.filter(|j| j.channel == id);
        let mut new_shown = false;
        let mut previous: Option<&Message> = None;
        for m in &channel.messages {
            if !new_shown
                && unread_from
                    .is_some_and(|read| m.id > read && !m.pending && m.author != app.model.me)
            {
                new_shown = true;
                new_divider(ui, &palette, width);
            }
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
            let top = ui.cursor().top();
            message::show(&mut row, ui, m, continues);
            if let Some(jump) = jump.filter(|j| j.message == m.id) {
                let rect = Rect::from_min_max(
                    pos2(ui.max_rect().left(), top),
                    pos2(ui.max_rect().right(), ui.cursor().top()),
                );
                if !jump.scrolled {
                    ui.scroll_to_rect(rect, Some(Align::Center));
                    jump_scrolled = true;
                }
                // The jumped-to message lights up, then fades.
                let age = jump.at.elapsed().as_secs_f32();
                let fade = (1.0 - (age - 1.0).max(0.0) / 1.5).clamp(0.0, 1.0);
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::ZERO,
                    palette.accent.gamma_multiply(0.18 * fade),
                );
                ui.ctx().request_repaint();
            }
            previous = Some(m);
        }
        ui.add_space(8.0);
    });
    let offset = output.state.offset.y;
    let height = output.content_size.y;
    let viewport = output.inner_rect;
    // Far from the newest messages, or showing a jump into the past:
    // Discord's bar to jump back.
    if detached || height - offset - viewport.height() > viewport.height() * 1.5 {
        let bar = Rect::from_min_max(
            pos2(viewport.left() + 16.0, viewport.bottom() - 32.0),
            pos2(viewport.right() - 16.0, viewport.bottom()),
        );
        ui.painter().rect_filled(
            bar,
            CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 0,
            },
            palette.sidebar,
        );
        ui.painter().text(
            pos2(bar.left() + 12.0, bar.center().y),
            Align2::LEFT_CENTER,
            "Você está vendo mensagens antigas",
            font(Weight::Medium, 14.0),
            palette.text,
        );
        let label = ui.painter().layout_no_wrap(
            "Ir para o presente".into(),
            font(Weight::SemiBold, 14.0),
            palette.strong,
        );
        let jump = Rect::from_min_size(
            pos2(
                bar.right() - 12.0 - label.size().x,
                bar.center().y - label.size().y / 2.0,
            ),
            label.size(),
        );
        let response = ui.interact(jump.expand(4.0), ui.id().with(("jump", id)), Sense::click());
        if response.hovered() {
            ui.painter().hline(
                jump.x_range(),
                jump.bottom(),
                Stroke::new(1.0, palette.strong),
            );
        }
        ui.painter().galley(jump.min, label, palette.strong);
        if response.clicked() {
            if detached {
                out.push(Action::Nav(crate::nav::NavAction::JumpToPresent(id)));
            }
            ui.data_mut(|d| d.insert_temp(egui::Id::new(("scroll-shift", id)), height));
        }
    }
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
    if let Some(jump) = &mut app.nav.jump {
        jump.scrolled |= jump_scrolled;
        if jump.at.elapsed().as_secs_f32() > 2.5 && jump.scrolled {
            app.nav.jump = None;
        }
    }
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

/// Grey rows shaped like messages while the history loads.
/// Discord's notice for an age-restricted channel the account may not
/// see yet, in place of the messages.
fn age_gate(palette: &Palette, out: &mut Vec<Action>, ui: &mut Ui, width: f32) {
    ui.add_space(32.0);
    ui.vertical_centered(|ui| {
        ui.set_max_width(width.min(440.0));
        ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
        let (rect, _) = ui.allocate_exact_size(vec2(80.0, 80.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 40.0, palette.hover);
        super::widgets::paint_icon(
            ui,
            crate::icons::Icon::EyeOff,
            rect.center(),
            40.0,
            palette.muted,
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Canal restrito por idade")
                .font(font(Weight::Bold, 20.0))
                .color(palette.strong),
        );
        ui.label(
            egui::RichText::new(
                "O Discord só mostra este canal a contas que confirmaram ser adultas. \
                 Confirme sua faixa etária em Configurações › Status da conta, no app \
                 do Discord ou no navegador, e abra o canal de novo.",
            )
            .font(font(Weight::Regular, 15.0))
            .color(palette.muted),
        );
        ui.add_space(8.0);
        let button = egui::Button::new(
            egui::RichText::new("Abrir o Discord no navegador")
                .font(font(Weight::Medium, 14.0))
                .color(Color32::WHITE),
        )
        .fill(palette.accent)
        .corner_radius(8)
        .min_size(vec2(0.0, 38.0));
        if ui
            .add(button)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            out.push(Action::Chat(crate::chat::ChatAction::OpenUrl(
                "https://discord.com/channels/@me".into(),
            )));
        }
    });
}

fn skeleton(ui: &mut Ui, palette: &Palette, width: f32) {
    let shade = palette.selected.gamma_multiply(0.6);
    let lines: [&[f32]; 6] = [
        &[0.5, 0.3],
        &[0.7],
        &[0.4, 0.6, 0.2],
        &[0.55],
        &[0.3, 0.45],
        &[0.65, 0.35],
    ];
    for (i, widths) in lines.iter().enumerate() {
        ui.add_space(20.0);
        let (rect, _) = ui.allocate_exact_size(
            vec2(width, 20.0 + widths.len() as f32 * 22.0),
            Sense::hover(),
        );
        ui.painter()
            .circle_filled(pos2(rect.left() + 36.0, rect.top() + 20.0), 20.0, shade);
        let name = Rect::from_min_size(
            pos2(rect.left() + 72.0, rect.top() + 4.0),
            vec2(80.0 + (i % 3) as f32 * 30.0, 14.0),
        );
        ui.painter().rect_filled(name, CornerRadius::same(7), shade);
        for (j, w) in widths.iter().enumerate() {
            let line = Rect::from_min_size(
                pos2(rect.left() + 72.0, rect.top() + 26.0 + j as f32 * 22.0),
                vec2((width - 120.0) * w, 14.0),
            );
            ui.painter().rect_filled(line, CornerRadius::same(7), shade);
        }
    }
}

/// Discord's red line where the unread messages begin.
fn new_divider(ui: &mut Ui, palette: &Palette, width: f32) {
    ui.add_space(12.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 12.0), Sense::hover());
    let y = rect.center().y;
    ui.painter().hline(
        (rect.left() + 16.0)..=(rect.right() - 16.0),
        y,
        Stroke::new(1.0, palette.danger),
    );
    let label =
        ui.painter()
            .layout_no_wrap("NOVAS".into(), font(Weight::Bold, 10.0), Color32::WHITE);
    let pill = Rect::from_min_size(
        pos2(rect.right() - 16.0 - label.size().x - 10.0, y - 7.0),
        vec2(label.size().x + 10.0, 14.0),
    );
    ui.painter().rect_filled(
        pill,
        CornerRadius {
            nw: 0,
            sw: 0,
            ne: 4,
            se: 4,
        },
        palette.danger,
    );
    let tip = [
        pos2(pill.left() - 6.0, y),
        pos2(pill.left(), pill.top()),
        pos2(pill.left(), pill.bottom()),
    ];
    ui.painter().add(egui::Shape::convex_polygon(
        tip.to_vec(),
        palette.danger,
        Stroke::NONE,
    ));
    ui.painter()
        .galley(pill.center() - label.size() / 2.0, label, Color32::WHITE);
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
    let mut out = Vec::new();
    egui::Panel::right("members")
        .resizable(false)
        .exact_size(MEMBERS_WIDTH)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(crate::theme::region(palette.sidebar, crate::theme::Region::Sidebar))
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
                    let mut row = message::Row {
                        app,
                        guild: Some(id),
                        channel: 0,
                        out: &mut out,
                        found: false,
                    };
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
                            ListItem::Member(user) => member(&mut row, ui, id, *user),
                        }
                    }
                });
        });
    app.actions.extend(out);
}

fn member(row: &mut message::Row<'_>, ui: &mut Ui, guild: Id, id: Id) {
    let app = row.app;
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
    // Discord opens the card to the left of the member list.
    if response.clicked() {
        row.out
            .push(Action::Chat(crate::chat::ChatAction::ShowProfile {
                user: id,
                anchor: rect,
                side: crate::chat::Side::Left,
            }));
    }
    message::user_menu(row, &response, id);
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
