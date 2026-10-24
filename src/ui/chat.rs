//! The open channel: its header, the messages, the message field and the
//! member list.

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, Sense, Stroke, Ui,
    UiBuilder, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{avatar, clock, font, hyphens, icon_button, paint_icon, timestamp};
use crate::app::{Action, App, Place};
use crate::icons::Icon;
use crate::model::{ChannelId, Message, Status};
use crate::theme::{HEADER_HEIGHT, MEMBERS_WIDTH, Palette};

/// Messages from one author this close together share one header.
const GROUP_MINUTES: i32 = 7;
const GUTTER: f32 = 72.0;
/// The narrowest the messages get before the member list hides.
const MEMBERS_MIN_CHAT: f32 = 640.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::CentralPanel::default()
        .frame(Frame::new().fill(palette.chat).inner_margin(Margin::ZERO))
        .show(ui, |ui| {
            let Some(channel) = app.channel else { return };
            header(app, ui, channel);
            // Like Discord, a narrow window gives the room to the messages.
            if app.show_members
                && app.server().is_some()
                && ui.available_width() >= MEMBERS_MIN_CHAT + MEMBERS_WIDTH
            {
                members(app, ui);
            }
            composer(app, ui, channel);
            egui::CentralPanel::default()
                .frame(Frame::new().inner_margin(Margin::ZERO))
                .show(ui, |ui| messages(app, ui, channel));
        });
}

fn header(app: &mut App, ui: &mut Ui, id: ChannelId) {
    let palette = app.palette;
    egui::Panel::top("chat-header")
        .resizable(false)
        .exact_size(HEADER_HEIGHT)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin::symmetric(8, 0)))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let full = rect.expand2(vec2(8.0, 0.0));
            ui.painter().hline(
                full.x_range(),
                full.bottom() - 0.5,
                Stroke::new(1.0, palette.divider),
            );
            let channel = app.model.channel(id);
            let direct = app
                .model
                .direct
                .iter()
                .find(|dm| dm.channel == id)
                .map(|dm| dm.user);
            let mut x = rect.left() + 8.0;
            let y = rect.center().y;
            match direct {
                Some(user) => {
                    let user = app.model.user(user);
                    avatar(
                        ui,
                        &palette,
                        user,
                        pos2(x + 12.0, y),
                        12.0,
                        Some(palette.chat),
                    );
                    x += 32.0;
                    let g = ui.painter().layout_no_wrap(
                        user.name.clone(),
                        font(Weight::SemiBold, 16.0),
                        palette.strong,
                    );
                    let w = g.size().x;
                    ui.painter()
                        .galley(pos2(x, y - g.size().y / 2.0), g, palette.strong);
                    x += w;
                }
                None => {
                    paint_icon(ui, Icon::Hash, pos2(x + 12.0, y), 24.0, palette.muted);
                    x += 30.0;
                    let g = ui.painter().layout_no_wrap(
                        channel.name.clone(),
                        font(Weight::SemiBold, 16.0),
                        palette.strong,
                    );
                    let w = g.size().x;
                    ui.painter()
                        .galley(pos2(x, y - g.size().y / 2.0), g, palette.strong);
                    x += w;
                }
            }
            let buttons_left = rect.right() - 420.0;
            if let Some(topic) = &channel.topic {
                x += 12.0;
                ui.painter().vline(
                    x,
                    (y - 12.0)..=(y + 12.0),
                    Stroke::new(1.0, palette.selected),
                );
                x += 12.0;
                let clip = Rect::from_min_max(
                    pos2(x, rect.top()),
                    pos2(buttons_left - 8.0, rect.bottom()),
                );
                ui.painter().with_clip_rect(clip).text(
                    pos2(x, y),
                    Align2::LEFT_CENTER,
                    topic,
                    font(Weight::Medium, 14.0),
                    palette.muted,
                );
            }

            let right = Rect::from_min_max(pos2(buttons_left, rect.top()), rect.right_bottom());
            let show_members = app.show_members;
            let in_server = app.server().is_some();
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(right)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing = vec2(12.0, 0.0);
                    icon_button(ui, &palette, Icon::Help, 24.0, false, "Ajuda");
                    icon_button(ui, &palette, Icon::Inbox, 24.0, false, "Caixa de entrada");
                    search(ui, &palette);
                    if in_server {
                        let toggle = icon_button(
                            ui,
                            &palette,
                            Icon::Users,
                            24.0,
                            show_members,
                            if show_members {
                                "Ocultar lista de membros"
                            } else {
                                "Mostrar lista de membros"
                            },
                        );
                        if toggle.clicked() {
                            app.actions.push(Action::ToggleMembers);
                        }
                        icon_button(ui, &palette, Icon::Pin, 24.0, false, "Mensagens fixadas");
                        icon_button(
                            ui,
                            &palette,
                            Icon::Bell,
                            24.0,
                            false,
                            "Configurações de notificação",
                        );
                        icon_button(ui, &palette, Icon::Threads, 24.0, false, "Tópicos");
                    } else {
                        icon_button(
                            ui,
                            &palette,
                            Icon::User,
                            24.0,
                            false,
                            "Mostrar perfil do usuário",
                        );
                        icon_button(
                            ui,
                            &palette,
                            Icon::UserPlus,
                            24.0,
                            false,
                            "Adicionar amigos à DM",
                        );
                        icon_button(ui, &palette, Icon::Pin, 24.0, false, "Mensagens fixadas");
                    }
                },
            );
        });
}

