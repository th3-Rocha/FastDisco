//! Voice, drawn: who is in each voice channel under it in the channel
//! list, the "Voz conectada" panel above the user panel, and the call's
//! tiles in the middle, as Discord shows them.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{Elide, avatar, font, paint_icon};
use crate::app::{Action, App};
use crate::call::State;
use crate::icons::Icon;
use crate::model::{Id, VoiceMember};
use crate::theme::SIDEBAR_WIDTH;

/// Discord's green for a voice that is talking.
const TALKING: Color32 = Color32::from_rgb(0x23, 0xa5, 0x5a);

/// A slashed icon: muted or deafened.
fn slashed(ui: &Ui, icon: Icon, center: egui::Pos2, size: f32, color: Color32) {
    paint_icon(ui, icon, center, size, color);
    let d = size * 0.45;
    ui.painter().line_segment(
        [center + vec2(-d, -d), center + vec2(d, d)],
        Stroke::new(2.0, color),
    );
}

/// The people in a voice channel, under its row.
pub fn members(app: &mut App, ui: &mut Ui, channel: Id) {
    let palette = app.palette;
    let guild = app.model.channel(channel).and_then(|c| c.guild);
    for (user, state) in app.model.in_voice(channel) {
        let (slot, response) = ui.allocate_exact_size(vec2(SIDEBAR_WIDTH, 30.0), Sense::click());
        let rect = Rect::from_min_max(
            pos2(slot.left() + 38.0, slot.top() + 1.0),
            pos2(slot.right() - 8.0, slot.bottom() - 1.0),
        );
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), palette.hover);
        }
        let center = pos2(rect.left() + 14.0, rect.center().y);
        if let Some(person) = app.model.user(user) {
            avatar(ui, app, person, center, 10.0, None);
        }
        if app.talking(user) {
            ui.painter()
                .circle_stroke(center, 11.5, Stroke::new(2.0, TALKING));
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
        if state.self_deaf || state.deaf {
            slashed(
                ui,
                Icon::Headphones,
                pos2(right - 8.0, rect.center().y),
                14.0,
                red,
            );
            right -= 20.0;
        }
        if state.self_mute || state.mute {
            slashed(ui, Icon::Mic, pos2(right - 8.0, rect.center().y), 14.0, red);
            right -= 20.0;
        }
        let name = app.model.name_in(guild, user);
        Elide::new(ui, right - 4.0).text(
            pos2(rect.left() + 30.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            font(Weight::Medium, 14.0),
            if response.hovered() {
                palette.text
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
    }
}

/// Above the user panel while in a call: the state in colour, where, and
/// the button that hangs up.
pub fn panel(app: &mut App, ui: &mut Ui) {
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
    egui::Panel::bottom("voice-panel")
        .resizable(false)
        .exact_size(56.0)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.user_panel)
                .inner_margin(Margin::symmetric(8, 0)),
        )
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.expand2(vec2(8.0, 0.0)).x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, palette.divider),
            );
            let info = Rect::from_min_max(
                pos2(rect.left(), rect.top() + 8.0),
                pos2(rect.right() - 44.0, rect.bottom() - 8.0),
            );
            let response = ui.interact(info, ui.id().with("voice-info"), Sense::click());
            paint_icon(
                ui,
                Icon::Signal,
                pos2(info.left() + 10.0, info.center().y - 8.0),
                16.0,
                color,
            );
            ui.painter().text(
                pos2(info.left() + 24.0, info.center().y - 8.0),
                Align2::LEFT_CENTER,
                label,
                font(Weight::SemiBold, 14.0),
                color,
            );
            Elide::new(ui, info.right()).text(
                pos2(info.left() + 6.0, info.center().y + 10.0),
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
            let hang = Rect::from_center_size(
                pos2(rect.right() - 20.0, rect.center().y),
                Vec2::splat(32.0),
            );
            let hang = ui.interact(hang, ui.id().with("hang-up"), Sense::click());
            if hang.hovered() {
                ui.painter()
                    .rect_filled(hang.rect, CornerRadius::same(4), palette.hover);
            }
            paint_icon(
                ui,
                Icon::PhoneOff,
                hang.rect.center(),
                20.0,
                if hang.hovered() {
                    palette.strong
                } else {
                    palette.icon
                },
            );
            if hang.on_hover_text("Desconectar").clicked() {
                app.actions.push(Action::LeaveVoice);
            }
        });
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
        let fill = person.map_or(palette.sidebar, |p| p.color().gamma_multiply(0.55));
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
            slashed(
                ui,
                Icon::Mic,
                pos2(x + 7.0, pill.center().y),
                14.0,
                palette.danger,
            );
            x += 20.0;
        }
        if state.self_deaf || state.deaf {
            slashed(
                ui,
                Icon::Headphones,
                pos2(x + 7.0, pill.center().y),
                14.0,
                palette.danger,
            );
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
        let tint = if off { palette.danger } else { Color32::WHITE };
        if off {
            slashed(ui, icon, c, 24.0, tint);
        } else {
            paint_icon(ui, icon, c, 24.0, tint);
        }
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
