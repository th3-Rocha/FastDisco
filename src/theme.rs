//! Colours and metrics. The layout follows Discord's desktop client so the
//! controls are where its users expect them.

use egui::Color32;

/// Every colour the interface draws.
#[derive(Clone, Copy, Debug)]
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

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const RAIL_WIDTH: f32 = 72.0;
pub const SIDEBAR_WIDTH: f32 = 240.0;
pub const MEMBERS_WIDTH: f32 = 240.0;
pub const HEADER_HEIGHT: f32 = 48.0;
pub const USER_PANEL_HEIGHT: f32 = 52.0;
