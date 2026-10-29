//! "Personalize o seu tema", as Discord's theme editor: dark or light,
//! up to five colours picked on a saturation square and a hue bar, the
//! colour's intensity, "Surpreenda-me!" and "Redefinir". Discord sells it
//! with Nitro; here it is free, and kept in settings.json.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use fastframe_fonts::Weight;

use super::widgets::{font, paint_icon};
use crate::app::{Action, App};
use crate::icons::Icon;
use crate::settings::ColorTheme;

const WIDTH: f32 = 320.0;
/// Discord's editor stops at five colours.
const MOST: usize = 5;

/// The panel over the left of the window while it is open.
pub fn show(app: &mut App, ui: &Ui) {
    if !app.customizer {
        return;
    }
    let palette = app.palette;
    let window = ui.ctx().content_rect();
    let theme = app.settings.color_theme.clone().unwrap_or_else(default_theme);
    let picked_id = egui::Id::new("customizer-picked");
    let mut picked: usize = ui.data(|d| d.get_temp(picked_id)).unwrap_or(0);
    picked = picked.min(theme.colors.len().saturating_sub(1));
    let mut changed: Option<ColorTheme> = None;
    let mut close = false;
    let mut reset = false;
    let rect = Rect::from_min_size(
        pos2(window.left() + 8.0, window.top() + crate::theme::TITLE_HEIGHT + 8.0),
        vec2(WIDTH, window.height() - crate::theme::TITLE_HEIGHT - 16.0),
    );
    egui::Area::new(egui::Id::new("customizer"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(palette.sidebar)
                .stroke(Stroke::new(1.0, palette.divider))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(16))
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(90),
                })
                .show(ui, |ui| {
                    ui.set_width(WIDTH - 32.0);
                    ui.set_height(rect.height() - 32.0);
                    ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
                    // Title and the close button.
                    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
                    ui.painter().text(
                        pos2(line.left(), line.center().y),
                        Align2::LEFT_CENTER,
                        "Personalize o seu tema",
                        font(Weight::SemiBold, 16.0),
                        palette.strong,
                    );
                    let x = Rect::from_center_size(pos2(line.right() - 12.0, line.center().y), vec2(24.0, 24.0));
                    let response = ui.interact(x, ui.id().with("close"), Sense::click());
                    paint_icon(ui, Icon::Close, x.center(), 20.0, if response.hovered() { palette.strong } else { palette.icon });
                    if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        close = true;
                    }
                    ui.add_space(8.0);
                    label(ui, &palette, "Aparência");
                    // Dark or light, two halves of one switch.
                    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                    ui.painter().rect_filled(bar, CornerRadius::same(8), palette.rail);
                    for (i, (icon, light)) in [(Icon::Moon, false), (Icon::Sun, true)].into_iter().enumerate() {
                        let half = Rect::from_min_size(
                            pos2(bar.left() + i as f32 * bar.width() / 2.0, bar.top()),
                            vec2(bar.width() / 2.0, bar.height()),
                        )
                        .shrink(4.0);
                        let response = ui.interact(half, ui.id().with(("look", i)), Sense::click());
                        if theme.light == light {
                            ui.painter().rect(half, CornerRadius::same(6), palette.selected, Stroke::new(1.0, palette.divider), egui::StrokeKind::Inside);
                        } else if response.hovered() {
                            ui.painter().rect_filled(half, CornerRadius::same(6), palette.hover);
                        }
                        paint_icon(ui, icon, half.center(), 18.0, if theme.light == light { palette.strong } else { palette.icon });
                        if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && theme.light != light {
                            changed = Some(ColorTheme { light, ..theme.clone() });
                        }
                    }
                    ui.add_space(8.0);
                    label(ui, &palette, "Cores");
                    let current = theme.colors.get(picked).copied().unwrap_or([0x58, 0x65, 0xf2]);
                    let hsv_id = egui::Id::new("customizer-hsv");
                    // The hue is kept apart, so a grey keeps the hue it had.
                    let mut hsv: [f32; 3] = ui
                        .data(|d| d.get_temp::<([u8; 3], [f32; 3])>(hsv_id))
                        .filter(|(rgb, _)| *rgb == current)
                        .map_or_else(|| to_hsv(current), |(_, hsv)| hsv);
                    let before = hsv;
                    saturation_square(ui, &palette, &mut hsv);
                    hue_bar(ui, &palette, &mut hsv);
                    // The colour's code.
                    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                    ui.painter().rect_filled(row, CornerRadius::same(8), palette.rail);
                    let swatch = Rect::from_min_size(row.min + vec2(8.0, 8.0), vec2(24.0, 24.0));
                    ui.painter().rect_filled(swatch, CornerRadius::same(4), rgb(current));
                    let code_id = egui::Id::new("customizer-code");
                    let mut code: String = ui
                        .data(|d| d.get_temp::<(usize, String)>(code_id))
                        .filter(|(i, _)| *i == picked)
                        .map_or_else(|| hex(current), |(_, c)| c);
                    let field = Rect::from_min_max(pos2(swatch.right() + 10.0, row.top() + 6.0), pos2(row.right() - 40.0, row.bottom() - 6.0));
                    let edit = ui.put(
                        field,
                        egui::TextEdit::singleline(&mut code)
                            .frame(Frame::NONE)
                            .font(font(Weight::Medium, 15.0))
                            .text_color(palette.text)
                            .vertical_align(egui::Align::Center),
                    );
                    if edit.changed() {
                        if let Some(color) = parse_hex(&code) {
                            hsv = to_hsv(color);
                        }
                    } else if !edit.has_focus() {
                        code = hex(current);
                    }
                    ui.data_mut(|d| d.insert_temp(code_id, (picked, code)));
                    paint_icon(ui, Icon::Pipette, pos2(row.right() - 20.0, row.center().y), 16.0, palette.muted);
                    if hsv != before {
                        let color = from_hsv(hsv);
                        let mut next = theme.clone();
                        if let Some(slot) = next.colors.get_mut(picked) {
                            *slot = color;
                        }
                        ui.data_mut(|d| d.insert_temp(hsv_id, (color, hsv)));
                        changed = Some(next);
                    }
                    // The theme's colours: the one being edited ringed, an x
                    // to take one out.
                    if theme.colors.len() > 1 {
                        let (chips, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
                        for (i, color) in theme.colors.iter().enumerate() {
                            let chip = Rect::from_min_size(pos2(chips.left() + i as f32 * 44.0, chips.top()), vec2(36.0, 36.0));
                            let response = ui.interact(chip, ui.id().with(("chip", i)), Sense::click());
                            ui.painter().rect_filled(chip.shrink(3.0), CornerRadius::same(6), rgb(*color));
                            if i == picked {
                                ui.painter().rect_stroke(chip, CornerRadius::same(8), Stroke::new(2.0, palette.strong), egui::StrokeKind::Inside);
                            }
                            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                picked = i;
                            }
                            if i == picked {
                                let x = Rect::from_center_size(chip.right_top() + vec2(-2.0, 2.0), vec2(16.0, 16.0));
                                let remove = ui.interact(x, ui.id().with(("remove", i)), Sense::click());
                                ui.painter().circle_filled(x.center(), 8.0, palette.danger);
                                paint_icon(ui, Icon::Close, x.center(), 11.0, Color32::WHITE);
                                if remove.clicked() {
                                    let mut next = theme.clone();
                                    next.colors.remove(i);
                                    picked = picked.min(next.colors.len() - 1);
                                    changed = Some(next);
                                }
                            }
                        }
                    }
                    if theme.colors.len() < MOST
                        && button(ui, &palette, Some(Icon::Plus), "Adicionar cor", palette.rail)
                    {
                        let mut next = theme.clone();
                        let [h, s, v] = to_hsv(current);
                        next.colors.push(from_hsv([(h + 0.15) % 1.0, s, v]));
                        picked = next.colors.len() - 1;
                        changed = Some(next);
                    }
                    ui.add_space(8.0);
                    label(ui, &palette, "Controles");
                    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
                    ui.painter().text(pos2(line.left(), line.center().y), Align2::LEFT_CENTER, "Intensidade de cor", font(Weight::Medium, 15.0), palette.text);
                    ui.painter().text(pos2(line.right(), line.center().y), Align2::RIGHT_CENTER, format!("{}%", theme.intensity), font(Weight::Medium, 15.0), palette.text);
                    let mut intensity = f32::from(theme.intensity);
                    if slider(ui, &palette, &mut intensity) {
                        changed = Some(ColorTheme { intensity: intensity.round() as u8, ..theme.clone() });
                    }
                    ui.add_space(16.0);
                    if button(ui, &palette, Some(Icon::Dice), "Surpreenda-me!", palette.rail) {
                        changed = Some(surprise(ui.input(|i| i.time)));
                        picked = 0;
                    }
                    if button(ui, &palette, None, "Redefinir", palette.rail) {
                        reset = true;
                    }
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        if button(ui, &palette, None, "Voltar às Configurações", palette.rail) {
                            close = true;
                            app.actions.push(Action::OpenSettings(crate::app::SettingsPage::Appearance));
                        }
                    });
                });
        });
    ui.data_mut(|d| d.insert_temp(picked_id, picked));
    if reset {
        app.actions.push(Action::SetColorTheme(None));
    } else if let Some(theme) = changed {
        app.actions.push(Action::SetColorTheme(Some(theme)));
    } else if app.settings.color_theme.is_none() {
        // Opening the editor starts from Discord's blurple.
        app.actions.push(Action::SetColorTheme(Some(theme)));
    }
    if close {
        app.actions.push(Action::Customize(false));
    }
}

