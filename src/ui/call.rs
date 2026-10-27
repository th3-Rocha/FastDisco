//! Voice, drawn: who is in each voice channel under it in the channel
//! list, the "Voz conectada" panel above the user panel, and the call's
//! tiles in the middle, as Discord shows them.

use egui::{Align2, Color32, CornerRadius, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use fastframe_fonts::Weight;

use super::widgets::{Elide, avatar, font, paint_icon};
use crate::app::{Action, App};
use crate::call::State;
use crate::icons::Icon;
use crate::model::{Id, VoiceMember};
use crate::theme::SIDEBAR_WIDTH;

/// Discord's green for a voice that is talking.
pub const TALKING: Color32 = Color32::from_rgb(0x23, 0xa5, 0x5a);

/// The people in a voice channel, under its row.
pub fn members(app: &mut App, ui: &mut Ui, channel: Id) {
    let palette = app.palette;
    let guild = app.model.channel(channel).and_then(|c| c.guild);
    for (user, state) in app.model.in_voice(channel) {
        let (slot, response) = ui.allocate_exact_size(vec2(SIDEBAR_WIDTH, 35.0), Sense::click());
        let rect = Rect::from_min_max(
            pos2(slot.left() + 38.0, slot.top() + 1.0),
            pos2(slot.right() - 8.0, slot.bottom() - 1.0),
        );
        let lit = response.hovered() || response.context_menu_opened();
        if lit {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), palette.hover);
        }
        let center = pos2(rect.left() + 19.0, rect.center().y);
        if let Some(person) = app.model.user(user) {
            avatar(ui, app, person, center, 12.0, None);
        }
        let talking = app.talking(user);
        if talking {
            ui.painter()
                .circle_stroke(center, 13.5, Stroke::new(2.0, TALKING));
        }
        let mut right = rect.right() - 6.0;
        let red = palette.danger;
        if state.stream {
            let live = ui.painter().layout_no_wrap(
                "AO VIVO".into(),
                font(Weight::Bold, 10.0),
                Color32::WHITE,
            );
            let pill = Rect::from_min_size(
                pos2(right - live.size().x - 8.0, rect.center().y - 8.0),
                vec2(live.size().x + 8.0, 16.0),
            );
            ui.painter().rect_filled(pill, CornerRadius::same(4), red);
            ui.painter().galley(
                pill.min + vec2(4.0, 8.0 - live.size().y / 2.0),
                live,
                Color32::WHITE,
            );
            right = pill.left() - 4.0;
        }
        // Grey when the person chose it, red when the server did, as
        // Discord's list.
        let y = rect.center().y;
        if state.self_deaf || state.deaf {
            let tint = if state.deaf { red } else { palette.icon };
            paint_icon(ui, Icon::HeadphonesOff, pos2(right - 8.0, y), 16.0, tint);
            right -= 22.0;
        }
        // Silenced on this machine shows as muted too.
        let local = app.settings.local_mutes.contains(&user);
        if state.self_mute || state.mute || local {
            let tint = if state.mute { red } else { palette.icon };
            paint_icon(ui, Icon::MicOff, pos2(right - 8.0, y), 16.0, tint);
            right -= 22.0;
        }
        let name = app.model.name_in(guild, user);
        Elide::new(ui, right - 4.0).text(
            pos2(rect.left() + 41.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            font(
                if talking {
                    Weight::SemiBold
                } else {
                    Weight::Medium
                },
                15.0,
            ),
            if talking || lit {
                palette.strong
            } else {
                palette.muted
            },
        );
        if response.clicked() {
            app.actions
                .push(Action::Chat(crate::chat::ChatAction::ShowProfile {
                    user,
                    anchor: rect,
                    side: crate::chat::Side::Right,
                }));
        }
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        member_menu(app, &response, guild, channel, user, &state, rect);
    }
}

