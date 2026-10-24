//! The window: the server rail, the sidebar and the open channel, in
//! Discord's arrangement.

mod chat;
mod rail;
mod sidebar;
mod widgets;

use crate::app::App;

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    rail::show(app, ui);
    sidebar::show(app, ui);
    chat::show(app, ui);
}
