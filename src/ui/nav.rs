//! Getting around, drawn: the quick switcher (Ctrl+K), the list of
//! shortcuts (Ctrl+/), the pinned messages and inbox popouts, the search
//! results beside the conversation, and the keyboard shortcuts themselves.

use egui::{
    Align2, Color32, CornerRadius, Frame, Key, Margin, Modifiers, Rect, RichText, Sense, Shadow,
    Stroke, Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::message;
use super::widgets::{Elide, avatar, badge, font, icon_button, paint_icon};
use crate::app::{Action, App, Place};
use crate::chat::{ChatAction, PickerTab, PickerTarget};
use crate::icons::Icon;
use crate::model::ChannelKind;
use crate::nav::{Found, InboxTab, NavAction, Popout, SHORTCUTS, Target};

fn act(app: &mut App, action: NavAction) {
    app.actions.push(Action::Nav(action));
}

/// Discord's keyboard shortcuts, before the views take their keys.
pub fn keys(app: &mut App, ctx: &egui::Context) {
    if app.nav.switcher.is_some() || app.admin.modal.is_some() || app.chat.dialog.is_some() {
        return;
    }
    let cmd = Modifiers::COMMAND;
    let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
    let cmd_alt = Modifiers::COMMAND | Modifiers::ALT;
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
    let pressed = |modifiers: Modifiers, key: Key| {
        ctx.input_mut(|i| {
            // Exact modifiers, so Alt+Shift+↑ is not also Alt+↑.
            let hit = i.events.iter().position(|e| {
                matches!(e, egui::Event::Key { key: k, pressed: true, modifiers: m, .. }
                    if *k == key && m.matches_exact(modifiers))
            });
            if let Some(at) = hit {
                i.events.remove(at);
            }
            hit.is_some()
        })
    };
    let mut out: Vec<Action> = Vec::new();
    let nav = |a: NavAction| Action::Nav(a);
    if pressed(cmd, Key::K) {
        out.push(nav(NavAction::ToggleSwitcher));
    }
    if pressed(cmd, Key::Slash) {
        out.push(nav(NavAction::ToggleShortcuts));
    }
    if pressed(cmd, Key::I) {
        out.push(nav(NavAction::TogglePopout(Popout::Inbox)));
    }
    if pressed(cmd, Key::P) && app.channel.is_some() {
        out.push(nav(NavAction::TogglePopout(Popout::Pins)));
    }
    if pressed(cmd, Key::U) && app.server().is_some() {
        out.push(Action::ToggleMembers);
    }
    if pressed(cmd, Key::F) {
        app.nav.focus_search = true;
    }
    for (key, tab) in [
        (Key::E, PickerTab::Emoji),
        (Key::G, PickerTab::Gif),
        (Key::S, PickerTab::Sticker),
    ] {
        if pressed(cmd, key) && app.channel.is_some() {
            out.push(Action::Chat(ChatAction::OpenPicker {
                tab,
                target: PickerTarget::Composer,
            }));
        }
    }
    if pressed(cmd_shift, Key::U) && app.channel.is_some() {
        out.push(Action::Chat(ChatAction::AttachFiles));
    }
    if pressed(cmd_shift, Key::M) {
        out.push(Action::ToggleMute);
    }
    if pressed(cmd_shift, Key::D) {
        out.push(Action::ToggleDeafen);
    }
    if pressed(cmd, Key::Comma) {
        out.push(Action::OpenSettings(crate::app::SettingsPage::Account));
    }
    for (key, step) in [(Key::ArrowUp, -1), (Key::ArrowDown, 1)] {
        if pressed(alt_shift, key) {
            out.push(nav(NavAction::StepUnread(step)));
        }
        if pressed(cmd_alt, key) {
            out.push(nav(NavAction::StepServer(step)));
        }
    }
    if pressed(Modifiers::SHIFT, Key::Escape)
        && let Some(guild) = app.server()
    {
        out.push(Action::MarkGuildRead(guild));
    }
    // Escape closes what floats, else marks the channel read. The composer
    // does the same when it has the focus.
    let nothing_open = app.chat.popout.is_none()
        && app.chat.picker.is_none()
        && app.chat.viewer.is_none()
        && !app.admin.menu
        && app.chat.editing.is_none();
    if nothing_open && pressed(Modifiers::NONE, Key::Escape) {
        if app.nav.popout.is_some() {
            out.push(nav(NavAction::ClosePopout));
        } else if app.nav.shortcuts {
            out.push(nav(NavAction::ToggleShortcuts));
        } else if let Some(channel) = app.channel {
            out.push(Action::MarkRead(channel));
        }
    }
    app.actions.extend(out);
}

/// Ctrl+K: Discord's quick switcher.
pub fn switcher(app: &mut App, ui: &mut Ui) {
    let Some(state) = &app.nav.switcher else {
        return;
    };
    let palette = app.palette;
    let list = crate::nav::candidates(app, &state.query);
    let mut selected = state.selected.min(list.len().saturating_sub(1));
    let (up, down, enter, escape) = ui.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape)
                || i.consume_key(Modifiers::COMMAND, Key::K),
        )
    });
    if !list.is_empty() {
        if up {
            selected = (selected + list.len() - 1) % list.len();
        }
        if down {
            selected = (selected + 1) % list.len();
        }
    }
    let mut chosen = enter
        .then(|| list.get(selected).map(|c| c.target))
        .flatten();
    let mut query = state.query.clone();
    let response = egui::Modal::new(egui::Id::new("quick-switcher"))
        .backdrop_color(Color32::from_black_alpha(170))
        .frame(
            Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(20)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(560.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Pesquisar por servidores, canais ou DMs")
                        .font(font(Weight::Medium, 14.0))
                        .color(palette.muted),
                );
            });
            ui.add_space(12.0);
            Frame::new()
                .fill(palette.rail)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(12, 12))
                .show(ui, |ui| {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut query)
                            .frame(Frame::NONE)
                            .hint_text(
                                RichText::new("Para onde você gostaria de ir?")
                                    .color(palette.muted),
                            )
                            .font(font(Weight::Regular, 20.0))
                            .text_color(palette.strong)
                            .desired_width(f32::INFINITY),
                    );
                    edit.request_focus();
                });
            ui.add_space(12.0);
            if list.is_empty() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Nada encontrado.")
                            .font(font(Weight::Medium, 15.0))
                            .color(palette.muted),
                    );
                });
                ui.add_space(24.0);
            } else {
                if state.query.is_empty() {
                    ui.label(
                        RichText::new("CANAIS ANTERIORES")
                            .font(font(Weight::Bold, 12.0))
                            .color(palette.muted),
                    );
                    ui.add_space(6.0);
                }
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (index, candidate) in list.iter().enumerate() {
                            let (rect, row) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 36.0),
                                Sense::click(),
                            );
                            if row.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                                selected = index;
                            }
                            let lit = index == selected;
                            if lit {
                                ui.painter().rect_filled(
                                    rect,
                                    CornerRadius::same(4),
                                    palette.selected,
                                );
                                if up || down {
                                    ui.scroll_to_rect(rect, None);
                                }
                            }
                            let icon_at = pos2(rect.left() + 20.0, rect.center().y);
                            match candidate.target {
                                Target::Channel(id) => {
                                    let channel = app.model.channel(id);
                                    match channel.map(|c| c.kind) {
                                        Some(ChannelKind::Direct) => {
                                            if let Some(user) = channel
                                                .and_then(|c| c.recipients.first())
                                                .and_then(|u| app.model.user(*u))
                                            {
                                                avatar(ui, app, user, icon_at, 11.0, None);
                                            }
                                        }
                                        Some(ChannelKind::Group) => paint_icon(
                                            ui,
                                            Icon::Users,
                                            icon_at,
                                            18.0,
                                            palette.muted,
                                        ),
                                        Some(_) => paint_icon(
                                            ui,
                                            channel.map_or(Icon::Hash, |c| {
                                                super::widgets::channel_icon_of(&app.model, c)
                                            }),
                                            icon_at,
                                            18.0,
                                            palette.muted,
                                        ),
                                        None => {
                                            paint_icon(ui, Icon::Hash, icon_at, 18.0, palette.muted)
                                        }
                                    }
                                }
                                Target::User(user) => {
                                    if let Some(user) = app.model.user(user) {
                                        avatar(ui, app, user, icon_at, 11.0, None);
                                    }
                                }
                                Target::Guild(guild) => {
                                    let icon = app
                                        .model
                                        .guild(guild)
                                        .and_then(|g| app.picture(g.icon.as_deref()));
                                    let r = Rect::from_center_size(icon_at, Vec2::splat(22.0));
                                    match icon {
                                        Some(uri) => egui::Image::new(uri)
                                            .corner_radius(CornerRadius::same(6))
                                            .paint_at(ui, r),
                                        None => {
                                            ui.painter().rect_filled(
                                                r,
                                                CornerRadius::same(6),
                                                palette.chat,
                                            );
                                        }
                                    }
                                }
                            }
                            let strong = candidate.unread || candidate.mentions > 0 || lit;
                            let color = if strong { palette.strong } else { palette.text };
                            let right = rect.right() - 12.0;
                            let mut text_right = right;
                            if candidate.mentions > 0 {
                                badge(
                                    ui,
                                    &palette,
                                    pos2(right, rect.center().y),
                                    candidate.mentions,
                                    None,
                                );
                                text_right -= 32.0;
                            } else if candidate.unread {
                                ui.painter().circle_filled(
                                    pos2(right - 4.0, rect.center().y),
                                    4.0,
                                    palette.strong,
                                );
                                text_right -= 16.0;
                            }
                            if let Some(detail) = &candidate.detail {
                                let galley = ui.painter().layout_no_wrap(
                                    detail.to_uppercase(),
                                    font(Weight::SemiBold, 11.0),
                                    palette.muted,
                                );
                                let w = galley.size().x.min(180.0);
                                Elide::new(ui, text_right).text(
                                    pos2(text_right - w, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    detail.to_uppercase(),
                                    font(Weight::SemiBold, 11.0),
                                    palette.muted,
                                );
                                text_right -= w + 12.0;
                            }
                            Elide::new(ui, text_right).text(
                                pos2(rect.left() + 40.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                candidate.title.clone(),
                                font(Weight::Medium, 16.0),
                                color,
                            );
                            if row.clicked() {
                                chosen = Some(candidate.target);
                            }
                        }
                    });
            }
            ui.add_space(14.0);
            ui.label(
                RichText::new(
                    "DICA PROFISSIONAL: Comece a busca com @ # ! * para filtrar os resultados.",
                )
                .font(font(Weight::SemiBold, 12.0))
                .color(palette.muted),
            );
        });
    if let Some(state) = &mut app.nav.switcher {
        if query != state.query {
            state.query = query;
            selected = 0;
        }
        state.selected = selected;
    }
    if let Some(target) = chosen {
        act(app, NavAction::SwitchTo(target));
    } else if escape || response.should_close() {
        act(app, NavAction::CloseSwitcher);
    }
}

