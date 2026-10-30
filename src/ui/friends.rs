//! The friends page, home's first screen: the friends who are on, all of
//! them, pending requests, blocked people, and adding a friend by username.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2,
    vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{Elide, avatar, font, paint_icon, status_color};
use crate::app::{Action, App};
use crate::chat::ChatAction;
use crate::icons::Icon;
use crate::model::{Id, Status};
use crate::nav::{FriendsTab, NavAction, relationship};
use crate::theme::HEADER_HEIGHT;

const ADD_GREEN: Color32 = Color32::from_rgb(0x24, 0x80, 0x46);

pub fn show(app: &mut App, ui: &mut Ui) {
    let mut out: Vec<Action> = Vec::new();
    header(app, ui, &mut out);
    egui::CentralPanel::default()
        .frame(Frame::new().inner_margin(Margin {
            left: 30,
            right: 20,
            top: 16,
            bottom: 0,
        }))
        .show(ui, |ui| match app.nav.friends_tab {
            FriendsTab::Add => add_friend(app, ui, &mut out),
            tab => list(app, ui, tab, &mut out),
        });
    app.actions.extend(out);
}

fn header(app: &mut App, ui: &mut Ui, out: &mut Vec<Action>) {
    let palette = app.palette;
    let pending = app
        .model
        .relationships
        .values()
        .filter(|k| **k == relationship::INCOMING)
        .count() as u32;
    egui::Panel::top("friends-header")
        .resizable(false)
        .exact_size(HEADER_HEIGHT)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.expand2(vec2(16.0, 0.0)).x_range(),
                rect.bottom() - 0.5,
                Stroke::new(1.0, palette.divider),
            );
            let y = rect.center().y;
            paint_icon(
                ui,
                Icon::Users,
                pos2(rect.left() + 12.0, y),
                22.0,
                palette.muted,
            );
            ui.painter().text(
                pos2(rect.left() + 32.0, y),
                Align2::LEFT_CENTER,
                "Amigos",
                font(Weight::SemiBold, 16.0),
                palette.strong,
            );
            let mut x = rect.left() + 104.0;
            ui.painter()
                .circle_filled(pos2(x - 12.0, y), 2.0, palette.muted);
            for (tab, label) in [
                (FriendsTab::Online, "Disponível"),
                (FriendsTab::All, "Todos"),
                (FriendsTab::Pending, "Pendente"),
                (FriendsTab::Blocked, "Bloqueado"),
                (FriendsTab::Add, "Adicionar amigo"),
            ] {
                let selected = app.nav.friends_tab == tab;
                let add = tab == FriendsTab::Add;
                let width_of = ui
                    .painter()
                    .layout_no_wrap(label.into(), font(Weight::Medium, 15.0), palette.text)
                    .size()
                    .x;
                let badge = tab == FriendsTab::Pending && pending > 0;
                let width = width_of + 16.0 + if badge { 24.0 } else { 0.0 };
                let tab_rect = Rect::from_min_size(pos2(x, y - 13.0), vec2(width, 26.0));
                x += width + 12.0;
                let response = ui.interact(
                    tab_rect,
                    ui.id().with(("friends-tab", label)),
                    Sense::click(),
                );
                let hovered = response.hovered();
                let (fill, text_color) = match (add, selected, hovered) {
                    (true, true, _) => (None, palette.online),
                    (true, false, _) => (Some(ADD_GREEN), Color32::WHITE),
                    (false, true, _) => (Some(palette.selected), palette.strong),
                    (false, false, true) => (Some(palette.hover), palette.text),
                    (false, false, false) => (None, palette.muted),
                };
                if let Some(fill) = fill {
                    ui.painter()
                        .rect_filled(tab_rect, CornerRadius::same(4), fill);
                }
                let galley = ui.painter().layout_no_wrap(
                    label.into(),
                    font(Weight::Medium, 15.0),
                    text_color,
                );
                ui.painter().galley(
                    pos2(tab_rect.left() + 8.0, y - galley.size().y / 2.0),
                    galley,
                    text_color,
                );
                if badge {
                    super::widgets::badge(
                        ui,
                        &palette,
                        pos2(tab_rect.right() - 6.0, y),
                        pending,
                        None,
                    );
                }
                if response.clicked() {
                    out.push(Action::Nav(NavAction::FriendsTab(tab)));
                }
            }
        });
}

