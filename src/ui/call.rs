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

/// Discord's green for a voice that is talking.
pub const TALKING: Color32 = Color32::from_rgb(0x23, 0xa5, 0x5a);

/// The people in a voice channel, under its row.
pub fn members(app: &mut App, ui: &mut Ui, channel: Id) {
    let palette = app.palette;
    let guild = app.model.channel(channel).and_then(|c| c.guild);
    let in_call = app.call.as_ref().is_some_and(|c| c.channel == channel);
    for (user, state) in app.model.in_voice(channel) {
        let (slot, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 35.0), Sense::click());
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
            // From inside the call, the badge opens the share.
            if in_call {
                let badge = ui.interact(pill, ui.id().with(("live", user)), Sense::click());
                let badge = badge.on_hover_cursor(egui::CursorIcon::PointingHand);
                let badge = if crate::call::CAN_WATCH {
                    badge.on_hover_text("Assistir transmissão")
                } else {
                    badge.on_hover_text("Assistir transmissão (ainda não disponível no Discofast)")
                };
                if badge.clicked() {
                    app.actions.push(Action::WatchStream(user));
                }
            }
        }
        // Grey when the person chose it, red when the server did, as
        // Discord's list.
        let y = rect.center().y;
        if state.video {
            paint_icon(ui, Icon::Video, pos2(right - 8.0, y), 16.0, palette.icon);
            right -= 22.0;
        }
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
    // Offered for a while after joining, a ring counting it down.
    if let Some(since) = app
        .call
        .as_ref()
        .filter(|c| c.channel == channel)
        .map(|c| c.since)
        && guild.is_some()
    {
        let left = 1.0 - since.elapsed().as_secs_f32() / INVITE_FOR;
        if left > 0.0 {
            invite_row(app, ui, channel, left);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}

/// How long "Convidar para voz" stays after joining, in seconds.
const INVITE_FOR: f32 = 20.0;

/// "Convidar para voz" under the people in the user's own channel; its
/// ring is the time `left`, from 1 to 0.
fn invite_row(app: &mut App, ui: &mut Ui, channel: Id, left: f32) {
    let palette = app.palette;
    let (slot, response) = ui.allocate_exact_size(vec2(ui.available_width(), 35.0), Sense::click());
    let rect = Rect::from_min_max(
        pos2(slot.left() + 38.0, slot.top() + 1.0),
        pos2(slot.right() - 8.0, slot.bottom() - 1.0),
    );
    let lit = response.hovered();
    if lit {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.hover);
    }
    let tint = if lit { palette.strong } else { palette.muted };
    let center = pos2(rect.left() + 19.0, rect.center().y);
    ui.painter()
        .circle_stroke(center, 11.5, Stroke::new(1.5, palette.selected));
    let steps = 48;
    let arc: Vec<egui::Pos2> = (0..=((steps as f32 * left).ceil() as usize))
        .map(|i| {
            let angle = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * (i as f32 / steps as f32).min(left);
            center + 11.5 * vec2(angle.cos(), angle.sin())
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(arc, Stroke::new(1.5, tint)));
    paint_icon(ui, Icon::UserPlus, center, 13.0, tint);
    ui.painter().text(
        pos2(rect.left() + 41.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Convidar para voz",
        font(Weight::Medium, 15.0),
        tint,
    );
    paint_icon(
        ui,
        Icon::ChevronRight,
        pos2(rect.right() - 12.0, rect.center().y),
        16.0,
        tint,
    );
    if response.clicked() {
        app.actions
            .push(Action::Admin(crate::admin::AdminAction::InviteTo(channel)));
    }
}

/// The right-click menu on someone in a voice channel, as Discord's: the
/// person, their volume here, and for moderators the server's mute,
/// deafen, disconnect and move. The moderation follows what the voice
/// channel allows, not only the server's roles.
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
    let allowed = app
        .model
        .channel(channel)
        .filter(|_| guild.is_some())
        .map_or(0, |c| app.model.channel_permissions(c));
    let (mute, deafen, moving) = (
        allowed & permission::MUTE_MEMBERS != 0,
        allowed & permission::DEAFEN_MEMBERS != 0,
        allowed & permission::MOVE_MEMBERS != 0,
    );
    let can = |bit| guild.is_some_and(|g| app.model.can(g, bit));
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
        ui.set_width(MENU_WIDTH);
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        if menu_item(ui, &palette, "Perfil", palette.text).clicked() {
            chosen.push(Action::Chat(ChatAction::ShowProfile {
                user,
                anchor,
                side: Side::Right,
            }));
            ui.close();
        }
        if menu_item(ui, &palette, "Mencionar", palette.text).clicked() {
            chosen.push(Action::Chat(ChatAction::Mention(user)));
            ui.close();
        }
        if !me && menu_item(ui, &palette, "Mensagem", palette.text).clicked() {
            chosen.push(Action::Chat(ChatAction::MessageUser {
                user,
                text: String::new(),
            }));
            ui.close();
        }
        menu_line(ui, &palette);
        if me {
            if check_item(ui, &palette, "Silenciar", muted, palette.text) {
                chosen.push(Action::ToggleMute);
            }
            if check_item(ui, &palette, "Ensurdecer", deafened, palette.text) {
                chosen.push(Action::ToggleDeafen);
            }
        } else {
            if let Some(level) = volume_slider(ui, &palette, volume) {
                chosen.push(Action::SetUserVolume(user, level));
            }
            menu_line(ui, &palette);
            if check_item(ui, &palette, "Silenciar", local, palette.text) {
                chosen.push(Action::ToggleLocalMute(user));
            }
        }
        if let Some(guild) = guild
            && (kick || ban)
        {
            if kick
                && menu_item(ui, &palette, &format!("Expulsar {name}"), palette.danger).clicked()
            {
                chosen.push(Action::Admin(AdminAction::Open(Modal::Kick {
                    guild,
                    user,
                })));
                ui.close();
            }
            if ban && menu_item(ui, &palette, &format!("Banir {name}"), palette.danger).clicked() {
                chosen.push(Action::Admin(AdminAction::Open(Modal::Ban { guild, user })));
                ui.close();
            }
        }
        if let Some(guild) = guild
            && (mute || deafen || moving)
        {
            menu_line(ui, &palette);
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
                    "Silenciar voz no servidor",
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
                    "Desativar áudio no servidor",
                    state.deaf,
                    palette.danger,
                )
            {
                chosen.push(moderate(json!({ "deaf": !state.deaf })));
            }
            if moving {
                if !elsewhere.is_empty() {
                    let item = submenu_item(ui, &palette, "Mover para");
                    egui::containers::menu::SubMenu::new().show(ui, &item, |ui| {
                        ui.set_width(MENU_WIDTH - 40.0);
                        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                        for (id, label) in &elsewhere {
                            if menu_item(ui, &palette, label, palette.text).clicked() {
                                chosen.push(moderate(json!({ "channel_id": id.to_string() })));
                                ui.close();
                            }
                        }
                    });
                }
                if menu_item(ui, &palette, "Desconectar", palette.danger).clicked() {
                    chosen.push(moderate(json!({ "channel_id": null })));
                    ui.close();
                }
            }
        }
        menu_line(ui, &palette);
        if menu_item(ui, &palette, "Copiar ID do usuário", palette.text).clicked() {
            chosen.push(Action::Chat(ChatAction::Copy(user.to_string())));
            ui.close();
        }
    });
    app.actions.extend(chosen);
}