/// The right-click menu on someone in a voice channel, as Discord's: the
/// person, their volume here, and for moderators the server's mute,
/// deafen, disconnect and move.
fn member_menu(
    app: &mut App,
    response: &egui::Response,
    guild: Option<Id>,
    channel: Id,
    user: Id,
    state: &VoiceMember,
    anchor: Rect,
) {
    use crate::admin::{AdminAction, Modal};
    use crate::chat::{ChatAction, Side};
    use crate::model::permission;
    use serde_json::json;

    let palette = app.palette;
    let me = user == app.model.me;
    let can = |bit| guild.is_some_and(|g| app.model.can(g, bit));
    let (mute, deafen, moving) = (
        can(permission::MUTE_MEMBERS),
        can(permission::DEAFEN_MEMBERS),
        can(permission::MOVE_MEMBERS),
    );
    let (kick, ban) = (!me && can(permission::KICK), !me && can(permission::BAN));
    let name = app.model.name_in(guild, user);
    let volume = app.settings.user_volumes.get(&user).copied().unwrap_or(100);
    let local = app.settings.local_mutes.contains(&user);
    let (muted, deafened) = (app.settings.muted, app.settings.deafened);
    // Other voice channels of the server, for "Mover para".
    let elsewhere: Vec<(Id, String)> = guild
        .and_then(|g| app.model.guild(g))
        .map(|g| {
            g.channels
                .iter()
                .filter_map(|id| app.model.channel(*id))
                .filter(|c| {
                    c.kind == crate::model::ChannelKind::Voice
                        && c.id != channel
                        && app.model.can_view(c)
                })
                .map(|c| (c.id, c.name.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut chosen: Vec<Action> = Vec::new();
    response.context_menu(|ui| {
        ui.set_min_width(200.0);
        if ui.button("Perfil").clicked() {
            chosen.push(Action::Chat(ChatAction::ShowProfile {
                user,
                anchor,
                side: Side::Right,
            }));
            ui.close();
        }
        if ui.button("Mencionar").clicked() {
            chosen.push(Action::Chat(ChatAction::Mention(user)));
            ui.close();
        }
        if !me && ui.button("Mensagem").clicked() {
            chosen.push(Action::Chat(ChatAction::MessageUser {
                user,
                text: String::new(),
            }));
            ui.close();
        }
        ui.separator();
        if me {
            if check_item(ui, &palette, "Silenciar", muted, palette.text) {
                chosen.push(Action::ToggleMute);
            }
            if check_item(ui, &palette, "Ensurdecer", deafened, palette.text) {
                chosen.push(Action::ToggleDeafen);
            }
        } else {
            ui.label(
                egui::RichText::new("Volume do usuário")
                    .font(font(Weight::SemiBold, 12.0))
                    .color(palette.muted),
            );
            let mut level = f32::from(volume);
            ui.spacing_mut().slider_width = ui.available_width();
            let slider = ui
                .add(
                    egui::Slider::new(&mut level, 0.0..=200.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .on_hover_text(format!("{}%", level.round()));
            if slider.changed() {
                chosen.push(Action::SetUserVolume(user, level.round() as u16));
            }
            if check_item(ui, &palette, "Silenciar", local, palette.text) {
                chosen.push(Action::ToggleLocalMute(user));
            }
        }
        if let Some(guild) = guild
            && (mute || deafen || moving)
        {
            ui.separator();
            let moderate = |change| {
                Action::Admin(AdminAction::VoiceMember {
                    guild,
                    user,
                    change,
                })
            };
            if mute
                && check_item(
                    ui,
                    &palette,
                    "Silenciar no servidor",
                    state.mute,
                    palette.danger,
                )
            {
                chosen.push(moderate(json!({ "mute": !state.mute })));
            }
            if deafen
                && check_item(
                    ui,
                    &palette,
                    "Ensurdecer no servidor",
                    state.deaf,
                    palette.danger,
                )
            {
                chosen.push(moderate(json!({ "deaf": !state.deaf })));
            }
            if moving {
                let disconnect =
                    egui::Button::new(egui::RichText::new("Desconectar").color(palette.danger));
                if ui.add(disconnect).clicked() {
                    chosen.push(moderate(json!({ "channel_id": null })));
                    ui.close();
                }
                if !elsewhere.is_empty() {
                    ui.menu_button("Mover para", |ui| {
                        ui.set_min_width(180.0);
                        for (id, label) in &elsewhere {
                            if ui.button(format!("🔊 {label}")).clicked() {
                                chosen.push(moderate(json!({ "channel_id": id.to_string() })));
                                ui.close();
                            }
                        }
                    });
                }
            }
        }
        if let Some(guild) = guild
            && (kick || ban)
        {
            ui.separator();
            let item = |verb: &str| {
                egui::Button::new(
                    egui::RichText::new(format!("{verb} {name}")).color(palette.danger),
                )
            };
            if kick && ui.add(item("Expulsar")).clicked() {
                chosen.push(Action::Admin(AdminAction::Open(Modal::Kick {
                    guild,
                    user,
                })));
                ui.close();
            }
            if ban && ui.add(item("Banir")).clicked() {
                chosen.push(Action::Admin(AdminAction::Open(Modal::Ban { guild, user })));
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Copiar ID do usuário").clicked() {
            chosen.push(Action::Chat(ChatAction::Copy(user.to_string())));
            ui.close();
        }
    });
    app.actions.extend(chosen);
}

/// A menu row with Discord's checkbox on the right; true when clicked.
/// The menu stays open, as Discord's does for these.
fn check_item(
    ui: &mut Ui,
    palette: &crate::theme::Palette,
    label: &str,
    on: bool,
    color: Color32,
) -> bool {
    let height = ui.spacing().interact_size.y + 4.0;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(2), palette.hover);
    }
    ui.painter().text(
        pos2(rect.left() + 4.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Button.resolve(ui.style()),
        color,
    );
    let tick = Rect::from_center_size(
        pos2(rect.right() - 12.0, rect.center().y),
        Vec2::splat(18.0),
    );
    if on {
        ui.painter()
            .rect_filled(tick, CornerRadius::same(4), palette.accent);
        paint_icon(ui, Icon::Check, tick.center(), 14.0, Color32::WHITE);
    } else {
        ui.painter().rect_stroke(
            tick,
            CornerRadius::same(4),
            Stroke::new(1.5, palette.muted),
            egui::StrokeKind::Inside,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// The height of the call's part of the user card.
pub const SECTION: f32 = 96.0;

/// The call's part of the user card: the state in colour and where, the
/// noise suppression and hang up buttons, and the row of call buttons.
pub fn section(app: &mut App, ui: &mut Ui, rect: Rect) {
    let Some(call) = &app.call else {
        return;
    };
    let palette = app.palette;
    let (label, color) = match &call.state {
        State::Joining | State::Connecting => ("Conectando a RTC", palette.idle),
        State::Connected => ("Voz conectada", TALKING),
        State::Failed(_) => ("Sem rota", palette.danger),
    };
    let channel = call.channel;
    let place = {
        let name = app
            .model
            .channel(channel)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        match call.guild.and_then(|g| app.model.guild(g)) {
            Some(g) => format!("{name} / {}", g.name),
            None => name,
        }
    };
    let error = match &call.state {
        State::Failed(e) => Some(e.clone()),
        _ => None,
    };
    // The icon in its little box, the state and the place.
    let badge = Rect::from_min_size(
        pos2(rect.left() + 10.0, rect.top() + 10.0),
        Vec2::splat(36.0),
    );
    ui.painter().rect(
        badge,
        CornerRadius::same(6),
        crate::theme::mix(palette.user_panel, color, 0.12),
        Stroke::new(1.0, palette.divider),
        egui::StrokeKind::Inside,
    );
    paint_icon(ui, Icon::Connection, badge.center(), 20.0, color);
    let info = Rect::from_min_max(
        pos2(badge.right() + 8.0, badge.top()),
        pos2(rect.right() - 80.0, badge.bottom()),
    );
    let response = ui.interact(info, ui.id().with("voice-info"), Sense::click());
    let title = ui.painter().text(
        pos2(info.left(), info.center().y - 8.0),
        Align2::LEFT_CENTER,
        label,
        font(Weight::SemiBold, 16.0),
        color,
    );
    if response.hovered() {
        ui.painter()
            .hline(title.x_range(), title.bottom(), Stroke::new(1.0, color));
    }
    Elide::new(ui, info.right()).text(
        pos2(info.left(), info.center().y + 10.0),
        Align2::LEFT_CENTER,
        place,
        font(Weight::Regular, 12.0),
        palette.muted,
    );
    let response = match error {
        Some(e) => response.on_hover_text(e),
        None => response,
    };
    if response.clicked() {
        app.actions.push(Action::ShowCall(true));
    }
    // Noise suppression and hang up, at the right.
    let y = badge.center().y;
    let suppressing = app.settings.noise_suppression > 0;
    let noise = Rect::from_center_size(pos2(rect.right() - 58.0, y), Vec2::splat(32.0));
    let noise = ui.interact(noise, ui.id().with("noise"), Sense::click());
    let hang = Rect::from_center_size(pos2(rect.right() - 22.0, y), Vec2::splat(32.0));
    let hang = ui.interact(hang, ui.id().with("hang-up"), Sense::click());
    for (button, icon) in [(&noise, Icon::NoiseSuppression), (&hang, Icon::PhoneOff)] {
        if button.hovered() {
            ui.painter()
                .rect_filled(button.rect, CornerRadius::same(6), palette.hover);
        }
        let tint = if button.hovered() {
            palette.strong
        } else {
            palette.icon
        };
        paint_icon(ui, icon, button.rect.center(), 20.0, tint);
    }
    let hint = if suppressing {
        "Desativar supressão de ruído"
    } else {
        "Ativar supressão de ruído"
    };
    if noise.on_hover_text(hint).clicked() {
        app.actions
            .push(Action::SetNoiseSuppression(if suppressing { 0 } else { 2 }));
    }
    if hang.on_hover_text("Desconectar").clicked() {
        app.actions.push(Action::LeaveVoice);
    }
    // Camera, screen, activities and soundboard, as Discord's row; not in
    // FastDisco yet, so drawn dim.
    let row = Rect::from_min_max(
        pos2(rect.left() + 10.0, badge.bottom() + 8.0),
        pos2(rect.right() - 10.0, badge.bottom() + 40.0),
    );
    let gap = 8.0;
    let width = (row.width() - gap * 3.0) / 4.0;
    let buttons = [
        (Icon::Video, "Ativar câmera"),
        (Icon::ScreenShare, "Compartilhar sua tela"),
        (Icon::Activities, "Iniciar uma atividade"),
        (Icon::Soundboard, "Abrir painel de efeitos sonoros"),
    ];
    for (i, (icon, hint)) in buttons.into_iter().enumerate() {
        let at = Rect::from_min_size(
            pos2(row.left() + i as f32 * (width + gap), row.top()),
            vec2(width, row.height()),
        );
        let response = ui.interact(at, ui.id().with(("call-row", i)), Sense::hover());
        ui.painter().rect_filled(
            at,
            CornerRadius::same(6),
            crate::theme::mix(palette.user_panel, palette.hover, 0.6),
        );
        paint_icon(
            ui,
            icon,
            at.center(),
            20.0,
            palette.icon.gamma_multiply(0.5),
        );
        response.on_hover_text(format!("{hint} (ainda não disponível no FastDisco)"));
    }
}

/// The call in the middle: a tile per person, the talking ones outlined in
/// green, and the buttons at the bottom.
pub fn view(app: &mut App, ui: &mut Ui) {
    let Some(call) = &app.call else {
        return;
    };
    let palette = app.palette;
    let channel = call.channel;
    let guild = call.guild;
    let name = app
        .model
        .channel(channel)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let mut people: Vec<(Id, VoiceMember)> = app.model.in_voice(channel);
    let me = app.model.me;
    if !people.iter().any(|(u, _)| *u == me) {
        // Until the gateway echoes the user, show them anyway.
        people.insert(
            0,
            (
                me,
                VoiceMember {
                    channel,
                    ..Default::default()
                },
            ),
        );
    }
    let rect = ui.max_rect();
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, Color32::BLACK);
    ui.painter().text(
        pos2(rect.left() + 20.0, rect.top() + 24.0),
        Align2::LEFT_CENTER,
        format!("🔊 {name}"),
        font(Weight::SemiBold, 16.0),
        Color32::WHITE,
    );
    // Tiles in a grid that fits the space, 16:9 each.
    let area = Rect::from_min_max(
        pos2(rect.left() + 16.0, rect.top() + 56.0),
        pos2(rect.right() - 16.0, rect.bottom() - 96.0),
    );
    let count = people.len().max(1);
    let mut columns = 1;
    while columns < count {
        let rows = count.div_ceil(columns);
        let w = (area.width() - 8.0 * (columns as f32 - 1.0)) / columns as f32;
        if (w * 9.0 / 16.0) * rows as f32 + 8.0 * (rows as f32 - 1.0) <= area.height() {
            break;
        }
        columns += 1;
    }
    let rows = count.div_ceil(columns);
    let mut w = (area.width() - 8.0 * (columns as f32 - 1.0)) / columns as f32;
    let mut h = w * 9.0 / 16.0;
    let total_h = h * rows as f32 + 8.0 * (rows as f32 - 1.0);
    if total_h > area.height() {
        h = (area.height() - 8.0 * (rows as f32 - 1.0)) / rows as f32;
        w = h * 16.0 / 9.0;
    }
    let grid_w = w * columns as f32 + 8.0 * (columns as f32 - 1.0);
    let grid_h = h * rows as f32 + 8.0 * (rows as f32 - 1.0);
    let origin = pos2(
        area.center().x - grid_w / 2.0,
        area.center().y - grid_h / 2.0,
    );
    for (i, (user, state)) in people.iter().enumerate() {
        let (col, row) = (i % columns, i / columns);
        let tile = Rect::from_min_size(
            origin + vec2(col as f32 * (w + 8.0), row as f32 * (h + 8.0)),
            vec2(w, h),
        );
        let person = app.model.user(*user);
        // The avatar's own colour, as Discord's tiles; until it is known,
        // the colour behind the initial.
        let fill = person.map_or(palette.sidebar, |p| {
            app.picture_color(p.picture.as_deref())
                .unwrap_or_else(|| p.color().gamma_multiply(0.55))
        });
        ui.painter().rect_filled(tile, CornerRadius::same(8), fill);
        let radius = (h * 0.22).clamp(20.0, 60.0);
        if let Some(person) = person {
            avatar(ui, app, person, tile.center(), radius, None);
        }
        if app.talking(*user) {
            ui.painter().rect_stroke(
                tile,
                CornerRadius::same(8),
                Stroke::new(3.0, TALKING),
                egui::StrokeKind::Inside,
            );
        }
        // The name, and how they hear and speak, at the bottom left.
        let label = app.model.name_in(guild, *user);
        let galley =
            ui.painter()
                .layout_no_wrap(label, font(Weight::SemiBold, 14.0), Color32::WHITE);
        let icons = (state.self_mute || state.mute) as u8 + (state.self_deaf || state.deaf) as u8;
        let pill = Rect::from_min_size(
            pos2(tile.left() + 8.0, tile.bottom() - 32.0),
            vec2(galley.size().x + 16.0 + 20.0 * f32::from(icons), 24.0),
        );
        ui.painter()
            .rect_filled(pill, CornerRadius::same(4), Color32::from_black_alpha(150));
        let mut x = pill.left() + 8.0;
        if state.self_mute || state.mute {
            let at = pos2(x + 7.0, pill.center().y);
            paint_icon(ui, Icon::MicOff, at, 14.0, palette.danger);
            x += 20.0;
        }
        if state.self_deaf || state.deaf {
            let at = pos2(x + 7.0, pill.center().y);
            paint_icon(ui, Icon::HeadphonesOff, at, 14.0, palette.danger);
            x += 20.0;
        }
        ui.painter().galley(
            pos2(x, pill.center().y - galley.size().y / 2.0),
            galley,
            Color32::WHITE,
        );
    }
    // The bottom bar: microphone, sound, hang up.
    let center = pos2(rect.center().x, rect.bottom() - 48.0);
    let (muted, deaf) = (app.settings.muted, app.settings.deafened);
    let buttons = [
        (
            Icon::Mic,
            muted,
            if muted {
                "Ativar microfone"
            } else {
                "Silenciar"
            },
            Action::ToggleMute,
        ),
        (
            Icon::Headphones,
            deaf,
            if deaf {
                "Ativar áudio"
            } else {
                "Desativar áudio"
            },
            Action::ToggleDeafen,
        ),
        (Icon::PhoneOff, false, "Desconectar", Action::LeaveVoice),
    ];
    for (i, (icon, off, hint, action)) in buttons.into_iter().enumerate() {
        let c = center + vec2((i as f32 - 1.0) * 64.0, 0.0);
        let r = Rect::from_center_size(c, Vec2::splat(52.0));
        let response = ui.interact(r, ui.id().with(("call-button", i)), Sense::click());
        let fill = match (icon, off, response.hovered()) {
            (Icon::PhoneOff, _, false) => palette.danger,
            (Icon::PhoneOff, _, true) => palette.danger.gamma_multiply(0.8),
            (_, true, _) => Color32::WHITE,
            (_, false, true) => Color32::from_gray(70),
            (_, false, false) => Color32::from_gray(45),
        };
        ui.painter().circle_filled(c, 26.0, fill);
        let (icon, tint) = match (icon, off) {
            (Icon::Mic, true) => (Icon::MicOff, palette.danger),
            (Icon::Headphones, true) => (Icon::HeadphonesOff, palette.danger),
            _ => (icon, Color32::WHITE),
        };
        paint_icon(ui, icon, c, 24.0, tint);
        if response.on_hover_text(hint).clicked() {
            app.actions.push(action);
        }
    }
    if let Some(State::Failed(error)) = app.call.as_ref().map(|c| c.state.clone()) {
        ui.painter().text(
            pos2(rect.center().x, rect.bottom() - 92.0),
            Align2::CENTER_CENTER,
            format!("Não foi possível conectar: {error}"),
            font(Weight::Medium, 14.0),
            palette.danger,
        );
    }
    // The tiles move with who talks.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}