/// Who each tab lists, by name.
fn people(app: &App, tab: FriendsTab) -> Vec<(Id, u8)> {
    let filter = app.nav.friend_filter.to_lowercase();
    let mut list: Vec<(Id, u8)> = app
        .model
        .relationships
        .iter()
        .map(|(id, kind)| (*id, *kind))
        .filter(|(id, kind)| match tab {
            FriendsTab::Online => {
                *kind == relationship::FRIEND
                    && app
                        .model
                        .user(*id)
                        .is_some_and(|u| u.status != Status::Offline)
            }
            FriendsTab::All => *kind == relationship::FRIEND,
            FriendsTab::Pending => {
                *kind == relationship::INCOMING || *kind == relationship::OUTGOING
            }
            FriendsTab::Blocked => *kind == relationship::BLOCKED,
            FriendsTab::Add => false,
        })
        .filter(|(id, _)| {
            filter.is_empty()
                || app.model.user(*id).is_some_and(|u| {
                    u.name.to_lowercase().contains(&filter)
                        || u.username.to_lowercase().contains(&filter)
                })
        })
        .collect();
    list.sort_by_cached_key(|(id, kind)| {
        (
            *kind != relationship::INCOMING,
            app.model
                .user(*id)
                .map(|u| u.name.to_lowercase())
                .unwrap_or_default(),
        )
    });
    list
}

fn list(app: &mut App, ui: &mut Ui, tab: FriendsTab, out: &mut Vec<Action>) {
    let palette = app.palette;
    // The search over the list.
    Frame::new()
        .fill(palette.rail)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.nav.friend_filter)
                        .frame(Frame::NONE)
                        .hint_text(RichText::new("Buscar").color(palette.muted))
                        .font(font(Weight::Regular, 15.0))
                        .text_color(palette.text)
                        .desired_width(ui.available_width() - 28.0),
                );
                let (icon, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                paint_icon(ui, Icon::Search, icon.center(), 18.0, palette.muted);
            });
        });
    let people = people(app, tab);
    let label = match tab {
        FriendsTab::Online => "DISPONÍVEL",
        FriendsTab::All => "TODOS OS AMIGOS",
        FriendsTab::Pending => "PENDENTE",
        FriendsTab::Blocked => "BLOQUEADO",
        FriendsTab::Add => "",
    };
    ui.add_space(20.0);
    ui.label(
        RichText::new(format!("{label} — {}", people.len()))
            .font(font(Weight::SemiBold, 12.0))
            .color(palette.muted),
    );
    ui.add_space(12.0);
    if people.is_empty() {
        let text = match tab {
            FriendsTab::Online => "Ninguém por perto para brincar com o Wumpus.",
            FriendsTab::Pending => {
                "Não há pedidos de amizade pendentes. Fique à vontade para adicionar alguém."
            }
            FriendsTab::Blocked => "Você não pode desbloquear o Wumpus.",
            _ => "O Wumpus está esperando amigos. Mas você não precisa!",
        };
        ui.add_space(80.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(text)
                    .font(font(Weight::Regular, 15.0))
                    .color(palette.muted),
            );
        });
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for (user, kind) in people {
                row(app, ui, user, kind, out);
            }
        });
}

