//! The window: the server rail, the sidebar and the open channel, in
//! Discord's arrangement.

mod chat;
mod composer;
mod login;
mod message;
pub mod overlay;
mod player;
mod profile;
mod rail;
mod settings;
mod sidebar;
mod widgets;

use crate::app::{App, Screen};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    match app.screen {
        Screen::Connecting => login::loading(app, ui),
        Screen::Login => login::show(app, ui),
        Screen::Main if app.settings_page.is_some() => {
            settings::show(app, ui);
            sidebar::logout_dialog(app, ui);
        }
        Screen::Main => {
            if let Some(player) = &mut app.chat.player {
                player.update();
            }
            app.update_animations();
            // Alt+Up and Alt+Down move between channels, as in Discord.
            let alt = egui::Modifiers::ALT;
            if ui.input_mut(|i| i.consume_key(alt, egui::Key::ArrowUp)) {
                app.actions.push(crate::app::Action::StepChannel(-1));
            }
            if ui.input_mut(|i| i.consume_key(alt, egui::Key::ArrowDown)) {
                app.actions.push(crate::app::Action::StepChannel(1));
            }
            composer::type_anywhere(app, ui.ctx());
            rail::show(app, ui);
            sidebar::show(app, ui);
            chat::show(app, ui);
            profile::popout(app, ui);
            sidebar::logout_dialog(app, ui);
            let wanted: Vec<crate::model::Id> =
                ui.data_mut(|d| {
                    std::mem::take(d.get_temp_mut_or_default::<Vec<crate::model::Id>>(
                        egui::Id::new("want-app-icons"),
                    ))
                });
            for application in wanted {
                app.want_app_icon(application);
            }
        }
    }
}

/// Widgets that sense clicks but are not buttons: a message's background
/// (for its context menu) and text with links, which sets its own hand.
const NO_HAND: &str = "no-pointing-hand";

pub fn no_hand(ui: &egui::Ui, id: egui::Id) {
    ui.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<egui::Id>>(egui::Id::new(NO_HAND))
            .push(id)
    });
}

/// The pointing hand over anything clickable, as in Discord. Run after
/// drawing; text fields keep their text cursor.
pub fn pointing_hand(ctx: &egui::Context) {
    let skip: Vec<egui::Id> = ctx.data_mut(|d| {
        std::mem::take(d.get_temp_mut_or_default::<Vec<egui::Id>>(egui::Id::new(NO_HAND)))
    });
    if ctx.output(|o| o.cursor_icon) != egui::CursorIcon::Default {
        return;
    }
    let hovered = ctx.interaction_snapshot(|s| s.hovered.clone());
    let clickable = hovered.iter().any(|id| {
        !skip.contains(id)
            && ctx
                .read_response(*id)
                .is_some_and(|r| r.sense.senses_click() && r.enabled())
    });
    if clickable {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}
