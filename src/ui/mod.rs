//! The window: the server rail, the sidebar and the open channel, in
//! Discord's arrangement.

mod chat;
mod composer;
mod login;
mod message;
pub mod overlay;
mod player;
mod rail;
mod sidebar;
mod widgets;

use crate::app::{App, Screen};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    match app.screen {
        Screen::Connecting => login::loading(app, ui),
        Screen::Login => login::show(app, ui),
        Screen::Main => {
            if let Some(player) = &mut app.chat.player {
                player.update();
            }
            composer::type_anywhere(app, ui.ctx());
            rail::show(app, ui);
            sidebar::show(app, ui);
            chat::show(app, ui);
            sidebar::logout_dialog(app, ui);
        }
    }
}