/// The width of Discord's right-click menus.
const MENU_WIDTH: f32 = 220.0;
/// The height of a row in them.
const MENU_ROW: f32 = 32.0;

/// A row of Discord's menu: the label, grey under the pointer.
fn menu_row(
    ui: &mut Ui,
    palette: &crate::theme::Palette,
    label: &str,
    color: Color32,
) -> (Rect, egui::Response) {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), MENU_ROW), Sense::click());
    let lit = response.hovered() || response.has_focus();
    if lit {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.hover);
    }
    Elide::new(ui, rect.right() - 34.0).text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label.to_owned(),
        font(Weight::Medium, 14.0),
        if lit && color == palette.text {
            palette.strong
        } else {
            color
        },
    );
    (
        rect,
        response.on_hover_cursor(egui::CursorIcon::PointingHand),
    )
}

pub fn menu_item(
    ui: &mut Ui,
    palette: &crate::theme::Palette,
    label: &str,
    color: Color32,
) -> egui::Response {
    menu_row(ui, palette, label, color).1
}

/// A row that opens another menu, with the arrow on the right.
fn submenu_item(ui: &mut Ui, palette: &crate::theme::Palette, label: &str) -> egui::Response {
    let (rect, response) = menu_row(ui, palette, label, palette.text);
    paint_icon(
        ui,
        Icon::ChevronRight,
        pos2(rect.right() - 16.0, rect.center().y),
        16.0,
        palette.text,
    );
    response
}

/// The thin line between the menu's groups.
fn menu_line(ui: &mut Ui, palette: &crate::theme::Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(
        (rect.left() + 4.0)..=(rect.right() - 4.0),
        rect.center().y,
        Stroke::new(1.0, palette.divider),
    );
}