/// Ctrl+/: every shortcut.
pub fn shortcuts(app: &mut App, ui: &mut Ui) {
    if !app.nav.shortcuts {
        return;
    }
    let palette = app.palette;
    let response = egui::Modal::new(egui::Id::new("shortcuts"))
        .backdrop_color(Color32::from_black_alpha(170))
        .frame(
            Frame::new()
                .fill(palette.chat)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(24)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(640.0);
            ui.label(
                RichText::new("Atalhos de teclado")
                    .font(font(Weight::Bold, 20.0))
                    .color(palette.strong),
            );
            ui.add_space(12.0);
            egui::ScrollArea::vertical()
                .max_height(ui.ctx().content_rect().height() * 0.7)
                .show(ui, |ui| shortcut_rows(app, ui));
        });
    if response.should_close() {
        act(app, NavAction::ToggleShortcuts);
    }
}

/// The shortcuts as rows: what it does on the left, the keys on the right.
pub fn shortcut_rows(app: &App, ui: &mut Ui) {
    let palette = app.palette;
    for (keys, what) in SHORTCUTS {
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            ui.label(
                RichText::new(*what)
                    .font(font(Weight::Medium, 15.0))
                    .color(palette.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                Frame::new()
                    .fill(palette.rail)
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(*keys)
                                .font(font(Weight::SemiBold, 13.0))
                                .color(palette.strong),
                        );
                    });
            });
        });
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().hline(
            rect.x_range(),
            rect.center().y,
            Stroke::new(1.0, palette.divider),
        );
    }
}

