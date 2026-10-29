//! Colours and metrics. The layout follows Discord's desktop client so the
//! controls are where its users expect them.

use egui::Color32;

/// Every colour the interface draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The title bar and the server rail, around everything else.
    pub frame: Color32,
    /// Text fields and the deepest surfaces.
    pub rail: Color32,
    /// The channel list and the member list.
    pub sidebar: Color32,
    /// The conversation.
    pub chat: Color32,
    /// The user panel under the channel list.
    pub user_panel: Color32,
    /// A row under the pointer.
    pub hover: Color32,
    /// The selected row.
    pub selected: Color32,
    /// The message field.
    pub composer: Color32,
    /// A message under the pointer.
    pub message_hover: Color32,
    /// Lines between regions.
    pub divider: Color32,
    /// Names, messages and selected rows.
    pub text: Color32,
    /// Headers and the brightest text.
    pub strong: Color32,
    /// Channel names, timestamps and hints.
    pub muted: Color32,
    /// Buttons that are not in use.
    pub icon: Color32,
    pub accent: Color32,
    pub online: Color32,
    pub idle: Color32,
    pub dnd: Color32,
    pub offline: Color32,
    /// Mention badges.
    pub danger: Color32,
    /// The background of a message that mentions you.
    pub mention: Color32,
    pub mention_bar: Color32,
}

impl Palette {
    /// Discord's grey theme ("Cinza" since its 2025 redesign): the title
    /// bar, the rail and the channel list one colour, the conversation a
    /// step lighter, the user card floating over both.
    pub const fn dark() -> Self {
        Self {
            frame: rgb(0x2c2d32),
            rail: rgb(0x222327),
            sidebar: rgb(0x2c2d32),
            chat: rgb(0x323339),
            user_panel: rgb(0x393a41),
            hover: rgb(0x36373d),
            selected: rgb(0x404147),
            composer: rgb(0x393a41),
            message_hover: rgb(0x2f3035),
            divider: rgb(0x3c3d43),
            text: rgb(0xdfe0e2),
            strong: rgb(0xfbfbfb),
            muted: rgb(0x9fa0a6),
            icon: rgb(0xb8b9bf),
            accent: rgb(0x5865f2),
            online: rgb(0x43a25a),
            idle: rgb(0xca9654),
            dnd: rgb(0xd83a42),
            offline: rgb(0x82838b),
            danger: rgb(0xd83a42),
            mention: Color32::from_rgba_premultiplied(0x2c, 0x26, 0x14, 0x30),
            mention_bar: rgb(0xf0b232),
        }
    }
}

impl Palette {
    /// Discord's light theme.
    pub const fn light() -> Self {
        Self {
            frame: rgb(0xe3e5e8),
            rail: rgb(0xe3e5e8),
            sidebar: rgb(0xf2f3f5),
            chat: rgb(0xffffff),
            user_panel: rgb(0xebedef),
            hover: rgb(0xe0e1e5),
            selected: rgb(0xd4d7dc),
            composer: rgb(0xebedef),
            message_hover: rgb(0xf7f7f8),
            divider: rgb(0xe1e2e4),
            text: rgb(0x313338),
            strong: rgb(0x060607),
            muted: rgb(0x5c5e66),
            icon: rgb(0x4e5058),
            accent: rgb(0x5865f2),
            online: rgb(0x23a55a),
            idle: rgb(0xf0b232),
            dnd: rgb(0xf23f43),
            offline: rgb(0x80848e),
            danger: rgb(0xda373c),
            mention: Color32::from_rgba_premultiplied(0x5c, 0x4c, 0x1a, 0x26),
            mention_bar: rgb(0xf0b232),
        }
    }

    /// Whether the background is dark, for egui's own widgets.
    pub fn is_dark(&self) -> bool {
        // Unmultiplied: a colour theme's surfaces are see-through.
        let [r, g, b, _] = self.chat.to_srgba_unmultiplied();
        (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000 < 128
    }
}

/// Mixes `b` into `a` by `t` (0 is `a`, 1 is `b`).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Palette files and Omarchy's rendering use fastframe's sixteen shared
/// colour names; they map onto Discord's regions, and the regions a file
/// leaves out follow from the ones it sets.
impl fastframe_theme::Palette for Palette {
    fn base(base: fastframe_theme::Base) -> Self {
        match base {
            fastframe_theme::Base::Dark => Self::dark(),
            fastframe_theme::Base::Light => Self::light(),
        }
    }