/// The search field in the header. Searching arrives with the gateway.
fn search(ui: &mut Ui, palette: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(144.0, 24.0), Sense::click());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(4), palette.rail);
    ui.painter().text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Buscar",
        font(Weight::Medium, 14.0),
        palette.muted,
    );
    paint_icon(
        ui,
        Icon::Search,
        pos2(rect.right() - 12.0, rect.center().y),
        16.0,
        palette.muted,
    );
}

fn composer(app: &mut App, ui: &mut Ui, id: ChannelId) {
    let palette = app.palette;
    let placeholder = match app.model.direct.iter().find(|dm| dm.channel == id) {
        Some(dm) => format!("Conversar com @{}", app.model.user(dm.user).name),
        None => format!("Conversar em #{}", app.model.channel(id).name),
    };
    egui::Panel::bottom("composer")
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin {
            left: 16,
            right: 16,
            top: 0,
            bottom: 24,
        }))
        .show(ui, |ui| {
            Frame::new()
                .fill(palette.composer)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
                        icon_button(
                            ui,
                            &palette,
                            Icon::PlusCircle,
                            24.0,
                            false,
                            "Enviar um arquivo",
                        );
                        let buttons = 4.0 * 24.0 + 3.0 * 16.0;
                        let width = ui.available_width() - buttons - 16.0;
                        let edit = egui::TextEdit::singleline(&mut app.drafts[id])
                            .hint_text(egui::RichText::new(placeholder).color(palette.muted))
                            .font(font(Weight::Regular, 16.0))
                            .text_color(palette.text)
                            .frame(Frame::NONE)
                            .desired_width(width);
                        let response = ui.add(edit);
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            app.actions.push(Action::Send);
                            response.request_focus();
                        }
                        icon_button(ui, &palette, Icon::Gift, 24.0, false, "Presentear");
                        icon_button(
                            ui,
                            &palette,
                            Icon::Gif,
                            24.0,
                            false,
                            "Abrir seletor de GIFs",
                        );
                        icon_button(
                            ui,
                            &palette,
                            Icon::Sticker,
                            24.0,
                            false,
                            "Abrir seletor de figurinhas",
                        );
                        icon_button(ui, &palette, Icon::Smile, 24.0, false, "Selecionar emoji");
                    });
                });
        });
}

fn messages(app: &mut App, ui: &mut Ui, id: ChannelId) {
    let palette = app.palette;
    let server = app.server();
    egui::ScrollArea::vertical()
        .id_salt(("messages", id))
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let width = ui.available_width();
            // Short channels sit at the bottom, as in Discord.
            let channel = app.model.channel(id);
            welcome(app, ui, id, width);
            let mut previous: Option<&Message> = None;
            for message in &channel.messages {
                let new_day = previous.is_some_and(|p| p.minute < 0 && message.minute >= 0);
                if new_day {
                    divider(ui, &palette, width, "Hoje");
                }
                let continues = !new_day
                    && previous.is_some_and(|p| {
                        p.author == message.author && message.minute - p.minute <= GROUP_MINUTES
                    });
                if !continues {
                    ui.add_space(16.0);
                }
                row(app, ui, server, message, continues, width);
                previous = Some(message);
            }
            ui.add_space(8.0);
        });
}

