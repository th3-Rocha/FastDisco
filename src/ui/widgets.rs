//! Small pieces shared by the views.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Ui, Vec2, vec2};
use fastframe_fonts::Weight;

use crate::app::App;
use crate::icons::Icon;
use crate::model::{Status, User};
pub use crate::text::hyphens;
use crate::theme::Palette;

pub fn font(weight: Weight, size: f32) -> FontId {
    weight.font_id(size)
}

/// Paints single-line text that ends in "…" where it would pass `right`,
/// as Discord cuts long names.
pub struct Elide<'a> {
    ui: &'a Ui,
    right: f32,
}

impl<'a> Elide<'a> {
    pub fn new(ui: &'a Ui, right: f32) -> Self {
        Self { ui, right }
    }

    /// Like `Painter::text`, for left-anchored text; returns where it
    /// went.
    pub fn text(
        &self,
        pos: Pos2,
        anchor: Align2,
        text: impl ToString,
        font_id: FontId,
        color: Color32,
    ) -> Rect {
        let mut job = egui::text::LayoutJob::single_section(
            text.to_string(),
            egui::TextFormat {
                font_id,
                color,
                ..Default::default()
            },
        );
        job.wrap.max_width = (self.right - pos.x).max(0.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        let galley = self.ui.painter().layout_job(job);
        let rect = anchor.anchor_size(pos, galley.size());
        self.ui.painter().galley(rect.min, galley, color);
        rect
    }
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
    tinted_button(
        ui,
        palette,
        icon,
        size,
        active.then_some(palette.strong),
        hint,
    )
}

/// An icon button in a colour of its own, as the red of a muted
/// microphone; with no colour it brightens under the pointer.
pub fn tinted_button(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    size: f32,
    tint: Option<Color32>,
    hint: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hovered = response.hovered();
    if hovered && size >= 32.0 {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.hover);
    }
    let tint = tint.unwrap_or(if hovered {
        palette.strong
    } else {
        palette.icon
    });
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
pub fn avatar(ui: &Ui, app: &App, user: &User, center: Pos2, radius: f32, status: Option<Color32>) {
    let palette = &app.palette;
    let painter = ui.painter();
    painter.circle_filled(center, radius, user.color());
    match app.picture(user.picture.as_deref()) {
        Some(uri) => {
            let rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
            egui::Image::new(uri)
                .corner_radius(CornerRadius::same(radius as u8))
                .paint_at(ui, rect);
        }
        None => {
            let initial: String = user.name.chars().take(1).collect();
            painter.text(
                center,
                Align2::CENTER_CENTER,
                initial,
                font(Weight::SemiBold, radius * 0.9),
                Color32::WHITE,
            );
        }
    }
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

/// "Hoje às 10:04", "Ontem às 22:50", "21/10/2026 18:03".
pub fn timestamp(time: jiff::Timestamp) -> String {
    let zone = jiff::tz::TimeZone::system();
    let local = time.to_zoned(zone.clone());
    let today = jiff::Timestamp::now().to_zoned(zone).date();
    let date = local.date();
    if date == today {
        format!("Hoje às {}", local.strftime("%H:%M"))
    } else if today.yesterday().ok() == Some(date) {
        format!("Ontem às {}", local.strftime("%H:%M"))
    } else {
        local.strftime("%d/%m/%Y %H:%M").to_string()
    }
}

pub fn clock(time: jiff::Timestamp) -> String {
    time.to_zoned(jiff::tz::TimeZone::system())
        .strftime("%H:%M")
        .to_string()
}

/// The local day of a moment, for the dividers between days.
pub fn day(time: jiff::Timestamp) -> jiff::civil::Date {
    time.to_zoned(jiff::tz::TimeZone::system()).date()
}

/// "24 de outubro de 2026".
pub fn long_date(date: jiff::civil::Date) -> String {
    const MONTHS: [&str; 12] = [
        "janeiro",
        "fevereiro",
        "março",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ];
    format!(
        "{} de {} de {}",
        date.day(),
        MONTHS[date.month() as usize - 1],
        date.year()
    )
}