/// Where a header button sits, for its popout to hang from.
pub fn remember_anchor(ui: &Ui, popout: Popout, rect: Rect) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(("popout-anchor", popout as u8)), rect));
}

/// The pinned messages or the inbox, hanging below their header button.
pub fn popout(app: &mut App, ui: &mut Ui) {
    let Some(popout) = app.nav.popout else {
        return;
    };
    let palette = app.palette;
    let anchor: Rect = ui
        .data(|d| d.get_temp(egui::Id::new(("popout-anchor", popout as u8))))
        .unwrap_or(Rect::from_min_size(
            pos2(ui.ctx().content_rect().right() - 40.0, 48.0),
            Vec2::splat(24.0),
        ));
    let width = if popout == Popout::Inbox {
        480.0
    } else {
        420.0
    };
    let screen = ui.ctx().content_rect();
    let left = (anchor.right() - width).max(screen.left() + 8.0);
    let height = (screen.bottom() - anchor.bottom() - 40.0).clamp(200.0, 640.0);
    let mut out: Vec<Action> = Vec::new();
    let area = egui::Area::new(egui::Id::new("nav-popout"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(left, anchor.bottom() + 8.0))
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius::same(8))
                .shadow(Shadow {
                    offset: [0, 8],
                    blur: 16,
                    spread: 0,
                    color: Color32::from_black_alpha(110),
                })
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.set_max_height(height);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    match popout {
                        Popout::Pins => pins(app, ui, &mut out),
                        Popout::Inbox => inbox(app, ui, &mut out),
                        Popout::Threads => super::threads::popout(app, ui, &mut out),
                    }
                });
        });
    app.actions.extend(out);
    if area.response.clicked_elsewhere() {
        act(app, NavAction::ClosePopout);
    }
}