/// The start of a channel: its big icon and a welcome.
fn welcome(app: &App, ui: &mut Ui, id: ChannelId, width: f32) {
    let palette = app.palette;
    let channel = app.model.channel(id);
    let free = ui.available_height() - 400.0;
    if free > 0.0 && channel.messages.len() < 12 {
        ui.add_space(free.min(ui.available_height()));
    }
    let (rect, _) = ui.allocate_exact_size(vec2(width, 150.0), Sense::hover());
    let left = rect.left() + 16.0;
    let direct = app
        .model
        .direct
        .iter()
        .find(|dm| dm.channel == id)
        .map(|dm| dm.user);
    let (title, line) = match direct {
        Some(user) => {
            let user = app.model.user(user);
            avatar(
                ui,
                &palette,
                user,
                pos2(left + 40.0, rect.top() + 40.0),
                40.0,
                None,
            );
            (
                user.name.clone(),
                format!(
                    "Este é o começo do seu histórico de mensagens diretas com {}.",
                    user.name
                ),
            )
        }
        None => {
            ui.painter().circle_filled(
                pos2(left + 34.0, rect.top() + 34.0),
                34.0,
                palette.selected,
            );
            paint_icon(
                ui,
                Icon::Hash,
                pos2(left + 34.0, rect.top() + 34.0),
                42.0,
                palette.strong,
            );
            (
                format!("Boas\u{2010}vindas a #{}!", channel.name),
                format!("Este é o começo do canal #{}.", channel.name),
            )
        }
    };
    ui.painter().text(
        pos2(left, rect.top() + 96.0),
        Align2::LEFT_CENTER,
        title,
        font(Weight::Bold, 30.0),
        palette.strong,
    );
    ui.painter().text(
        pos2(left, rect.top() + 130.0),
        Align2::LEFT_CENTER,
        line,
        font(Weight::Regular, 16.0),
        palette.muted,
    );
}

fn divider(ui: &mut Ui, palette: &Palette, width: f32, label: &str) {
    ui.add_space(12.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 12.0), Sense::hover());
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        font(Weight::SemiBold, 12.0),
        palette.muted,
    );
    let mid = rect.center();
    let half = galley.size().x / 2.0 + 6.0;
    let stroke = Stroke::new(1.0, palette.selected);
    ui.painter()
        .hline((rect.left() + 16.0)..=(mid.x - half), mid.y, stroke);
    ui.painter()
        .hline((mid.x + half)..=(rect.right() - 16.0), mid.y, stroke);
    ui.painter()
        .galley(mid - galley.size() / 2.0, galley, palette.muted);
}

fn row(
    app: &App,
    ui: &mut Ui,
    server: Option<usize>,
    message: &Message,
    continues: bool,
    width: f32,
) {
    let palette = app.palette;
    let text_width = width - GUTTER - 48.0;
    let job = rich(app, &message.text, text_width);
    let galley = ui.painter().layout_job(job);
    let head = if continues { 0.0 } else { 22.0 };
    let height = 2.0 + head + galley.size().y + 2.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let hovered = response.hovered();
    if message.mentions_me {
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, palette.mention);
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            CornerRadius::ZERO,
            palette.mention_bar,
        );
    } else if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, palette.message_hover);
    }
    let author = app.model.user(message.author);
    let top = rect.top() + 2.0;
    if continues {
        if hovered {
            ui.painter().text(
                pos2(rect.left() + GUTTER - 10.0, top + 11.0),
                Align2::RIGHT_CENTER,
                clock(message.minute),
                font(Weight::Regular, 11.0),
                palette.muted,
            );
        }
    } else {
        avatar(
            ui,
            &palette,
            author,
            pos2(rect.left() + 16.0 + 20.0, top + 20.0),
            20.0,
            None,
        );
        let color = app
            .model
            .name_color(server, message.author)
            .unwrap_or(palette.strong);
        let name =
            ui.painter()
                .layout_no_wrap(author.name.clone(), font(Weight::Medium, 16.0), color);
        let name_width = name.size().x;
        ui.painter()
            .galley(pos2(rect.left() + GUTTER, top), name, color);
        ui.painter().text(
            pos2(rect.left() + GUTTER + name_width + 8.0, top + 11.0),
            Align2::LEFT_CENTER,
            timestamp(message.minute),
            font(Weight::Regular, 12.0),
            palette.muted,
        );
    }
    ui.painter()
        .galley(pos2(rect.left() + GUTTER, top + head), galley, palette.text);
}

