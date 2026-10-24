//! Small pieces shared by the views.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Ui, Vec2, vec2};
use fastframe_fonts::Weight;

use crate::icons::Icon;
use crate::model::{Status, User};
use crate::theme::Palette;

pub fn font(weight: Weight, size: f32) -> FontId {
    weight.font_id(size)
}

pub fn paint_icon(ui: &Ui, icon: Icon, center: Pos2, size: f32, tint: Color32) {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    icon.image(tint, size).paint_at(ui, rect);
}

/// A square button that shows only an icon, brighter under the pointer.
pub fn icon_button(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    size: f32,
    active: bool,
    hint: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered();
    if hovered && size >= 32.0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.hover);
    }
    let tint = if active || hovered {
        palette.strong
    } else {
        palette.icon
    };
    paint_icon(ui, icon, rect.center(), 20.0, tint);
    response.on_hover_text(hint)
}

pub fn status_color(palette: &Palette, status: Status) -> Color32 {
    match status {
        Status::Online => palette.online,
        Status::Idle => palette.idle,
        Status::DoNotDisturb => palette.dnd,
        Status::Offline => palette.offline,
    }
}

/// A round avatar with the user's initial, and their status at the bottom
/// right cut out of `behind`.
pub fn avatar(
    ui: &Ui,
    palette: &Palette,
    user: &User,
    center: Pos2,
    radius: f32,
    status: Option<Color32>,
) {
    let painter = ui.painter();
    painter.circle_filled(center, radius, user.avatar);
    let initial: String = user.name.chars().take(1).collect();
    painter.text(
        center,
        Align2::CENTER_CENTER,
        initial,
        font(Weight::SemiBold, radius * 0.9),
        Color32::WHITE,
    );
    if let Some(behind) = status {
        let dot = (radius * 0.3).max(4.0);
        let at = center + vec2(radius * 0.7, radius * 0.7);
        painter.circle_filled(at, dot + 3.0, behind);
        let color = status_color(palette, user.status);
        match user.status {
            Status::Offline => {
                painter.circle_filled(at, dot, color);
                painter.circle_filled(at, dot * 0.5, behind);
            }
            Status::Idle => {
                painter.circle_filled(at, dot, color);
                painter.circle_filled(at - vec2(dot * 0.45, dot * 0.45), dot * 0.6, behind);
            }
            Status::DoNotDisturb => {
                painter.circle_filled(at, dot, color);
                let bar = Rect::from_center_size(at, vec2(dot * 1.2, dot * 0.45));
                painter.rect_filled(bar, CornerRadius::same(2), behind);
            }
            Status::Online => {
                painter.circle_filled(at, dot, color);
            }
        }
    }
}

/// The red pill with a count of mentions.
pub fn badge(ui: &Ui, palette: &Palette, right_center: Pos2, count: u32, ring: Option<Color32>) {
    let text = if count > 99 {
        "99+".to_owned()
    } else {
        count.to_string()
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text, font(Weight::Bold, 12.0), Color32::WHITE);
    let width = (galley.size().x + 10.0).max(16.0);
    let rect = Rect::from_min_size(right_center - vec2(width, 8.0), vec2(width, 16.0));
    if let Some(ring) = ring {
        ui.painter()
            .rect_filled(rect.expand(3.0), CornerRadius::same(11), ring);
    }
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), palette.danger);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

/// "Today at 10:04", "Yesterday at 22:50".
pub fn timestamp(minute: i32) -> String {
    let (day, minute) = if minute < 0 {
        ("Ontem", minute + 24 * 60)
    } else {
        ("Hoje", minute)
    };
    format!("{day} às {:02}:{:02}", minute / 60, minute % 60)
}

pub fn clock(minute: i32) -> String {
    let minute = minute.rem_euclid(24 * 60);
    format!("{:02}:{:02}", minute / 60, minute % 60)
}