fn popout_title(app: &App, ui: &mut Ui, icon: Icon, title: &str) {
    let palette = app.palette;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
    paint_icon(
        ui,
        icon,
        pos2(rect.left() + 26.0, rect.center().y),
        20.0,
        palette.icon,
    );
    ui.painter().text(
        pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        font(Weight::SemiBold, 16.0),
        palette.strong,
    );
}

fn pins(app: &mut App, ui: &mut Ui, out: &mut Vec<Action>) {
    let palette = app.palette;
    popout_title(app, ui, Icon::Pin, "Mensagens fixadas");
    let Some(channel) = app.channel else { return };
    match app.nav.pins.get(&channel).cloned() {
        None => empty_note(app, ui, "Carregando…"),
        Some(list) if list.is_empty() => empty_note(
            app,
            ui,
            "Este canal não tem nenhuma mensagem fixada... ainda.",
        ),
        Some(list) => {
            let _ = palette;
            found_list(app, ui, &list, false, out);
        }
    }
}

fn inbox(app: &mut App, ui: &mut Ui, out: &mut Vec<Action>) {
    let palette = app.palette;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
    paint_icon(
        ui,
        Icon::Inbox,
        pos2(rect.left() + 26.0, rect.center().y),
        20.0,
        palette.icon,
    );
    ui.painter().text(
        pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Caixa de entrada",
        font(Weight::SemiBold, 16.0),
        palette.strong,
    );
    // The tabs on the right of the title, as Discord.
    let mut x = rect.right() - 16.0;
    for (tab, label) in [
        (InboxTab::Unreads, "Não lidas"),
        (InboxTab::Mentions, "Menções"),
    ] {
        let galley =
            ui.painter()
                .layout_no_wrap(label.into(), font(Weight::Medium, 14.0), palette.text);
        let size = galley.size() + vec2(16.0, 6.0);
        let tab_rect = Rect::from_min_size(pos2(x - size.x, rect.center().y - size.y / 2.0), size);
        x -= size.x + 6.0;
        let response = ui.interact(tab_rect, ui.id().with(("inbox-tab", label)), Sense::click());
        let selected = app.nav.inbox_tab == tab;
        if selected {
            ui.painter()
                .rect_filled(tab_rect, CornerRadius::same(4), palette.selected);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(tab_rect, CornerRadius::same(4), palette.hover);
        }
        ui.painter().galley(
            tab_rect.center() - galley.size() / 2.0,
            galley,
            if selected {
                palette.strong
            } else {
                palette.muted
            },
        );
        if response.clicked() {
            out.push(Action::Nav(NavAction::InboxTab(tab)));
        }
    }
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        line.x_range(),
        line.center().y,
        Stroke::new(1.0, palette.divider),
    );
    match app.nav.inbox_tab {
        InboxTab::Mentions => match app.nav.mentions.clone() {
            None => empty_note(app, ui, "Carregando…"),
            Some(list) if list.is_empty() => {
                empty_note(app, ui, "Você está em dia! Nenhuma menção recente.")
            }
            Some(list) => found_list(app, ui, &list, true, out),
        },
        InboxTab::Unreads => unreads(app, ui, out),
    }
}