/// The message text, with mentions on their tinted background.
fn rich(app: &App, text: &str, wrap: f32) -> LayoutJob {
    let text = &*hyphens(text);
    let palette = app.palette;
    let body = TextFormat {
        font_id: font(Weight::Regular, 16.0),
        color: palette.text,
        line_height: Some(22.0),
        ..Default::default()
    };
    let mention = TextFormat {
        font_id: font(Weight::Medium, 16.0),
        color: Color32::from_rgb(0xc9, 0xcd, 0xfb),
        background: Color32::from_rgba_unmultiplied(0x58, 0x65, 0xf2, 0x4d),
        line_height: Some(22.0),
        ..Default::default()
    };
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let mut rest = text;
    while let Some(at) = rest.find('@') {
        job.append(&rest[..at], 0.0, body.clone());
        let tail = &rest[at..];
        let end = tail[1..]
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .map_or(tail.len(), |i| i + 1);
        job.append(&tail[..end], 0.0, mention.clone());
        rest = &tail[end..];
    }
    job.append(rest, 0.0, body);
    job
}

fn members(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let Place::Server(id) = app.place else { return };
    egui::Panel::right("members")
        .resizable(false)
        .exact_size(MEMBERS_WIDTH)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.sidebar)
                .inner_margin(Margin::ZERO),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("members")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let server = &app.model.servers[id];
                    let online = |user: usize| app.model.user(user).status != Status::Offline;
                    // Hoisted roles get their own group; everyone else online
                    // is "Online"; everyone offline is "Offline".
                    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
                    for (index, role) in server.roles.iter().enumerate().filter(|(_, r)| r.hoisted)
                    {
                        let list: Vec<usize> = server
                            .members
                            .iter()
                            .filter(|m| m.role == Some(index) && online(m.user))
                            .map(|m| m.user)
                            .collect();
                        groups.push((role.name.clone(), list));
                    }
                    let rest = server
                        .members
                        .iter()
                        .filter(|m| {
                            online(m.user) && !m.role.is_some_and(|r| server.roles[r].hoisted)
                        })
                        .map(|m| m.user)
                        .collect();
                    groups.push(("Online".into(), rest));
                    groups.push((
                        "Offline".into(),
                        server
                            .members
                            .iter()
                            .filter(|m| !online(m.user))
                            .map(|m| m.user)
                            .collect(),
                    ));

                    ui.add_space(8.0);
                    for (name, list) in groups.into_iter().filter(|(_, l)| !l.is_empty()) {
                        ui.add_space(16.0);
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(MEMBERS_WIDTH, 20.0), Sense::hover());
                        ui.painter().text(
                            pos2(rect.left() + 16.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            format!("{} — {}", name.to_uppercase(), list.len()),
                            font(Weight::SemiBold, 12.0),
                            palette.muted,
                        );
                        for user in list {
                            member(app, ui, Some(id), user);
                        }
                    }
                });
        });
}

fn member(app: &App, ui: &mut Ui, server: Option<usize>, id: usize) {
    let palette = app.palette;
    let user = app.model.user(id);
    let (slot, response) = ui.allocate_exact_size(vec2(MEMBERS_WIDTH, 44.0), Sense::click());
    let rect = Rect::from_min_max(
        pos2(slot.left() + 8.0, slot.top() + 1.0),
        pos2(slot.right() - 8.0, slot.bottom() - 1.0),
    );
    let hovered = response.hovered();
    let behind = if hovered {
        palette.hover
    } else {
        palette.sidebar
    };
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), behind);
    }
    let offline = user.status == Status::Offline && !hovered;
    let fade = |c: Color32| if offline { c.gamma_multiply(0.3) } else { c };
    let opacity = if offline { 0.3 } else { 1.0 };
    ui.scope(|ui| {
        ui.set_opacity(opacity);
        avatar(
            ui,
            &palette,
            user,
            pos2(rect.left() + 24.0, rect.center().y),
            16.0,
            (!offline).then_some(behind),
        );
    });
    let color = app.model.name_color(server, id).unwrap_or(palette.text);
    let name_y = if user.activity.is_some() && !offline {
        rect.center().y - 8.0
    } else {
        rect.center().y
    };
    ui.painter().text(
        pos2(rect.left() + 50.0, name_y),
        Align2::LEFT_CENTER,
        &user.name,
        font(Weight::Medium, 15.0),
        fade(color),
    );
    if let Some(activity) = user.activity.as_ref().filter(|_| !offline) {
        let clip = Rect::from_min_max(
            pos2(rect.left() + 50.0, rect.top()),
            rect.right_bottom() - vec2(4.0, 0.0),
        );
        ui.painter().with_clip_rect(clip).text(
            pos2(rect.left() + 50.0, rect.center().y + 9.0),
            Align2::LEFT_CENTER,
            activity,
            font(Weight::Regular, 12.0),
            palette.muted,
        );
    }
}