/// One person: avatar with status, name and what they are doing, and the
/// round buttons on the right.
fn row(app: &App, ui: &mut Ui, user: Id, kind: u8, out: &mut Vec<Action>) {
    let palette = app.palette;
    let Some(person) = app.model.user(user) else {
        return;
    };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 62.0), Sense::click());
    let hovered = response.hovered() || response.context_menu_opened();
    let inner = rect.shrink2(vec2(0.0, 1.0));
    if hovered {
        ui.painter().rect_filled(
            inner.expand2(vec2(10.0, 0.0)),
            CornerRadius::same(8),
            palette.hover,
        );
    } else {
        ui.painter().hline(
            rect.x_range(),
            rect.top() + 0.5,
            Stroke::new(1.0, palette.divider),
        );
    }
    let cy = rect.center().y;
    let ring = if hovered { palette.hover } else { palette.chat };
    let status = (kind == relationship::FRIEND).then(|| status_color(&palette, person.status));
    avatar(
        ui,
        app,
        person,
        pos2(rect.left() + 16.0, cy),
        16.0,
        status.map(|_| ring),
    );
    let line = match kind {
        relationship::INCOMING => "Pedido de amizade recebido".to_owned(),
        relationship::OUTGOING => "Pedido de amizade enviado".to_owned(),
        relationship::BLOCKED => "Bloqueado".to_owned(),
        _ => person.activity.clone().unwrap_or_else(|| {
            match person.status {
                Status::Online => "Disponível",
                Status::Idle => "Ausente",
                Status::DoNotDisturb => "Não perturbar",
                Status::Offline => "Offline",
            }
            .to_owned()
        }),
    };
    let buttons: Vec<(Icon, &str, Action)> = match kind {
        relationship::INCOMING => vec![
            (Icon::Check, "Aceitar", Action::Nav(NavAction::Accept(user))),
            (
                Icon::Close,
                "Ignorar",
                Action::Nav(NavAction::RemoveRelationship(user)),
            ),
        ],
        relationship::OUTGOING => vec![(
            Icon::Close,
            "Cancelar",
            Action::Nav(NavAction::RemoveRelationship(user)),
        )],
        relationship::BLOCKED => vec![(
            Icon::Close,
            "Desbloquear",
            Action::Nav(NavAction::RemoveRelationship(user)),
        )],
        _ => vec![(
            Icon::MessageCircle,
            "Mensagem",
            Action::Chat(ChatAction::MessageUser {
                user,
                text: String::new(),
            }),
        )],
    };
    let text_right = rect.right() - 16.0 - 46.0 * buttons.len() as f32;
    // The nickname the user gave this friend, when there is one.
    let shown = app.model.name_in(None, person.id);
    let name_galley = ui.painter().layout_no_wrap(
        shown.clone(),
        font(Weight::SemiBold, 16.0),
        palette.strong,
    );
    let name_w = name_galley.size().x.min(text_right - rect.left() - 44.0);
    Elide::new(ui, text_right).text(
        pos2(rect.left() + 44.0, cy - 9.0),
        Align2::LEFT_CENTER,
        shown,
        font(Weight::SemiBold, 16.0),
        palette.strong,
    );
    if hovered && person.username != person.name {
        Elide::new(ui, text_right).text(
            pos2(rect.left() + 50.0 + name_w, cy - 9.0),
            Align2::LEFT_CENTER,
            person.username.clone(),
            font(Weight::Medium, 14.0),
            palette.muted,
        );
    }
    Elide::new(ui, text_right).text(
        pos2(rect.left() + 44.0, cy + 10.0),
        Align2::LEFT_CENTER,
        line,
        font(Weight::Medium, 13.0),
        palette.muted,
    );
    let mut x = rect.right() - 8.0 - 18.0;
    let mut pressed = false;
    for (icon, hint, action) in buttons.into_iter().rev() {
        let center = pos2(x, cy);
        x -= 46.0;
        let button = ui.interact(
            Rect::from_center_size(center, Vec2::splat(36.0)),
            ui.id().with(("friend-button", user, hint)),
            Sense::click(),
        );
        ui.painter().circle_filled(
            center,
            18.0,
            if hovered {
                palette.rail
            } else {
                palette.sidebar
            },
        );
        let tint = match (button.hovered(), hint) {
            (true, "Aceitar") => palette.online,
            (true, "Ignorar" | "Cancelar") => palette.danger,
            (true, _) => palette.strong,
            _ => palette.icon,
        };
        paint_icon(ui, icon, center, 20.0, tint);
        if button.on_hover_text(hint).clicked() {
            out.push(action);
            pressed = true;
        }
    }
    if response.clicked() && !pressed && kind == relationship::FRIEND {
        out.push(Action::Chat(ChatAction::MessageUser {
            user,
            text: String::new(),
        }));
    }
    response.context_menu(|ui| {
        ui.set_min_width(180.0);
        if kind == relationship::FRIEND {
            if ui.button("Mensagem").clicked() {
                out.push(Action::Chat(ChatAction::MessageUser {
                    user,
                    text: String::new(),
                }));
                ui.close();
            }
            ui.separator();
            let remove = egui::Button::new(RichText::new("Desfazer amizade").color(palette.danger));
            if ui.add(remove).clicked() {
                out.push(Action::Nav(NavAction::RemoveRelationship(user)));
                ui.close();
            }
        }
        if kind != relationship::BLOCKED {
            let block = egui::Button::new(RichText::new("Bloquear").color(palette.danger));
            if ui.add(block).clicked() {
                out.push(Action::Nav(NavAction::Block(user)));
                ui.close();
            }
        }
        if ui.button("Copiar nome de usuário").clicked() {
            out.push(Action::Chat(ChatAction::Copy(person.username.clone())));
            ui.close();
        }
        if ui.button("Copiar ID do usuário").clicked() {
            out.push(Action::Chat(ChatAction::Copy(user.to_string())));
            ui.close();
        }
    });
}