/// Every channel with something new, by place, with a button to mark it
/// read.
fn unreads(app: &App, ui: &mut Ui, out: &mut Vec<Action>) {
    let palette = app.palette;
    let mut rows: Vec<(crate::model::Id, String, Option<String>, u32)> = Vec::new();
    for id in app.model.sorted_direct() {
        if let Some(c) = app.model.channel(id)
            && (c.unread() || c.mentions > 0)
        {
            rows.push((id, app.model.direct_name(c), None, c.mentions));
        }
    }
    for guild in &app.model.guilds {
        for id in &guild.channels {
            if let Some(c) = app.model.channel(*id)
                && c.kind.is_textual()
                && (c.unread() || c.mentions > 0)
                && app.model.can_view(c)
            {
                rows.push((
                    *id,
                    format!("#{}", c.name),
                    Some(guild.name.clone()),
                    c.mentions,
                ));
            }
        }
    }
    if rows.is_empty() {
        empty_note(app, ui, "Tudo lido! Nada novo por aqui.");
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add_space(8.0);
            for (id, name, place, mentions) in rows {
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
                let inner = rect.shrink2(vec2(8.0, 2.0));
                if response.hovered() {
                    ui.painter()
                        .rect_filled(inner, CornerRadius::same(4), palette.hover);
                }
                let check = Rect::from_center_size(
                    pos2(inner.right() - 20.0, inner.center().y),
                    Vec2::splat(28.0),
                );
                let check = ui.interact(check, ui.id().with(("read", id)), Sense::click());
                ui.painter().circle_filled(
                    check.rect.center(),
                    14.0,
                    if check.hovered() {
                        palette.selected
                    } else {
                        palette.chat
                    },
                );
                paint_icon(ui, Icon::Check, check.rect.center(), 16.0, palette.strong);
                let check = check.on_hover_text("Marcar como lido");
                let text_right = inner.right() - 44.0 - if mentions > 0 { 30.0 } else { 0.0 };
                if mentions > 0 {
                    badge(
                        ui,
                        &palette,
                        pos2(inner.right() - 44.0, inner.center().y),
                        mentions,
                        None,
                    );
                }
                let top = if place.is_some() {
                    inner.center().y - 9.0
                } else {
                    inner.center().y
                };
                Elide::new(ui, text_right).text(
                    pos2(inner.left() + 12.0, top),
                    Align2::LEFT_CENTER,
                    name,
                    font(Weight::SemiBold, 15.0),
                    palette.strong,
                );
                if let Some(place) = place {
                    Elide::new(ui, text_right).text(
                        pos2(inner.left() + 12.0, inner.center().y + 10.0),
                        Align2::LEFT_CENTER,
                        place,
                        font(Weight::Medium, 12.0),
                        palette.muted,
                    );
                }
                if check.clicked() {
                    out.push(Action::MarkRead(id));
                } else if response.clicked() {
                    out.push(Action::OpenChannel(id));
                    out.push(Action::Nav(NavAction::ClosePopout));
                }
            }
            ui.add_space(8.0);
        });
}