    fn set(&mut self, name: &str, color: Color32) -> bool {
        match name {
            "window" | "chat" => self.chat = color,
            "panel" | "sidebar" => self.sidebar = color,
            "surface" | "composer" => self.composer = color,
            "surface_hover" | "hover" => self.hover = color,
            "surface_active" | "selected" => self.selected = color,
            "outline" | "divider" => self.divider = color,
            "text" => self.text = color,
            "secondary" | "icon" => self.icon = color,
            "dim" | "muted" => self.muted = color,
            "accent" => self.accent = color,
            "danger" => self.danger = color,
            "warning" => self.idle = color,
            "rail" => self.rail = color,
            "frame" => self.frame = color,
            "user_panel" => self.user_panel = color,
            "message_hover" => self.message_hover = color,
            "strong" => self.strong = color,
            "online" => self.online = color,
            "mention_bar" => self.mention_bar = color,
            // Shared names Discord's layout has no place for.
            "accent_hover" | "on_accent" | "overlay" | "shadow" => {}
            _ => return false,
        }
        true
    }

    fn derive(&mut self, given: &std::collections::BTreeSet<&str>) {
        let dark = self.is_dark();
        let shade = if dark {
            Color32::BLACK
        } else {
            Color32::from_gray(200)
        };
        if !given.contains("rail") {
            self.rail = mix(self.sidebar, shade, if dark { 0.35 } else { 0.1 });
        }
        if !given.contains("frame") {
            self.frame = self.rail;
        }
        if !given.contains("user_panel") {
            self.user_panel = mix(self.sidebar, shade, if dark { 0.2 } else { 0.05 });
        }
        if !given.contains("message_hover") {
            self.message_hover = mix(self.chat, self.hover, 0.5);
        }
        if !given.contains("strong") {
            self.strong = mix(
                self.text,
                if dark { Color32::WHITE } else { Color32::BLACK },
                0.5,
            );
        }
        if !given.contains("mention_bar") && given.contains("warning") {
            self.mention_bar = self.idle;
        }
        self.dnd = self.danger;
        self.mention = Color32::from_rgba_unmultiplied(
            self.mention_bar.r(),
            self.mention_bar.g(),
            self.mention_bar.b(),
            26,
        );
    }
}

static CURRENT: std::sync::RwLock<Palette> = std::sync::RwLock::new(Palette::dark());

/// The palette in use, for drawing code that has no `App` at hand.
pub fn current() -> Palette {
    *CURRENT.read().unwrap_or_else(|e| e.into_inner())
}

/// egui's own widgets (menus, tooltips, text fields, selections) in the
/// palette's colours.
pub fn apply(ctx: &egui::Context, palette: &Palette) {
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = *palette;
    let dark = palette.is_dark();
    ctx.all_styles_mut(|style| {
        let mut visuals = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.selection.bg_fill = palette.accent;
        visuals.text_cursor.stroke.color = palette.text;
        let floating = if dark {
            mix(palette.rail, Color32::BLACK, 0.4)
        } else {
            Color32::WHITE
        };
        visuals.window_fill = floating;
        visuals.panel_fill = palette.chat;
        visuals.extreme_bg_color = palette.rail;
        // Discord's menus: rounder, with a faint edge.
        visuals.window_stroke = egui::Stroke::new(1.0, palette.divider);
        visuals.window_corner_radius = egui::CornerRadius::same(6);
        visuals.menu_corner_radius = egui::CornerRadius::same(8);
        visuals.popup_shadow = egui::Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(100),
        };
        visuals.widgets.hovered.weak_bg_fill = palette.accent;
        visuals.widgets.hovered.bg_fill = palette.accent;
        visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
        visuals.widgets.inactive.fg_stroke.color = palette.text;
        visuals.widgets.noninteractive.fg_stroke.color = palette.text;
        visuals.override_text_color = None;
        style.visuals = visuals;
    });
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const RAIL_WIDTH: f32 = 72.0;
pub const SIDEBAR_WIDTH: f32 = 240.0;
/// How narrow and how wide the channel list can be dragged.
pub const SIDEBAR_RANGE: std::ops::RangeInclusive<f32> = 200.0..=440.0;
pub const MEMBERS_WIDTH: f32 = 240.0;
pub const HEADER_HEIGHT: f32 = 48.0;
/// The strip across the top with the arrows, the place and the inbox.
pub const TITLE_HEIGHT: f32 = 36.0;

/// The colour themes offered, as Discord's grid: a name, light or dark,
/// and the gradient's colours.
pub const PRESETS: &[(&str, bool, &[u32])] = &[
    ("Menta", true, &[0x9fd8a6, 0xc9e6b4]),
    ("Pêssego", true, &[0xf5d38a, 0xf0a68a]),
    ("Lavanda", true, &[0xa7b7e0, 0x9ec4c9]),
    ("Limonada", true, &[0xd8ecc0, 0xf4d9b0]),
    ("Rosa antigo", true, &[0xd59ab8, 0xd8c79b]),
    ("Algodão-doce", true, &[0xf3c4d2, 0xc7cdf2]),
    ("Céu", true, &[0xa9d6f0, 0xe6f0c4]),
    ("Creme", true, &[0xf3edd8, 0xe3d6b8]),
    ("Crepúsculo", false, &[0x3b2a7a, 0xb5652d]),
    ("Neon", false, &[0x1d4fd8, 0xc026d3, 0x0ea5e9]),
    ("Floresta", false, &[0x1e3b2a, 0x6b5a2e]),
    ("Sangue", false, &[0x7a0d14, 0x1a0505]),
    ("Meia-noite", false, &[0x2a2270, 0x0d0b1f]),
    ("Tijolo", false, &[0x7a3b2e, 0x5a2e25]),
    ("Névoa", false, &[0x4b5066, 0x8d8fa8]),
    ("Musgo", false, &[0x3d5a4b, 0x6b7d6e]),
    ("Oceano", false, &[0x2d4a78, 0x2f7a8a]),
    ("Aurora", false, &[0x1aa39a, 0x7b3fa0]),
    ("Pôr do sol", false, &[0xc81d6b, 0xe0a023]),
    ("Nebulosa", false, &[0x0f5c8c, 0x2e9e6b, 0x1a2b6b]),
    ("Areia", false, &[0x6b5a3c, 0x3a3226]),
    ("Safira", false, &[0x1e2a8a, 0x3b4fd1]),
];

/// A preset as the settings keep it.
pub fn preset(index: usize) -> Option<crate::settings::ColorTheme> {
    let (_, light, colors) = PRESETS.get(index)?;
    Some(crate::settings::ColorTheme {
        colors: colors.iter().map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8]).collect(),
        light: *light,
        intensity: 74,
    })
}

