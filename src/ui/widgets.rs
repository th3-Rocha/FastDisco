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

/// An icon, moving as the rcord pack's do: each its own little motion
/// while the pointer is over it, a press that sinks it, and a pop when it
/// changes (a microphone muted or unmuted).
pub fn paint_icon(ui: &Ui, icon: Icon, center: Pos2, size: f32, tint: Color32) {
    let area = Rect::from_center_size(center, Vec2::splat(size.max(20.0) + 10.0));
    let hovered = ui.rect_contains_pointer(area);
    paint_icon_state(ui, icon, center, size, tint, hovered);
}

/// As [`paint_icon`], with the hover of the whole row or button that holds
/// the icon, as the pack's containers trigger their icon.
pub fn paint_icon_state(ui: &Ui, icon: Icon, center: Pos2, size: f32, tint: Color32, hovered: bool) {
    let ctx = ui.ctx();
    // Keyed by where it sits, so each copy of an icon moves on its own.
    let id = egui::Id::new(("icon-motion", (center.x / 2.0) as i32, (center.y / 2.0) as i32));
    let (duration, _) = motion(icon, 0.0);
    let t = ctx.animate_bool_with_time_and_easing(id, hovered, duration, ease_out);
    let pressed = hovered && ctx.input(|i| i.pointer.primary_down());
    let press = ctx.animate_bool_with_time(id.with("press"), pressed, 0.1);
    // A different icon in the same place (muted, unmuted): pop in.
    let last: Option<Icon> = ctx.data(|d| d.get_temp(id.with("icon")));
    if last != Some(icon) {
        ctx.data_mut(|d| {
            d.insert_temp(id.with("icon"), icon);
            if last.is_some() {
                d.insert_temp(id.with("changed"), ctx.input(|i| i.time));
            }
        });
    }
    let pop = ctx
        .data(|d| d.get_temp::<f64>(id.with("changed")))
        .map_or(1.0, |at| {
            let p = ((ctx.input(|i| i.time) - at) / 0.26).clamp(0.0, 1.0) as f32;
            if p < 1.0 {
                ctx.request_repaint();
            }
            0.8 + 0.2 * ease_spring(p)
        });
    let (_, shape) = motion(icon, t);
    let scale = shape.scale * (1.0 - 0.08 * press) * pop;
    let center = center + shape.offset * (size / 24.0) + vec2(0.0, press);
    let side = size * scale;
    let rect = Rect::from_center_size(center, Vec2::splat(side));
    icon.image(tint, side)
        .rotate(shape.angle.to_radians(), Vec2::splat(0.5))
        .paint_at(ui, rect);
}

/// Where an icon is at `t` of its hover motion.
struct Shape {
    angle: f32,
    scale: f32,
    offset: Vec2,
}