/// "Volume do usuário" and Discord's slider: blue up to a white knob,
/// up to 200%. The new level while it is dragged or clicked.
fn volume_slider(ui: &mut Ui, palette: &crate::theme::Palette, volume: u16) -> Option<u16> {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Volume do usuário",
        font(Weight::Medium, 14.0),
        palette.text,
    );
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click_and_drag());
    let track = Rect::from_min_max(
        pos2(rect.left() + 10.0, rect.center().y - 4.0),
        pos2(rect.right() - 10.0, rect.center().y + 4.0),
    );
    let mut level = f32::from(volume.min(200)) / 200.0;
    let mut changed = None;
    if let Some(pos) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        level = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let value = (level * 200.0).round() as u16;
        if value != volume {
            changed = Some(value);
        }
    }
    let knob = pos2(track.left() + level * track.width(), track.center().y);
    ui.painter()
        .rect_filled(track, CornerRadius::same(4), palette.divider);
    ui.painter().rect_filled(
        Rect::from_min_max(track.min, pos2(knob.x, track.max.y)),
        CornerRadius::same(4),
        palette.accent,
    );
    ui.painter().circle_filled(knob, 9.0, Color32::WHITE);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() || response.dragged() {
        response.on_hover_text_at_pointer(format!("{}%", (level * 200.0).round()));
    }
    changed
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
    let (rect, response) = menu_row(ui, palette, label, color);
    let tick = Rect::from_center_size(
        pos2(rect.right() - 18.0, rect.center().y),
        Vec2::splat(20.0),
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
    response.clicked()
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
    let noise_icon = if suppressing {
        Icon::NoiseSuppression
    } else {
        Icon::NoiseSuppressionOff
    };
    for (button, icon) in [(&noise, noise_icon), (&hang, Icon::HangUp)] {
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
    let noise = noise.on_hover_text("Supressão de ruído");
    noise_popup(app, &noise, suppressing);
    if hang.on_hover_text("Desconectar").clicked() {
        app.actions.push(Action::LeaveVoice);
    }
    // Camera, screen, activities and soundboard, as Discord's row. Camera
    // and screen work on the user's own video server; the rest are not in
    // Discofast yet, so drawn dim.
    let row = Rect::from_min_max(
        pos2(rect.left() + 10.0, badge.bottom() + 8.0),
        pos2(rect.right() - 10.0, badge.bottom() + 40.0),
    );
    let gap = 8.0;
    let width = (row.width() - gap * 3.0) / 4.0;
    let room = app.call.as_ref().and_then(|c| c.room.as_ref());
    let ready = room.is_some_and(|r| r.state == State::Connected);
    let room_error = room.and_then(|r| match &r.state {
        State::Failed(e) => Some(e.clone()),
        _ => None,
    });
    let (camera, screen) = room.map_or((false, false), |r| (r.camera, r.screen));
    let buttons = [
        (
            Icon::Video,
            camera,
            if camera {
                "Desativar câmera"
            } else {
                "Ativar câmera"
            },
        ),
        (
            Icon::ScreenShare,
            screen,
            if screen {
                "Parar de compartilhar"
            } else {
                "Compartilhar sua tela"
            },
        ),
        (Icon::Activities, false, "Iniciar uma atividade"),
        (Icon::Soundboard, false, "Abrir painel de efeitos sonoros"),
    ];
    for (i, (icon, on, hint)) in buttons.into_iter().enumerate() {
        let at = Rect::from_min_size(
            pos2(row.left() + i as f32 * (width + gap), row.top()),
            vec2(width, row.height()),
        );
        let kind = match icon {
            Icon::Video => Some(crate::room::Kind::Camera),
            Icon::ScreenShare => Some(crate::room::Kind::Screen),
            _ => None,
        };
        let usable = kind.is_some() && ready;
        let sense = if usable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(at, ui.id().with(("call-row", i)), sense);
        let fill = if on {
            TALKING
        } else if usable && response.hovered() {
            palette.hover
        } else {
            crate::theme::mix(palette.user_panel, palette.hover, 0.6)
        };
        ui.painter().rect_filled(at, CornerRadius::same(6), fill);
        let tint = match (on, usable) {
            (true, _) => Color32::WHITE,
            (false, true) => palette.icon,
            (false, false) => palette.icon.gamma_multiply(0.5),
        };
        paint_icon(ui, icon, at.center(), 20.0, tint);
        let response = match (&kind, usable, &room_error) {
            (Some(_), true, _) => response.on_hover_text(hint),
            (Some(_), false, Some(error)) => {
                response.on_hover_text(format!("{hint}: o servidor de vídeo falhou ({error})"))
            }
            (Some(_), false, None) if room.is_some() => {
                response.on_hover_text(format!("{hint} (conectando ao servidor de vídeo)"))
            }
            (Some(_), false, None) if crate::room::AVAILABLE => response.on_hover_text(format!(
                "{hint}: configure seu servidor de vídeo em Voz e vídeo"
            )),
            _ => response.on_hover_text(format!("{hint} (ainda não disponível no Discofast)")),
        };
        if let Some(kind) = kind
            && response.clicked()
        {
            app.actions.push(Action::ToggleVideo(kind));
        }
    }
}

/// The call in the middle: a tile per person, the talking ones outlined in
/// green, and the buttons at the bottom.
pub fn view(app: &mut App, ui: &mut Ui) {
    if let Some(room) = app.call.as_mut().and_then(|c| c.room.as_mut()) {
        room.refresh(ui.ctx());
    }
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
    let watched = call.watching.as_ref().map(|w| w.user);
    let focus = call.room.as_ref().and_then(|r| r.focus);
    let mut area = Rect::from_min_max(
        pos2(rect.left() + 16.0, rect.top() + 56.0),
        pos2(rect.right() - 16.0, rect.bottom() - 96.0),
    );
    // A watched share takes the stage, with everyone in a strip under it,
    // as Discord's focus view.
    let stage = Rect::from_min_max(area.min, pos2(area.right(), area.bottom() - 124.0));
    if let Some(streamer) = watched {
        stream_stage(app, ui, stage, guild, streamer);
        area = Rect::from_min_max(pos2(area.left(), stage.bottom() + 12.0), area.max);
    } else if let Some(user) = focus {
        own_stage(app, ui, stage, guild, user);
        area = Rect::from_min_max(pos2(area.left(), stage.bottom() + 12.0), area.max);
    }
    // Each person's tile, and beside it a tile for each share they have
    // that is not on the stage: Discord's, and one on the own server.
    let room = app.call.as_ref().and_then(|c| c.room.as_ref());
    let mut entries: Vec<(Id, Entry)> = Vec::new();
    for (user, state) in &people {
        entries.push((*user, Entry::Person(state)));
        if state.stream && watched != Some(*user) {
            entries.push((*user, Entry::Stream));
        }
        if room.is_some_and(|r| r.sends(*user, crate::room::Kind::Screen))
            && (watched.is_some() || focus != Some(*user))
        {
            entries.push((*user, Entry::OwnScreen));
        }
    }
    let tiles = grid(area, entries.len().max(1));
    for ((user, entry), tile) in entries.into_iter().zip(tiles) {
        let state = match entry {
            Entry::Person(state) => state,
            Entry::Stream => {
                stream_tile(app, ui, tile, guild, user);
                continue;
            }
            Entry::OwnScreen => {
                own_screen_tile(app, ui, tile, guild, user);
                continue;
            }
        };
        let person = app.model.user(user);
        // The avatar's own colour, as Discord's tiles; until it is known,
        // the colour behind the initial.
        let fill = person.map_or(palette.sidebar, |p| {
            app.picture_color(p.picture.as_deref())
                .unwrap_or_else(|| p.color().gamma_multiply(0.55))
        });
        ui.painter().rect_filled(tile, CornerRadius::same(8), fill);
        let radius = (tile.height() * 0.22).clamp(20.0, 60.0);
        let camera = app
            .call
            .as_ref()
            .and_then(|c| c.room.as_ref())
            .and_then(|r| r.video(user, crate::room::Kind::Camera));
        match (camera, person) {
            (Some(texture), _) => paint_video(ui, texture, tile, true),
            (None, Some(person)) => avatar(ui, app, person, tile.center(), radius, None),
            (None, None) => {}
        }
        if app.talking(user) {
            ui.painter().rect_stroke(
                tile,
                CornerRadius::same(8),
                Stroke::new(3.0, TALKING),
                egui::StrokeKind::Inside,
            );
        }
        // The name, and how they hear and speak, at the bottom left.
        let label = app.model.name_in(guild, user);
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
    controls(app, ui, pos2(rect.center().x, rect.bottom() - 48.0));
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

/// The call's bottom bar, as Discord's: the microphone with its menu and
/// the camera, then activities, the soundboard and more, then the red
/// hang-up, each group on a dark rounded tile.
fn controls(app: &mut App, ui: &mut Ui, center: egui::Pos2) {
    let palette = app.palette;
    const BUTTON: f32 = 40.0;
    const PAD: f32 = 4.0;
    const GAP: f32 = 12.0;
    let tile = Color32::from_rgb(0x1a, 0x1a, 0x1e);
    let hover = Color32::from_rgb(0x2e, 0x2f, 0x35);
    let first = PAD * 2.0 + BUTTON + 20.0 + BUTTON;
    let second = PAD * 2.0 + BUTTON * 3.0;
    let hang = 72.0;
    let total = first + GAP + second + GAP + hang;
    let top = center.y - (BUTTON + PAD * 2.0) / 2.0;
    let height = BUTTON + PAD * 2.0;
    let mut x = center.x - total / 2.0;
    let group = |ui: &Ui, x: f32, width: f32| {
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(x, top), vec2(width, height)),
            CornerRadius::same(12),
            tile,
        );
    };
    let button =
        |ui: &mut Ui, at: Rect, key: &str, icon: Icon, tint: Color32, fill: Option<Color32>| {
            let response = ui.interact(at, ui.id().with(("call-control", key)), Sense::click());
            let fill = fill.or(response.hovered().then_some(hover));
            if let Some(fill) = fill {
                ui.painter().rect_filled(at, CornerRadius::same(8), fill);
            }
            paint_icon(ui, icon, at.center(), 20.0, tint);
            response
        };
    let unavailable = Color32::from_gray(110);

    group(ui, x, first);
    let muted = app.settings.muted;
    let mic = Rect::from_min_size(pos2(x + PAD, top + PAD), Vec2::splat(BUTTON));
    let (icon, tint, fill) = if muted {
        (
            Icon::MicOff,
            palette.danger,
            Some(Color32::from_rgb(0x3a, 0x1c, 0x1f)),
        )
    } else {
        (Icon::Mic, Color32::WHITE, None)
    };
    let response = button(ui, mic, "mic", icon, tint, fill);
    let hint = if muted {
        "Ativar microfone"
    } else {
        "Silenciar"
    };
    if response.on_hover_text(hint).clicked() {
        app.actions.push(Action::ToggleMute);
    }
    let chevron = Rect::from_min_size(pos2(mic.right(), top + PAD), vec2(20.0, BUTTON));
    let more_mic = button(
        ui,
        chevron,
        "mic-more",
        Icon::ChevronDown,
        palette.icon,
        None,
    );
    let mut chosen = None;
    let deaf = app.settings.deafened;
    egui::Popup::menu(&more_mic).show(|ui| {
        ui.set_min_width(200.0);
        if ui
            .button(if deaf {
                "Desativar ensurdecer"
            } else {
                "Ensurdecer"
            })
            .clicked()
        {
            chosen = Some(Action::ToggleDeafen);
            ui.close();
        }
        if ui.button("Configurações de voz").clicked() {
            chosen = Some(Action::OpenSettings(crate::app::SettingsPage::Voice));
            ui.close();
        }
    });
    if let Some(action) = chosen {
        app.actions.push(action);
    }
    let camera = Rect::from_min_size(pos2(chevron.right(), top + PAD), Vec2::splat(BUTTON));
    // The camera works on the user's own video server, once connected.
    let room = app.call.as_ref().and_then(|c| c.room.as_ref());
    let ready = room.is_some_and(|r| r.state == State::Connected);
    let on = room.is_some_and(|r| r.camera);
    let (tint, fill) = match (on, ready) {
        (true, _) => (Color32::WHITE, Some(TALKING)),
        (false, true) => (Color32::WHITE, None),
        (false, false) => (unavailable, None),
    };
    let hint = match (on, ready, room.is_some()) {
        (true, _, _) => "Desativar câmera",
        (false, true, _) => "Ativar câmera",
        (false, false, true) => "Ativar câmera (conectando ao servidor de vídeo)",
        (false, false, false) if crate::room::AVAILABLE => {
            "Ativar câmera: configure seu servidor de vídeo em Voz e vídeo"
        }
        _ => "Ativar câmera (ainda não disponível no Discofast)",
    };
    let response = button(ui, camera, "camera", Icon::Video, tint, fill).on_hover_text(hint);
    if ready && response.clicked() {
        app.actions
            .push(Action::ToggleVideo(crate::room::Kind::Camera));
    }
    x += first + GAP;

    group(ui, x, second);
    for (i, (icon, hint)) in [
        (Icon::Activities, "Iniciar uma atividade"),
        (Icon::Soundboard, "Abrir painel de efeitos sonoros"),
    ]
    .into_iter()
    .enumerate()
    {
        let at = Rect::from_min_size(
            pos2(x + PAD + i as f32 * BUTTON, top + PAD),
            Vec2::splat(BUTTON),
        );
        button(ui, at, hint, icon, unavailable, None)
            .on_hover_text(format!("{hint} (ainda não disponível no Discofast)"));
    }
    let more = Rect::from_min_size(pos2(x + PAD + 2.0 * BUTTON, top + PAD), Vec2::splat(BUTTON));
    let more = button(ui, more, "more", Icon::More, Color32::WHITE, None);
    let mut chosen = None;
    let suppressing = app.settings.noise_suppression > 0;
    egui::Popup::menu(&more).show(|ui| {
        ui.set_min_width(220.0);
        let label = if suppressing {
            "Desativar supressão de ruído"
        } else {
            "Ativar supressão de ruído"
        };
        if ui.button(label).clicked() {
            chosen = Some(Action::SetNoiseSuppression(if suppressing { 0 } else { 2 }));
            ui.close();
        }
        if ui.button("Configurações de voz").clicked() {
            chosen = Some(Action::OpenSettings(crate::app::SettingsPage::Voice));
            ui.close();
        }
    });
    if let Some(action) = chosen {
        app.actions.push(action);
    }
    x += second + GAP;

    let at = Rect::from_min_size(pos2(x, top), vec2(hang, height));
    let response = ui.interact(at, ui.id().with("call-hang-up"), Sense::click());
    let red = Color32::from_rgb(0xda, 0x37, 0x3c);
    let fill = if response.hovered() {
        red.gamma_multiply(0.85)
    } else {
        red
    };
    ui.painter().rect_filled(at, CornerRadius::same(12), fill);
    paint_icon(ui, Icon::HangUp, at.center(), 24.0, Color32::WHITE);
    if response.on_hover_text("Desconectar").clicked() {
        app.actions.push(Action::LeaveVoice);
    }
}

/// Discord's card on the call panel's noise button: the switch, what it
/// does, the microphone's level to try it on, and who makes it.
fn noise_popup(app: &mut App, button: &egui::Response, suppressing: bool) {
    let palette = app.palette;
    let level = app.call.as_ref().map(|c| c.shared.level());
    let mut chosen = None;
    egui::Popup::menu(button)
        .align(egui::RectAlign::TOP_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(0x1a, 0x1a, 0x1e))
                .corner_radius(CornerRadius::same(8))
                .stroke(Stroke::new(1.0, palette.selected))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(|ui| {
            ui.set_width(268.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Supressão de ruído")
                        .font(font(Weight::SemiBold, 17.0))
                        .color(palette.strong),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, response) = ui.allocate_exact_size(vec2(44.0, 26.0), Sense::click());
                    let t = ui.ctx().animate_bool(response.id, suppressing);
                    let fill = crate::theme::mix(palette.offline, palette.accent, t);
                    ui.painter().rect_filled(rect, CornerRadius::same(13), fill);
                    let knob = pos2(rect.left() + 13.0 + t * 18.0, rect.center().y);
                    ui.painter().circle_filled(knob, 10.0, Color32::WHITE);
                    if response.clicked() {
                        chosen = Some(Action::SetNoiseSuppression(if suppressing { 0 } else { 2 }));
                    }
                });
            });
            ui.label(
                egui::RichText::new(
                    "Ative a supressão de ruído! Tente fazer barulho, como bater palmas, enquanto você fala. Seus amigos só ouvirão a sua linda voz.",
                )
                .font(font(Weight::Regular, 15.0))
                .color(palette.text),
            );
            ui.label(
                egui::RichText::new("Teste do microfone")
                    .font(font(Weight::SemiBold, 16.0))
                    .color(palette.strong),
            );
            let testing_id = egui::Id::new("noise-popup-testing");
            let testing = ui.data(|d| d.get_temp::<bool>(testing_id).unwrap_or(false));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let label = if testing { "Parar" } else { "Testar" };
                let test = egui::Button::new(
                    egui::RichText::new(label)
                        .font(font(Weight::Medium, 15.0))
                        .color(palette.strong),
                )
                .fill(palette.hover)
                .corner_radius(CornerRadius::same(8))
                .min_size(vec2(68.0, 34.0));
                if ui
                    .add_enabled(level.is_some(), test)
                    .on_disabled_hover_text("Entre em uma chamada para testar.")
                    .clicked()
                {
                    ui.data_mut(|d| d.insert_temp(testing_id, !testing));
                }
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
                super::settings::meter(ui, &palette, rect, level.filter(|_| testing));
            });
            if testing {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(33));
            }
            ui.label(
                egui::RichText::new("Fornecido por")
                    .font(font(Weight::SemiBold, 16.0))
                    .color(palette.strong),
            );
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("DeepFilterNet")
                        .font(font(Weight::Bold, 17.0))
                        .color(palette.strong),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let more = ui.add(
                        egui::Label::new(
                            egui::RichText::new("Saiba mais")
                                .font(font(Weight::Medium, 14.0))
                                .color(Color32::from_rgb(0x00, 0xa8, 0xfc)),
                        )
                        .sense(Sense::click()),
                    );
                    if more.clicked() {
                        chosen = Some(Action::Chat(crate::chat::ChatAction::OpenUrl(
                            "https://github.com/Rikorose/DeepFilterNet".into(),
                        )));
                    }
                });
            });
        });
    if let Some(action) = chosen {
        app.actions.push(action);
    }
}