fn empty_note(app: &App, ui: &mut Ui, text: &str) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(text)
                .font(font(Weight::Medium, 15.0))
                .color(app.palette.muted),
        );
    });
    ui.add_space(40.0);
}

/// Messages shown away from their channel, each in a card with "Pular",
/// and with where it was said when `places`.
fn found_list(app: &App, ui: &mut Ui, list: &[Found], places: bool, out: &mut Vec<Action>) {
    let palette = app.palette;
    egui::ScrollArea::vertical()
        .id_salt(("found", places))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add_space(8.0);
            for found in list {
                if places {
                    place_line(app, ui, found);
                }
                Frame::new()
                    .fill(palette.chat)
                    .corner_radius(CornerRadius::same(6))
                    .stroke(Stroke::new(1.0, palette.divider))
                    .inner_margin(Margin::symmetric(0, 6))
                    .outer_margin(Margin::symmetric(12, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let mut row = message::Row {
                            app,
                            guild: found.guild,
                            channel: found.channel,
                            out,
                            found: true,
                        };
                        message::show(&mut row, ui, &found.message, false);
                    });
            }
            ui.add_space(8.0);
        });
}

/// "# canal • Servidor" above a message from elsewhere.
fn place_line(app: &App, ui: &mut Ui, found: &Found) {
    let palette = app.palette;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let channel = app.model.channel(found.channel);
    let name = match channel {
        Some(c) if c.guild.is_none() => app.model.direct_name(c),
        Some(c) => format!("#{}", c.name),
        None => "#canal".into(),
    };
    let place = found
        .guild
        .and_then(|g| app.model.guild(g))
        .map(|g| format!("  •  {}", g.name))
        .unwrap_or_default();
    Elide::new(ui, rect.right() - 16.0).text(
        pos2(rect.left() + 16.0, rect.center().y),
        Align2::LEFT_CENTER,
        format!("{name}{place}"),
        font(Weight::SemiBold, 13.0),
        palette.text,
    );
}