fn color(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// The gradient behind the window: the theme's colours, darkened for a
/// dark theme and lightened for a light one so text stays readable.
pub fn backdrop(theme: &crate::settings::ColorTheme) -> Vec<Color32> {
    let (toward, by) = if theme.light {
        (Color32::WHITE, 0.25)
    } else {
        (Color32::BLACK, 0.45)
    };
    theme.colors.iter().map(|c| mix(color(*c), toward, by)).collect()
}

/// The dark or light palette tinted by a colour theme. The window's frame,
/// the channel list and the conversation turn see-through, so the
/// gradient painted behind shows; everything over them takes a little
/// of the theme's colour.
pub fn tinted(theme: &crate::settings::ColorTheme) -> Palette {
    let mut p = if theme.light { Palette::light() } else { Palette::dark() };
    let colors = backdrop(theme);
    if colors.is_empty() {
        return p;
    }
    let n = colors.len() as u32;
    let sum = colors.iter().fold([0u32; 3], |a, c| {
        [a[0] + u32::from(c.r()), a[1] + u32::from(c.g()), a[2] + u32::from(c.b())]
    });
    let average = Color32::from_rgb((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8);
    let t = f32::from(theme.intensity.min(100)) / 100.0;
    for slot in [
        &mut p.user_panel,
        &mut p.composer,
        &mut p.hover,
        &mut p.selected,
        &mut p.rail,
        &mut p.message_hover,
        &mut p.divider,
    ] {
        *slot = mix(*slot, average, t * 0.3);
    }
    // Solid, for menus and dialogs; `region` makes the big areas see-through.
    for slot in [&mut p.frame, &mut p.sidebar, &mut p.chat] {
        *slot = mix(*slot, average, t * 0.35);
    }
    p
}

/// How see-through the big areas are: 0 without a colour theme.
static SEE_THROUGH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// One of the window's big areas, which a colour theme's gradient shows
/// through.
#[derive(Clone, Copy)]
pub enum Region {
    /// The title bar and the server rail.
    Frame,
    Sidebar,
    Chat,
}

/// `fill` for a big area: see-through over the gradient under a colour
/// theme, as it is otherwise.
pub fn region(fill: Color32, region: Region) -> Color32 {
    let t = f32::from_bits(SEE_THROUGH.load(std::sync::atomic::Ordering::Relaxed));
    if t <= 0.0 {
        return fill;
    }
    let clear = match region {
        Region::Frame => 0.8,
        Region::Sidebar => 0.65,
        Region::Chat => 0.6,
    };
    let [r, g, b, _] = fill.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, ((1.0 - clear * t) * 255.0) as u8)
}

static BACKDROP: std::sync::RwLock<Vec<Color32>> = std::sync::RwLock::new(Vec::new());

/// The gradient `paint_backdrop` draws; empty for a plain theme.
pub fn set_backdrop(colors: Vec<Color32>, intensity: u8) {
    let t = if colors.is_empty() { 0.0 } else { f32::from(intensity.min(100)) / 100.0 };
    SEE_THROUGH.store(f32::to_bits(t), std::sync::atomic::Ordering::Relaxed);
    let mut current = BACKDROP.write().unwrap_or_else(|e| e.into_inner());
    if *current != colors {
        *current = colors;
    }
}

/// The colour at `t` (0 to 1) along a gradient's stops.
pub fn sample(colors: &[Color32], t: f32) -> Color32 {
    match colors.len() {
        0 => Color32::TRANSPARENT,
        1 => colors[0],
        n => {
            let x = t.clamp(0.0, 1.0) * (n - 1) as f32;
            let i = (x as usize).min(n - 2);
            mix(colors[i], colors[i + 1], x - i as f32)
        }
    }
}

/// Paints a diagonal gradient over `rect`, top left to bottom right, with
/// rounded corners of `corner`.
pub fn paint_gradient(painter: &egui::Painter, rect: egui::Rect, colors: &[Color32], corner: f32) {
    let at = |pos: egui::Pos2| {
        let u = (pos.x - rect.left()) / rect.width().max(1.0);
        let v = (pos.y - rect.top()) / rect.height().max(1.0);
        sample(colors, u * 0.6 + v * 0.4)
    };
    let mut mesh = egui::Mesh::default();
    if corner <= 0.0 {
        const STEPS: u32 = 16;
        for row in 0..=STEPS {
            for column in 0..=STEPS {
                let pos = rect.lerp_inside(egui::vec2(
                    column as f32 / STEPS as f32,
                    row as f32 / STEPS as f32,
                ));
                mesh.colored_vertex(pos, at(pos));
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
    } else {
        // A fan from the centre over the rounded outline: the gradient is
        // linear in position, so the triangles carry it exactly.
        let r = corner.min(rect.width() / 2.0).min(rect.height() / 2.0);
        let corners = [
            (rect.right_bottom() + egui::vec2(-r, -r), 0.0f32),
            (rect.left_bottom() + egui::vec2(r, -r), 90.0),
            (rect.left_top() + egui::vec2(r, r), 180.0),
            (rect.right_top() + egui::vec2(-r, r), 270.0),
        ];
        mesh.colored_vertex(rect.center(), at(rect.center()));
        for (center, start) in corners {
            for step in 0..=6 {
                let angle = (start + step as f32 * 15.0).to_radians();
                let pos = center + egui::vec2(angle.cos(), angle.sin()) * r;
                mesh.colored_vertex(pos, at(pos));
            }
        }
        let n = mesh.vertices.len() as u32 - 1;
        for i in 1..=n {
            mesh.add_triangle(0, i, if i == n { 1 } else { i + 1 });
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The window's background: the colour theme's gradient, or `fill`.
pub fn paint_backdrop(painter: &egui::Painter, rect: egui::Rect, fill: Color32) {
    let colors = BACKDROP.read().unwrap_or_else(|e| e.into_inner()).clone();
    if colors.is_empty() {
        painter.rect_filled(rect, 0.0, fill);
    } else {
        paint_gradient(painter, rect, &colors, 0.0);
        painter.rect_filled(rect, 0.0, region(fill, Region::Frame));
    }
}
