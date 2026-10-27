//! The window: the server rail, the sidebar and the open channel, in
//! Discord's arrangement.

mod admin;
mod bots;
mod call;
mod chat;
mod composer;
mod friends;
mod login;
mod message;
mod nav;
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
        Screen::Main if app.admin.settings.is_some() => {
            admin::settings(app, ui);
            admin::modal(app, ui);
        }
        Screen::Main => {
            if let Some(player) = &mut app.chat.player {
                player.update();
            }
            app.update_animations();
            nav::keys(app, ui.ctx());
            // Who talks changes the rings in the channel list.
            if app.call.is_some() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
            // Alt+Up and Alt+Down move between channels, as in Discord.
            let alt = egui::Modifiers::ALT;
            if ui.input_mut(|i| i.consume_key(alt, egui::Key::ArrowUp)) {
                app.actions.push(crate::app::Action::StepChannel(-1));
            }
            if ui.input_mut(|i| i.consume_key(alt, egui::Key::ArrowDown)) {
                app.actions.push(crate::app::Action::StepChannel(1));
            }
            composer::paste_files(app, ui.ctx());
            composer::type_anywhere(app, ui.ctx());
            let window = ui.max_rect();
            rail::show(app, ui);
            sidebar::show(app, ui);
            sidebar::card(app, ui, window);
            chat::show(app, ui);
            profile::popout(app, ui);
            sidebar::logout_dialog(app, ui);
            admin::modal(app, ui);
            nav::popout(app, ui);
            nav::switcher(app, ui);
            nav::shortcuts(app, ui);
            bots::modal(app, ui);
            bots::forward_dialog(app, ui);
            bots::poll_dialog(app, ui);
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