/// The pack's hover motions, as far as a whole icon can move: how long it
/// takes, and its shape at `t` from rest (0) to hovered (1).
fn motion(icon: Icon, t: f32) -> (f32, Shape) {
    use std::f32::consts::PI;
    // A swing that ends where it began, as the pack's keyframes.
    let swing = |degrees: f32, turns: f32| (t * turns * PI).sin() * (1.0 - t) * degrees;
    let (fast, med, slow) = (0.1, 0.17, 0.32);
    let still = Shape { angle: 0.0, scale: 1.0, offset: Vec2::ZERO };
    let grow = |by: f32| Shape { scale: 1.0 + by * t, ..still };
    let turn = |degrees: f32| Shape { angle: degrees * t, ..still };
    let shift = |x: f32, y: f32| Shape { offset: vec2(x, y) * t, ..still };
    match icon {
        // Channels and the rail's corner badges stay still.
        Icon::Hash | Icon::Stage | Icon::Forum => (med, still),
        Icon::Settings => (slow, turn(90.0)),
        Icon::Bell => (slow, Shape { angle: swing(16.0, 3.0), ..still }),
        Icon::Users => (slow, Shape { angle: swing(10.0, 2.0), ..still }),
        Icon::Plus | Icon::Close | Icon::PlusCircle => (med, turn(90.0)),
        Icon::UserPlus | Icon::AddGroup => (med, grow(0.08)),
        Icon::MessageCircle | Icon::Threads => {
            (med, Shape { angle: -6.0 * t, scale: 1.0 + 0.08 * t, offset: Vec2::ZERO })
        }
        Icon::More => (med, grow(0.12)),
        Icon::Pencil => (slow, Shape { angle: swing(12.0, 4.0), ..still }),
        Icon::Palette => (med, turn(-14.0)),
        Icon::Compass => (slow, turn(-20.0)),
        Icon::Search => (med, grow(0.12)),
        Icon::Shield | Icon::ShieldLock => (med, grow(0.08)),
        Icon::Mic | Icon::MicOff | Icon::Headphones | Icon::HeadphonesOff => (fast, shift(0.0, -1.5)),
        Icon::LogOut | Icon::Forward | Icon::Send | Icon::External => (med, shift(2.0, 0.0)),
        Icon::Back | Icon::Reply | Icon::ReplyArrow => (med, shift(-2.0, 0.0)),
        Icon::Download => (med, shift(0.0, 2.0)),
        Icon::Speaker | Icon::Volume | Icon::VolumeOff | Icon::Soundboard => (med, grow(0.08)),
        Icon::Accessibility => (slow, Shape { offset: vec2(0.0, -(t * PI).sin() * 3.0), ..still }),
        Icon::Keyboard => (fast, shift(0.0, 1.0)),
        Icon::Gift => (med, Shape { angle: -8.0 * t, scale: 1.0 + 0.06 * t, offset: Vec2::ZERO }),
        Icon::Code => (med, grow(0.1)),
        Icon::Trash | Icon::Phone | Icon::HangUp => (slow, Shape { angle: swing(12.0, 3.0), ..still }),
        Icon::Star | Icon::StarFill | Icon::Sun => (slow, turn(45.0)),
        Icon::Dice => (slow, turn(90.0)),
        Icon::Moon => (med, turn(-15.0)),
        Icon::Pin => (med, turn(15.0)),
        Icon::Activities | Icon::Video | Icon::ScreenShare | Icon::Calendar => (med, grow(0.08)),
        _ => (med, grow(0.06)),
    }
}

/// The pack's `out` easing, cubic-bezier(.2,0,0,1), near enough.
fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(4)
}

/// The pack's spring, cubic-bezier(.34,1.56,.64,1): past the end, then back.
fn ease_spring(t: f32) -> f32 {
    let c = 1.70158 * 1.2;
    1.0 + (c + 1.0) * (t - 1.0).powi(3) + c * (t - 1.0).powi(2)
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

/// "10:04" today, as Discord's redesign shows it, "Ontem às 22:50",
/// "21/10/2026 18:03".
pub fn timestamp(time: jiff::Timestamp) -> String {
    let zone = jiff::tz::TimeZone::system();
    let local = time.to_zoned(zone.clone());
    let today = jiff::Timestamp::now().to_zoned(zone).date();
    let date = local.date();
    if date == today {
        local.strftime("%H:%M").to_string()
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

/// A group's picture, or Discord's coloured circle with two people when it
/// has none, the colour following the group.
pub fn group_icon(ui: &Ui, app: &App, channel: &crate::model::Channel, center: Pos2, radius: f32) {
    let rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    if let Some(uri) = app.picture(channel.icon.as_deref()) {
        egui::Image::new(uri)
            .corner_radius(CornerRadius::same(radius as u8))
            .paint_at(ui, rect);
        return;
    }
    const COLORS: [Color32; 8] = [
        Color32::from_rgb(0x58, 0x65, 0xf2),
        Color32::from_rgb(0x3b, 0xa5, 0x5c),
        Color32::from_rgb(0xfa, 0xa6, 0x1a),
        Color32::from_rgb(0xed, 0x42, 0x45),
        Color32::from_rgb(0xeb, 0x45, 0x9e),
        Color32::from_rgb(0x1a, 0xbc, 0x9c),
        Color32::from_rgb(0x9b, 0x59, 0xb6),
        Color32::from_rgb(0x74, 0x7f, 0x8d),
    ];
    let color = COLORS[((channel.id >> 22) % COLORS.len() as u64) as usize];
    ui.painter().circle_filled(center, radius, color);
    paint_icon(ui, Icon::Users, center, radius * 1.1, Color32::WHITE);
}

/// The icon Discord draws before a channel of each kind.
pub fn channel_icon(kind: crate::model::ChannelKind) -> Icon {
    use crate::model::ChannelKind;
    match kind {
        ChannelKind::Voice => Icon::Speaker,
        ChannelKind::Stage => Icon::Stage,
        ChannelKind::Forum => Icon::Forum,
        ChannelKind::Media => Icon::Image,
        ChannelKind::Thread | ChannelKind::Other => Icon::Threads,
        ChannelKind::Group => Icon::Users,
        _ => Icon::Hash,
    }
}