/// What the editor opens on: Discord's blurple, dark, 74%.
fn default_theme() -> ColorTheme {
    ColorTheme {
        colors: vec![[0x58, 0x65, 0xf2]],
        light: false,
        intensity: 74,
    }
}

/// Two or three colours a hue apart from a random start.
fn surprise(seed: f64) -> ColorTheme {
    let mut state = (seed * 1_000_003.0) as u64 ^ 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 10_000) as f32 / 10_000.0
    };
    let count = 2 + (next() * 2.0) as usize;
    let start = next();
    let step = 0.08 + next() * 0.25;
    let colors = (0..count)
        .map(|i| from_hsv([(start + step * i as f32) % 1.0, 0.45 + next() * 0.4, 0.55 + next() * 0.4]))
        .collect();
    ColorTheme {
        colors,
        light: next() > 0.7,
        intensity: 60 + (next() * 30.0) as u8,
    }
}

fn label(ui: &mut Ui, palette: &crate::theme::Palette, text: &str) {
    ui.label(egui::RichText::new(text).font(font(Weight::Medium, 14.0)).color(palette.muted));
}

/// A full-width button with an optional icon; whether it was clicked.
fn button(ui: &mut Ui, palette: &crate::theme::Palette, icon: Option<Icon>, text: &str, fill: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let fill = if response.hovered() { palette.hover } else { fill };
    ui.painter().rect(rect, CornerRadius::same(8), fill, Stroke::new(1.0, palette.divider), egui::StrokeKind::Inside);
    let galley = ui.painter().layout_no_wrap(text.into(), font(Weight::Medium, 15.0), palette.strong);
    let width = galley.size().x + if icon.is_some() { 26.0 } else { 0.0 };
    let left = rect.center().x - width / 2.0;
    if let Some(icon) = icon {
        paint_icon(ui, icon, pos2(left + 9.0, rect.center().y), 18.0, palette.strong);
    }
    ui.painter().galley(
        pos2(left + if icon.is_some() { 26.0 } else { 0.0 }, rect.center().y - galley.size().y / 2.0),
        galley,
        palette.strong,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Saturation across, brightness down, for the hue chosen.
fn saturation_square(ui: &mut Ui, palette: &crate::theme::Palette, hsv: &mut [f32; 3]) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 170.0), Sense::click_and_drag());
    const STEPS: u32 = 12;
    let mut mesh = egui::Mesh::default();
    for row in 0..=STEPS {
        for column in 0..=STEPS {
            let (s, v) = (column as f32 / STEPS as f32, 1.0 - row as f32 / STEPS as f32);
            let pos = rect.lerp_inside(vec2(column as f32 / STEPS as f32, row as f32 / STEPS as f32));
            mesh.colored_vertex(pos, rgb(from_hsv([hsv[0], s, v])));
        }
    }
    let w = STEPS + 1;
    for row in 0..STEPS {
        for column in 0..STEPS {
            let i = row * w + column;
            mesh.add_triangle(i, i + 1, i + w);
            mesh.add_triangle(i + 1, i + w + 1, i + w);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter().rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, palette.divider), egui::StrokeKind::Inside);
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        hsv[1] = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        hsv[2] = 1.0 - ((pointer.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
    }
    let knob = pos2(rect.left() + hsv[1] * rect.width(), rect.top() + (1.0 - hsv[2]) * rect.height());
    ring(ui, knob);
}

