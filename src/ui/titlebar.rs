//! Discord's strip across the top of the window: the back and forward
//! arrows, where the user is in the middle, and the inbox, help and the
//! update arrow at the right. It also moves the window, as a title bar.

use egui::{Align, Align2, CornerRadius, Frame, Layout, Rect, Sense, Ui, UiBuilder, Vec2, pos2, vec2};
use fastframe_fonts::Weight;

use super::widgets::{Elide, font, icon_button, paint_icon};
use crate::app::{Action, App, Place};
use crate::icons::Icon;
use crate::theme::{TITLE_HEIGHT, mix};

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::Panel::top("title-bar")
        .resizable(false)
        .exact_size(TITLE_HEIGHT)
        .show_separator_line(false)
        .frame(Frame::new().fill(palette.rail))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            // Under everything else, so the buttons win the pointer.
            let bar = ui.interact(rect, ui.id().with("drag"), Sense::click_and_drag());
            if bar.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if bar.double_clicked() {
                let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            super::no_hand(ui, bar.id);

            let y = rect.center().y;
            arrows(app, ui, pos2(rect.left() + 12.0, y));
            place(app, ui, rect);

            let right = Rect::from_min_max(
                pos2(rect.right() - 160.0, rect.top()),
                pos2(rect.right() - 8.0, rect.bottom()),
            );
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(right)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                    // Discord's green arrow when an update is ready.
                    if let crate::updates::Status::Ready(version) = &app.update {
                        let hint =
                            format!("Atualização pronta: versão {version}. Clique para reiniciar.");
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                        let tint = if response.hovered() {
                            palette.online.gamma_multiply(1.2)
                        } else {
                            palette.online
                        };
                        paint_icon(ui, Icon::Download, rect.center(), 20.0, tint);
                        if response.on_hover_text(hint).clicked() {
                            app.actions.push(Action::InstallUpdate);
                        }
                    }
                    if icon_button(ui, &palette, Icon::Help, 24.0, false, "Ajuda").clicked() {
                        app.actions
                            .push(Action::Chat(crate::chat::ChatAction::OpenUrl(
                                "https://support.discord.com".into(),
                            )));
                    }
                    super::nav::popout_button(app, ui, crate::nav::Popout::Inbox);
                },
            );
        });
}

/// Back and forward, dim when there is nowhere to go.
fn arrows(app: &mut App, ui: &mut Ui, left_center: egui::Pos2) {
    let palette = app.palette;
    for (i, (icon, hint, enabled, action)) in [
        (Icon::Back, "Voltar", !app.back.is_empty(), Action::Back),
        (Icon::Forward, "Avançar", !app.forward.is_empty(), Action::Forward),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::from_center_size(
            pos2(left_center.x + 10.0 + i as f32 * 26.0, left_center.y),
            Vec2::splat(24.0),
        );
        let response = ui.interact(rect, ui.id().with(hint), Sense::click());
        let response = response.on_hover_text(hint);
        let tint = match (enabled, response.hovered()) {
            (false, _) => mix(palette.rail, palette.icon, 0.4),
            (true, true) => palette.strong,
            (true, false) => palette.icon,
        };
        paint_icon(ui, icon, rect.center(), 18.0, tint);
        if enabled && response.clicked() {
            app.actions.push(action);
        }
    }
}

/// Where the user is, centred: the server's icon and name, or Discord's
/// logo and the home page's name.
fn place(app: &App, ui: &mut Ui, rect: Rect) {
    let palette = app.palette;
    let y = rect.center().y;
    let guild = match app.place {
        Place::Server(id) => app.model.guild(id),
        Place::Home => None,
    };
    let title = match (&guild, super::nav::is_friends(app)) {
        (Some(guild), _) => guild.name.clone(),
        (None, true) => "Amigos".to_owned(),
        (None, false) => "Mensagens diretas".to_owned(),
    };
    let text_font = font(Weight::SemiBold, 14.0);
    let width = ui
        .painter()
        .layout_no_wrap(title.clone(), text_font.clone(), palette.strong)
        .size()
        .x
        .min(rect.width() - 420.0)
        .max(0.0);
    let icon = 18.0;
    let left = rect.center().x - (icon + 8.0 + width) / 2.0;
    let icon_rect = Rect::from_center_size(pos2(left + icon / 2.0, y), Vec2::splat(icon));
    match guild {
        Some(guild) => super::rail::paint_guild(ui, app, guild, icon_rect, false),
        None => {
            ui.painter()
                .rect_filled(icon_rect, CornerRadius::same(5), palette.accent);
            paint_icon(ui, Icon::Discord, icon_rect.center(), 13.0, egui::Color32::WHITE);
        }
    }
    Elide::new(ui, left + icon + 8.0 + width).text(
        pos2(left + icon + 8.0, y),
        Align2::LEFT_CENTER,
        title,
        text_font,
        palette.strong,
    );
}
