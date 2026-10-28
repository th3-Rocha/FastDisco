//! Colours and metrics. The layout follows Discord's desktop client so the
//! controls are where its users expect them.

use egui::Color32;

/// Every colour the interface draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The server rail, the search field and the deepest surfaces.
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
    pub const fn dark() -> Self {
        Self {
            rail: rgb(0x1e1f22),
            sidebar: rgb(0x2b2d31),
            chat: rgb(0x313338),
            user_panel: rgb(0x232428),
            hover: rgb(0x35373c),
            selected: rgb(0x404249),
            composer: rgb(0x383a40),
            message_hover: rgb(0x2e3035),
            divider: rgb(0x1f2023),
            text: rgb(0xdbdee1),
            strong: rgb(0xf2f3f5),
            muted: rgb(0x949ba4),
            icon: rgb(0xb5bac1),
            accent: rgb(0x5865f2),
            online: rgb(0x23a55a),
            idle: rgb(0xf0b232),
            dnd: rgb(0xf23f43),
            offline: rgb(0x80848e),
            danger: rgb(0xf23f43),
            mention: Color32::from_rgba_premultiplied(0x2c, 0x26, 0x14, 0x30),
            mention_bar: rgb(0xf0b232),
        }
    }
}

impl Palette {
    /// Discord's light theme.
    pub const fn light() -> Self {
        Self {
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
        let c = self.chat;
        (u32::from(c.r()) * 299 + u32::from(c.g()) * 587 + u32::from(c.b()) * 114) / 1000 < 128
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
pub const TITLE_HEIGHT: f32 = 32.0;
