//! The window: the server rail, the sidebar and the open channel, in
//! Discord's arrangement.

mod admin;
mod bots;
mod call;
mod chat;
mod composer;
mod customizer;
mod discovery;
mod events;
mod friends;
mod login;
mod manage;
mod message;
mod nav;
pub mod overlay;
mod player;
mod profile;
mod rail;
mod settings;
mod sidebar;
mod soundboard;
mod threads;
mod titlebar;
mod widgets;

use crate::app::{App, Screen};

pub fn show(app: &mut App, ui: &mut egui::Ui) {
    match app.screen {
        Screen::Connecting => login::loading(app, ui),
        Screen::Login => login::show(app, ui),
        Screen::Main if app.settings_page.is_some() && !settings::as_window(ui.ctx()) => {
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
            // With settings open over the app, its keys and typing go to
            // the settings.
            let settings_open = app.settings_page.is_some();
            if !settings_open {
                nav::keys(app, ui.ctx());
            }
            // Who talks changes the rings in the channel list.
            if app.call.is_some() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
            // Alt+Up and Alt+Down move between channels, as in Discord.
            let alt = egui::Modifiers::ALT;
            if settings_open {
                // Nothing: the settings window has the keyboard.
            } else if ui.input_mut(|i| i.consume_key(alt, egui::Key::ArrowUp)) {
                app.actions.push(crate::app::Action::StepChannel(-1));
            }
            if ui.input_mut(|i| i.consume_key(alt, egui::Key::ArrowDown)) {
                app.actions.push(crate::app::Action::StepChannel(1));
            }
            // Alt+Left and Alt+Right are the title bar's arrows.
            if ui.input_mut(|i| i.consume_key(alt, egui::Key::ArrowLeft)) {
                app.actions.push(crate::app::Action::Back);
            }
            if ui.input_mut(|i| i.consume_key(alt, egui::Key::ArrowRight)) {
                app.actions.push(crate::app::Action::Forward);
            }
            app.track_location();
            if !settings_open {
                composer::paste_files(app, ui.ctx());
                composer::type_anywhere(app, ui.ctx());
            }
            let window = ui.max_rect();
            // Behind the sidebar's rounded corner.
            crate::theme::paint_backdrop(ui.painter(), window, app.palette.frame);
            titlebar::show(app, ui);
            titlebar::voice_taken(app, ui);
            rail::show(app, ui);
            if app.discovery.open {
                discovery::sidebar(app, ui);
                discovery::page(app, ui);
            } else {
                let sidebar = sidebar::show(app, ui);
                sidebar::card(app, ui, window);
                chat::show(app, ui);
                sidebar::frame_line(app, ui, sidebar, window);
            }
            if settings_open {
                settings::window(app, ui);
            }
            customizer::show(app, ui);
            profile::popout(app, ui);
            sidebar::logout_dialog(app, ui);
            admin::modal(app, ui);
            events::dialog(app, ui);
            nav::popout(app, ui);
            nav::switcher(app, ui);
            nav::shortcuts(app, ui);
            bots::modal(app, ui);
            bots::forward_dialog(app, ui);
            bots::poll_dialog(app, ui);
            call::incoming(app, ui);
            soundboard::panel(app, ui);
            discovery::dialogs(app, ui);
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

/// Discord's link blue.
pub const LINK: egui::Color32 = egui::Color32::from_rgb(0x00, 0xa8, 0xfc);

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