/// Tiles in a grid that fits the space, 16:9 each, centred.
fn grid(area: Rect, count: usize) -> Vec<Rect> {
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
    (0..count)
        .map(|i| {
            let (col, row) = (i % columns, i / columns);
            Rect::from_min_size(
                origin + vec2(col as f32 * (w + 8.0), row as f32 * (h + 8.0)),
                vec2(w, h),
            )
        })
        .collect()
}

/// Discord's red "AO VIVO" badge, its left edge at `at`. Returns its rect.
fn live_badge(ui: &Ui, palette: &crate::theme::Palette, at: egui::Pos2) -> Rect {
    let live =
        ui.painter()
            .layout_no_wrap("AO VIVO".into(), font(Weight::Bold, 12.0), Color32::WHITE);
    let pill = Rect::from_min_size(at, vec2(live.size().x + 12.0, 20.0));
    ui.painter()
        .rect_filled(pill, CornerRadius::same(4), palette.danger);
    ui.painter().galley(
        pill.min + vec2(6.0, 10.0 - live.size().y / 2.0),
        live,
        Color32::WHITE,
    );
    pill
}

/// A share not being watched: who it is and Discord's "Assistir
/// transmissão" button.
fn stream_tile(app: &mut App, ui: &mut Ui, tile: Rect, guild: Option<Id>, user: Id) {
    let palette = app.palette;
    ui.painter()
        .rect_filled(tile, CornerRadius::same(8), Color32::from_gray(24));
    live_badge(ui, &palette, tile.min + vec2(8.0, 8.0));
    let name = app.model.name_in(guild, user);
    Elide::new(ui, tile.right() - 8.0).text(
        pos2(tile.left() + 10.0, tile.bottom() - 18.0),
        Align2::LEFT_CENTER,
        name,
        font(Weight::SemiBold, 14.0),
        Color32::WHITE,
    );
    let label = ui.painter().layout_no_wrap(
        "Assistir transmissão".into(),
        font(Weight::SemiBold, 14.0),
        Color32::WHITE,
    );
    let button = Rect::from_center_size(tile.center(), label.size() + vec2(24.0, 14.0));
    let response = ui.interact(button, ui.id().with(("watch", user)), Sense::click());
    let fill = match (crate::call::CAN_WATCH, response.hovered()) {
        (false, _) => Color32::from_gray(60),
        (true, false) => Color32::from_gray(80),
        (true, true) => Color32::from_gray(100),
    };
    ui.painter()
        .rect_filled(button, CornerRadius::same(6), fill);
    let tint = if crate::call::CAN_WATCH {
        Color32::WHITE
    } else {
        Color32::from_gray(150)
    };
    ui.painter()
        .galley(button.center() - label.size() / 2.0, label, tint);
    if !crate::call::CAN_WATCH {
        response.on_hover_text("Ainda não disponível no Discofast");
        return;
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        app.actions.push(Action::WatchStream(user));
    }
}