/// The search results beside the conversation, where the member list was.
pub fn search_panel(app: &mut App, ui: &mut Ui) {
    let Some(search) = &app.nav.search else {
        return;
    };
    let palette = app.palette;
    let title = match (&search.results, &search.error) {
        (None, _) => "Buscando…".to_owned(),
        (Some(_), Some(_)) => "Erro na busca".to_owned(),
        (Some(_), None) if search.total == 1 => "1 resultado".to_owned(),
        (Some(_), None) => format!("{} resultados", search.total),
    };
    let results = search.results.clone();
    let error = search.error.clone();
    let mut out: Vec<Action> = Vec::new();
    egui::Panel::right("search-results")
        .resizable(false)
        .exact_size(420.0)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.sidebar)
                .inner_margin(Margin::ZERO),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let (rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
            ui.painter().text(
                pos2(rect.left() + 16.0, rect.center().y),
                Align2::LEFT_CENTER,
                title,
                font(Weight::SemiBold, 16.0),
                palette.strong,
            );
            let close = Rect::from_center_size(
                pos2(rect.right() - 24.0, rect.center().y),
                Vec2::splat(28.0),
            );
            let close = ui.interact(close, ui.id().with("close-search"), Sense::click());
            paint_icon(
                ui,
                Icon::Close,
                close.rect.center(),
                18.0,
                if close.hovered() {
                    palette.strong
                } else {
                    palette.icon
                },
            );
            if close.on_hover_text("Fechar").clicked() {
                out.push(Action::Nav(NavAction::CloseSearch));
            }
            match (results, error) {
                (None, _) => {}
                (Some(_), Some(error)) => empty_note(app, ui, &error),
                (Some(list), None) if list.is_empty() => {
                    empty_note(app, ui, "Nenhum resultado. Tente outras palavras.")
                }
                (Some(list), None) => found_list(app, ui, &list, true, &mut out),
            }
        });
    app.actions.extend(out);
}

/// The header's search field: Enter searches the server, or the
/// conversation at home; Ctrl+F puts the cursor in it.
pub fn search_field(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let id = egui::Id::new("header-search");
    let focused = ui.memory(|m| m.has_focus(id));
    let width = if focused || !app.nav.search_text.is_empty() {
        240.0
    } else {
        144.0
    };
    let width = ui
        .ctx()
        .animate_value_with_time(id.with("width"), width, 0.15);
    let mut submit = false;
    let mut clear = false;
    let frame = Frame::new()
        .fill(palette.rail)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.set_width(width - 12.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut app.nav.search_text)
                        .id(id)
                        .frame(Frame::NONE)
                        .hint_text(RichText::new("Buscar").color(palette.muted))
                        .font(font(Weight::Medium, 14.0))
                        .text_color(palette.text)
                        .desired_width(width - 40.0),
                );
                if std::mem::take(&mut app.nav.focus_search) {
                    edit.request_focus();
                }
                if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submit = true;
                }
                if edit.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                    clear = true;
                }
                let (icon, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                if app.nav.search_text.is_empty() {
                    paint_icon(ui, Icon::Search, icon.center(), 16.0, palette.muted);
                } else {
                    let x = ui.interact(icon, id.with("clear"), Sense::click());
                    paint_icon(ui, Icon::Close, icon.center(), 16.0, palette.text);
                    if x.clicked() {
                        clear = true;
                    }
                }
            });
        });
    let _ = frame;
    if submit {
        let text = app.nav.search_text.clone();
        act(app, NavAction::Search(text));
    }
    if clear {
        act(app, NavAction::CloseSearch);
    }
}

/// The inbox and pins buttons, which remember where they are so their
/// popouts hang from them.
pub fn popout_button(app: &mut App, ui: &mut Ui, popout: Popout) {
    let palette = app.palette;
    let (icon, hint) = match popout {
        Popout::Inbox => (Icon::Inbox, "Caixa de entrada"),
        Popout::Pins => (Icon::Pin, "Mensagens fixadas"),
        Popout::Threads => (Icon::Threads, "Tópicos"),
    };
    let open = app.nav.popout == Some(popout);
    let response = icon_button(ui, &palette, icon, 24.0, open, hint);
    remember_anchor(ui, popout, response.rect);
    if response.clicked() {
        act(app, NavAction::TogglePopout(popout));
    }
}

/// Home with no conversation open shows the friends page.
pub fn is_friends(app: &App) -> bool {
    app.place == Place::Home && app.channel.is_none()
}