/// The rainbow of hues.
fn hue_bar(ui: &mut Ui, _palette: &crate::theme::Palette, hsv: &mut [f32; 3]) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::click_and_drag());
    let colors: Vec<Color32> = (0..=12).map(|i| rgb(from_hsv([i as f32 / 12.0, 1.0, 1.0]))).collect();
    let mut mesh = egui::Mesh::default();
    for (i, color) in colors.iter().enumerate() {
        let x = rect.left() + rect.width() * i as f32 / 12.0;
        mesh.colored_vertex(pos2(x, rect.top()), *color);
        mesh.colored_vertex(pos2(x, rect.bottom()), *color);
    }
    for i in 0..12u32 {
        let k = i * 2;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        hsv[0] = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 0.999);
    }
    ring(ui, pos2(rect.left() + hsv[0] * rect.width(), rect.center().y));
}

/// The white ring that marks a picked spot.
fn ring(ui: &Ui, at: Pos2) {
    ui.painter().circle_stroke(at, 8.0, Stroke::new(2.5, Color32::WHITE));
    ui.painter().circle_stroke(at, 9.5, Stroke::new(1.0, Color32::from_black_alpha(90)));
}

/// Discord's slider: a filled track and a white knob; whether it moved.
fn slider(ui: &mut Ui, palette: &crate::theme::Palette, value: &mut f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 16.0, 6.0));
    ui.painter().rect_filled(track, CornerRadius::same(3), palette.selected);
    let mut moved = false;
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        let next = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0) * 100.0;
        if (next - *value).abs() >= 0.5 {
            *value = next;
            moved = true;
        }
    }
    let x = track.left() + track.width() * *value / 100.0;
    ui.painter().rect_filled(Rect::from_min_max(track.min, pos2(x, track.bottom())), CornerRadius::same(3), palette.accent);
    ui.painter().circle_filled(pos2(x, track.center().y), 9.0, Color32::WHITE);
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
    moved
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let text = text.trim().trim_start_matches('#');
    if text.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(text, 16).ok()?;
    Some([(value >> 16) as u8, (value >> 8) as u8, value as u8])
}

pub fn to_hsv(c: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = c.map(|x| f32::from(x) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    [h, s, max]
}

pub fn from_hsv([h, s, v]: [f32; 3]) -> [u8; 3] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    let (r, g, b) = match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r, g, b].map(|x| (x * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_survive_the_round_trip_through_hsv() {
        for c in [[0x58, 0x65, 0xf2], [255, 0, 0], [0, 0, 0], [12, 200, 99], [255, 255, 255]] {
            assert_eq!(from_hsv(to_hsv(c)), c);
        }
        assert_eq!(parse_hex("#5865F2"), Some([0x58, 0x65, 0xf2]));
        assert_eq!(hex([0x58, 0x65, 0xf2]), "#5865F2");
    }
}