/// The watched share, large: its newest picture, or how far the
/// connection has got, and a button to stop watching.
fn stream_stage(app: &mut App, ui: &mut Ui, stage: Rect, guild: Option<Id>, user: Id) {
    let palette = app.palette;
    let Some(watch) = app.call.as_mut().and_then(|c| c.watching.as_mut()) else {
        return;
    };
    if let Some(frame) = watch.screen.take() {
        let image =
            egui::ColorImage::from_rgba_unmultiplied([frame.width, frame.height], &frame.rgba);
        match &mut watch.texture {
            Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
            None => {
                watch.texture = Some(ui.ctx().load_texture(
                    "screen-share",
                    image,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
    }
    ui.painter()
        .rect_filled(stage, CornerRadius::same(8), Color32::from_gray(16));
    match (&watch.texture, &watch.state) {
        (Some(texture), _) => {
            let size = texture.size_vec2();
            let scale = (stage.width() / size.x).min(stage.height() / size.y);
            let shown = Rect::from_center_size(stage.center(), size * scale);
            ui.painter().image(
                texture.id(),
                shown,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        (None, State::Failed(error)) => {
            ui.painter().text(
                stage.center(),
                Align2::CENTER_CENTER,
                format!("Não foi possível assistir: {error}"),
                font(Weight::Medium, 14.0),
                palette.danger,
            );
        }
        (None, _) => {
            ui.painter().text(
                stage.center(),
                Align2::CENTER_CENTER,
                "Conectando à transmissão...",
                font(Weight::Medium, 14.0),
                Color32::from_gray(200),
            );
        }
    }
    live_badge(ui, &palette, stage.min + vec2(10.0, 10.0));
    let name = app.model.name_in(guild, user);
    let label = ui
        .painter()
        .layout_no_wrap(name, font(Weight::SemiBold, 14.0), Color32::WHITE);
    let pill = Rect::from_min_size(
        pos2(stage.left() + 10.0, stage.bottom() - 34.0),
        label.size() + vec2(16.0, 8.0),
    );
    ui.painter()
        .rect_filled(pill, CornerRadius::same(4), Color32::from_black_alpha(150));
    ui.painter()
        .galley(pill.min + vec2(8.0, 4.0), label, Color32::WHITE);
    let close = Rect::from_center_size(
        pos2(stage.right() - 24.0, stage.top() + 24.0),
        Vec2::splat(32.0),
    );
    let response = ui.interact(close, ui.id().with("stop-watching"), Sense::click());
    let fill = if response.hovered() {
        Color32::from_black_alpha(220)
    } else {
        Color32::from_black_alpha(150)
    };
    ui.painter().circle_filled(close.center(), 16.0, fill);
    paint_icon(ui, Icon::Close, close.center(), 18.0, Color32::WHITE);
    if response.on_hover_text("Parar de assistir").clicked() {
        app.actions.push(Action::StopWatching);
    }
}

/// What a tile in the call shows.
enum Entry<'a> {
    Person(&'a VoiceMember),
    /// Someone's Discord screen share, not watched.
    Stream,
    /// Someone's screen on the own video server, not on the stage.
    OwnScreen,
}

/// A video in `rect`, kept to its shape: filling it and cut at the edges
/// for a camera, as Discord's tiles, or whole inside it for a screen.
fn paint_video(ui: &Ui, texture: &egui::TextureHandle, rect: Rect, fill: bool) {
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let (sx, sy) = (rect.width() / size.x, rect.height() / size.y);
    let scale = if fill { sx.max(sy) } else { sx.min(sy) };
    let shown = Rect::from_center_size(rect.center(), size * scale);
    let visible = shown.intersect(rect);
    // The part of the picture that shows, as texture coordinates.
    let uv = Rect::from_min_max(
        pos2(
            (visible.left() - shown.left()) / shown.width(),
            (visible.top() - shown.top()) / shown.height(),
        ),
        pos2(
            (visible.right() - shown.left()) / shown.width(),
            (visible.bottom() - shown.top()) / shown.height(),
        ),
    );
    ui.painter()
        .image(texture.id(), visible, uv, Color32::WHITE);
}

/// Someone's screen on the own video server, small; a click puts it on
/// the stage.
fn own_screen_tile(app: &mut App, ui: &mut Ui, tile: Rect, guild: Option<Id>, user: Id) {
    let palette = app.palette;
    ui.painter()
        .rect_filled(tile, CornerRadius::same(8), Color32::from_gray(24));
    if let Some(texture) = app
        .call
        .as_ref()
        .and_then(|c| c.room.as_ref())
        .and_then(|r| r.video(user, crate::room::Kind::Screen))
    {
        paint_video(ui, texture, tile, false);
    }
    live_badge(ui, &palette, tile.min + vec2(8.0, 8.0));
    let name = app.model.name_in(guild, user);
    let label = ui
        .painter()
        .layout_no_wrap(name, font(Weight::SemiBold, 14.0), Color32::WHITE);
    let pill = Rect::from_min_size(
        pos2(tile.left() + 8.0, tile.bottom() - 32.0),
        label.size() + vec2(16.0, 8.0),
    );
    ui.painter()
        .rect_filled(pill, CornerRadius::same(4), Color32::from_black_alpha(150));
    ui.painter()
        .galley(pill.min + vec2(8.0, 4.0), label, Color32::WHITE);
    let response = ui.interact(tile, ui.id().with(("own-screen", user)), Sense::click());
    if response.hovered() {
        ui.painter().rect_stroke(
            tile,
            CornerRadius::same(8),
            Stroke::new(2.0, Color32::from_gray(120)),
            egui::StrokeKind::Inside,
        );
    }
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Ver em destaque")
        .clicked()
    {
        app.actions.push(Action::FocusScreen(Some(user)));
    }
}

/// Someone's screen on the own video server, large, with a button to put
/// it back among the tiles.
fn own_stage(app: &mut App, ui: &mut Ui, stage: Rect, guild: Option<Id>, user: Id) {
    let palette = app.palette;
    ui.painter()
        .rect_filled(stage, CornerRadius::same(8), Color32::from_gray(16));
    if let Some(texture) = app
        .call
        .as_ref()
        .and_then(|c| c.room.as_ref())
        .and_then(|r| r.video(user, crate::room::Kind::Screen))
    {
        paint_video(ui, texture, stage, false);
    }
    live_badge(ui, &palette, stage.min + vec2(10.0, 10.0));
    let name = app.model.name_in(guild, user);
    let label = ui
        .painter()
        .layout_no_wrap(name, font(Weight::SemiBold, 14.0), Color32::WHITE);
    let pill = Rect::from_min_size(
        pos2(stage.left() + 10.0, stage.bottom() - 34.0),
        label.size() + vec2(16.0, 8.0),
    );
    ui.painter()
        .rect_filled(pill, CornerRadius::same(4), Color32::from_black_alpha(150));
    ui.painter()
        .galley(pill.min + vec2(8.0, 4.0), label, Color32::WHITE);
    let close = Rect::from_center_size(
        pos2(stage.right() - 24.0, stage.top() + 24.0),
        Vec2::splat(32.0),
    );
    let response = ui.interact(close, ui.id().with("own-unfocus"), Sense::click());
    let fill = if response.hovered() {
        Color32::from_black_alpha(220)
    } else {
        Color32::from_black_alpha(150)
    };
    ui.painter().circle_filled(close.center(), 16.0, fill);
    paint_icon(ui, Icon::Close, close.center(), 18.0, Color32::WHITE);
    if response.on_hover_text("Sair do destaque").clicked() {
        app.actions.push(Action::FocusScreen(None));
    }
}