/// "Adicionar amigo": a username and Discord's answer.
fn add_friend(app: &mut App, ui: &mut Ui, out: &mut Vec<Action>) {
    let palette = app.palette;
    ui.label(
        RichText::new("Adicionar amigo")
            .font(font(Weight::SemiBold, 20.0))
            .color(palette.strong),
    );
    ui.add_space(8.0);
    ui.label(
        RichText::new("Você pode adicionar amigos com o nome de usuário do Discord deles.")
            .font(font(Weight::Regular, 15.0))
            .color(palette.muted),
    );
    ui.add_space(16.0);
    let border = match &app.nav.add_result {
        Some(Ok(_)) => palette.online,
        Some(Err(_)) => palette.danger,
        None => palette.rail,
    };
    let empty = app.nav.add_friend.trim().is_empty();
    let mut send = false;
    Frame::new()
        .fill(palette.rail)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin {
            left: 12,
            right: 12,
            top: 10,
            bottom: 10,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut app.nav.add_friend)
                        .frame(Frame::NONE)
                        .hint_text(
                            RichText::new(
                                "Você pode adicionar amigos com o nome de usuário deles.",
                            )
                            .color(palette.muted),
                        )
                        .font(font(Weight::Regular, 16.0))
                        .text_color(palette.strong)
                        .desired_width(ui.available_width() - 200.0),
                );
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    send = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button = egui::Button::new(
                        RichText::new("Enviar pedido de amizade")
                            .font(font(Weight::Medium, 14.0))
                            .color(Color32::WHITE),
                    )
                    .fill(palette.accent)
                    .corner_radius(CornerRadius::same(4))
                    .min_size(vec2(180.0, 32.0));
                    if ui.add_enabled(!empty, button).clicked() {
                        send = true;
                    }
                });
            });
        });
    match &app.nav.add_result {
        Some(Ok(text)) => {
            ui.add_space(8.0);
            ui.label(
                RichText::new(text)
                    .font(font(Weight::Regular, 14.0))
                    .color(palette.online),
            );
        }
        Some(Err(text)) => {
            ui.add_space(8.0);
            ui.label(
                RichText::new(text)
                    .font(font(Weight::Regular, 14.0))
                    .color(palette.danger),
            );
        }
        None => {}
    }
    if send && !empty {
        out.push(Action::Nav(NavAction::AddFriend));
    }
}
