//! The user profile, as Discord draws it: a card that opens beside a
//! clicked avatar or name, and the panel beside a direct message. Banner,
//! avatar with status, badges, name, pronouns, custom status, about me,
//! member since, roles, what they are playing or listening to, the note
//! and a field to message them.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Shadow, Stroke, Ui, Vec2,
    pos2, vec2,
};
use fastframe_fonts::Weight;

use super::message::{Row, TextStyle, blocks};
use super::widgets::{Elide, font, hyphens, long_date, paint_icon, status_color};
use crate::app::{Action, App};
use crate::chat::{ChatAction, ProfileState, Side};
use crate::icons::Icon;
use crate::markdown;
use crate::model::{Activity, ActivityKind, Id, Profile, Status, User};

const CARD_WIDTH: f32 = 300.0;
pub const PANEL_WIDTH: f32 = 340.0;
const BANNER: f32 = 105.0;
const AVATAR: f32 = 80.0;

/// The card beside what was clicked; a click elsewhere or Escape closes it.
pub fn popout(app: &mut App, ui: &mut Ui) {
    let Some(popout) = app.chat.popout else {
        return;
    };
    let content = ui.ctx().content_rect();
    let pos = match popout.side {
        Side::Right => pos2(popout.anchor.right() + 8.0, popout.anchor.top()),
        Side::Left => pos2(popout.anchor.left() - 8.0 - CARD_WIDTH, popout.anchor.top()),
        Side::Above => pos2(popout.anchor.left(), popout.anchor.top() - 8.0),
    };
    let pivot = if popout.side == Side::Above {
        Align2::LEFT_BOTTOM
    } else {
        Align2::LEFT_TOP
    };
    let pos = pos2(
        pos.x
            .clamp(content.left() + 8.0, content.right() - CARD_WIDTH - 8.0),
        pos.y,
    );
    let mut out = Vec::new();
    let area = egui::Area::new(egui::Id::new("profile-popout"))
        .order(egui::Order::Foreground)
        .pivot(pivot)
        .fixed_pos(pos)
        .constrain(true)
        .show(ui.ctx(), |ui| {
            let background = card_background(app, popout.user);
            Frame::new()
                .fill(background)
                .corner_radius(CornerRadius::same(8))
                .shadow(Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(110),
                })
                .show(ui, |ui| {
                    ui.set_width(CARD_WIDTH);
                    card(
                        app,
                        ui,
                        popout.user,
                        popout.guild,
                        CARD_WIDTH,
                        true,
                        &mut out,
                    );
                });
        });
    let escape = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if area.response.clicked_elsewhere() || escape {
        out.push(Action::Chat(ChatAction::CloseProfile));
    }
    app.actions.extend(out);
}

/// The panel at the right of a direct message.
pub fn panel(app: &mut App, ui: &mut Ui, user: Id) {
    let background = card_background(app, user);
    let mut out = Vec::new();
    egui::Panel::right("dm-profile")
        .resizable(false)
        .exact_size(PANEL_WIDTH)
        .show_separator_line(false)
        .frame(Frame::new().fill(background).inner_margin(Margin::ZERO))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("dm-profile", user))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    card(app, ui, user, None, PANEL_WIDTH, false, &mut out);
                });
        });
    app.actions.extend(out);
}

fn card_background(app: &App, user: Id) -> Color32 {
    match profile(app, user).and_then(|p| p.theme) {
        Some((top, _)) => top.gamma_multiply(0.6).to_opaque(),
        None => app.palette.user_panel,
    }
}

fn profile(app: &App, user: Id) -> Option<&Profile> {
    match app.chat.profiles.get(&user) {
        Some((ProfileState::Ready(p), _)) => Some(p),
        _ => None,
    }
}

fn card(
    app: &App,
    ui: &mut Ui,
    user_id: Id,
    guild: Option<Id>,
    width: f32,
    popout: bool,
    out: &mut Vec<Action>,
) {
    let palette = app.palette;
    let Some(user) = app.model.user(user_id).cloned() else {
        return;
    };
    let profile = profile(app, user_id).cloned();
    let loading = matches!(
        app.chat.profiles.get(&user_id),
        Some((ProfileState::Loading, _))
    );
    let background = card_background(app, user_id);
    ui.spacing_mut().item_spacing = Vec2::ZERO;

    // Banner: the picture, else the accent colour, else the avatar's.
    let (banner, _) = ui.allocate_exact_size(vec2(width, BANNER), Sense::hover());
    let top = if popout {
        CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        }
    } else {
        CornerRadius::ZERO
    };
    let fill = profile
        .as_ref()
        .and_then(|p| p.accent)
        .unwrap_or(user.color());
    ui.painter().rect_filled(banner, top, fill);
    if let Some(uri) = app.picture(profile.as_ref().and_then(|p| p.banner.as_deref())) {
        egui::Image::new(uri)
            .corner_radius(top)
            .paint_at(ui, banner);
    }

    // The avatar overlaps the banner, ringed in the card's colour.
    let center = pos2(banner.left() + 16.0 + AVATAR / 2.0, banner.bottom());
    ui.painter()
        .circle_filled(center, AVATAR / 2.0 + 6.0, background);
    super::widgets::avatar(ui, app, &user, center, AVATAR / 2.0, None);
    let dot = center + vec2(AVATAR * 0.35, AVATAR * 0.35);
    ui.painter().circle_filled(dot, 12.0, background);
    paint_status(ui, &user, dot, 8.0, background, palette.offline);
    let own = popout && user_id == app.model.me;
    if own {
        thought(app, ui, &user, center, banner, out);
    }

    // Badges in a pill at the right, under the banner.
    let badges = profile
        .as_ref()
        .map(|p| p.badges.clone())
        .unwrap_or_default();
    if !badges.is_empty() && !own {
        let size = 22.0;
        let pill_w = badges.len() as f32 * (size + 2.0) + 8.0;
        let pill = Rect::from_min_size(
            pos2(banner.right() - 16.0 - pill_w, banner.bottom() + 12.0),
            vec2(pill_w, size + 8.0),
        );
        ui.painter().rect_filled(
            pill,
            CornerRadius::same(8),
            crate::theme::current().rail.gamma_multiply(0.6),
        );
        for (i, badge) in badges.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(
                    pill.left() + 4.0 + i as f32 * (size + 2.0),
                    pill.top() + 4.0,
                ),
                Vec2::splat(size),
            );
            if let Some(uri) = app.picture(Some(&badge.icon)) {
                egui::Image::new(uri).paint_at(ui, rect);
            }
            let response = ui
                .interact(rect, ui.id().with(("badge", i)), Sense::click())
                .on_hover_text(&badge.description);
            if response.clicked()
                && let Some(link) = &badge.link
            {
                out.push(Action::Chat(ChatAction::OpenUrl(link.clone())));
            }
        }
    }
    ui.add_space(AVATAR / 2.0 + 16.0);

    Frame::new()
        .inner_margin(Margin {
            left: 16,
            right: 16,
            top: 0,
            bottom: 16,
        })
        .show(ui, |ui| {
            ui.set_width(width - 32.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
            let name = app.model.name_in(guild, user_id);
            ui.label(
                RichText::new(hyphens(&name))
                    .font(font(Weight::Bold, 20.0))
                    .color(palette.strong),
            );
            let mut second = user.username.clone();
            if let Some(pronouns) = profile.as_ref().and_then(|p| p.pronouns.clone()) {
                second.push_str(" • ");
                second.push_str(&pronouns);
            }
            ui.label(
                RichText::new(hyphens(&second))
                    .font(font(Weight::Regular, 14.0))
                    .color(palette.text),
            );
            if user.bot {
                ui.label(
                    RichText::new("APP")
                        .font(font(Weight::SemiBold, 10.0))
                        .color(Color32::WHITE)
                        .background_color(palette.accent),
                );
            }
            if let Some(custom) = user
                .activities
                .iter()
                .find(|a| a.kind == ActivityKind::Custom)
                .filter(|_| !own)
            {
                ui.add_space(4.0);
                custom_status(app, ui, custom);
            }
            ui.add_space(8.0);
            // The user's own card is Discord's account menu instead.
            if own {
                own_card(app, ui, &badges, out);
                return;
            }

            let section = |ui: &mut Ui, title: &str| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(title)
                        .font(font(Weight::Bold, 12.0))
                        .color(palette.strong),
                );
            };

            if loading && profile.is_none() {
                ui.add_space(8.0);
                ui.label(RichText::new("Carregando perfil…").color(palette.muted));
            }

            if let Some(bio) = profile.as_ref().and_then(|p| p.bio.clone()) {
                section(ui, "Sobre mim");
                let mut row = Row {
                    app,
                    guild,
                    channel: 0,
                    out,
                    found: false,
                };
                blocks(
                    &mut row,
                    ui,
                    0,
                    &markdown::parse(&bio),
                    &TextStyle::embed(app),
                    false,
                );
            }

            for activity in user
                .activities
                .iter()
                .filter(|a| a.kind != ActivityKind::Custom)
            {
                section(ui, &activity_title(activity));
                activity_card(app, ui, activity);
            }

            section(ui, "Membro desde");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                paint_small_logo(ui, palette.accent);
                let created =
                    jiff::Timestamp::from_millisecond(((user_id >> 22) + 1_420_070_400_000) as i64)
                        .ok();
                if let Some(created) = created {
                    ui.label(
                        RichText::new(short_date(created))
                            .font(font(Weight::Regular, 13.0))
                            .color(palette.text),
                    );
                }
                if let (Some(joined), Some(g)) = (
                    profile.as_ref().and_then(|p| p.joined),
                    guild.and_then(|g| app.model.guild(g)),
                ) {
                    ui.label(RichText::new("•").color(palette.muted));
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                    match app.picture(g.icon.as_deref()) {
                        Some(uri) => {
                            egui::Image::new(uri)
                                .corner_radius(CornerRadius::same(8))
                                .paint_at(ui, rect);
                        }
                        None => {
                            ui.painter()
                                .circle_filled(rect.center(), 8.0, palette.selected);
                        }
                    }
                    ui.label(
                        RichText::new(short_date(joined))
                            .font(font(Weight::Regular, 13.0))
                            .color(palette.text),
                    );
                }
            });

            if let (Some(g), Some(p)) = (guild.and_then(|g| app.model.guild(g)), profile.as_ref()) {
                let roles: Vec<_> = g.roles.iter().filter(|r| p.roles.contains(&r.id)).collect();
                if !roles.is_empty() {
                    section(ui, if roles.len() == 1 { "Cargo" } else { "Cargos" });
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        for role in roles {
                            let text = ui.painter().layout_no_wrap(
                                hyphens(&role.name).into_owned(),
                                font(Weight::Medium, 12.0),
                                palette.text,
                            );
                            let (rect, _) = ui.allocate_exact_size(
                                vec2(text.size().x + 28.0, 22.0),
                                Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(4),
                                crate::theme::current().rail.gamma_multiply(0.7),
                            );
                            ui.painter().circle_filled(
                                pos2(rect.left() + 11.0, rect.center().y),
                                6.0,
                                role.color.unwrap_or(palette.muted),
                            );
                            ui.painter().galley(
                                pos2(rect.left() + 22.0, rect.center().y - text.size().y / 2.0),
                                text,
                                palette.text,
                            );
                        }
                    });
                }
            }

            if let Some(p) = profile.as_ref() {
                let mut mutual = Vec::new();
                if !p.mutual_guilds.is_empty() {
                    let n = p.mutual_guilds.len();
                    mutual.push(format!(
                        "{n} {} em comum",
                        if n == 1 { "servidor" } else { "servidores" }
                    ));
                }
                if p.mutual_friends > 0 {
                    let n = p.mutual_friends;
                    mutual.push(format!(
                        "{n} {} em comum",
                        if n == 1 { "amigo" } else { "amigos" }
                    ));
                }
                if !mutual.is_empty() && user_id != app.model.me {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(mutual.join(" • "))
                            .font(font(Weight::Medium, 13.0))
                            .color(palette.muted),
                    );
                }
                if !p.connections.is_empty() && !popout {
                    section(ui, "Conexões");
                    for (kind, name) in &p.connections {
                        ui.label(
                            RichText::new(format!("{} — {}", connection_name(kind), name))
                                .font(font(Weight::Regular, 13.0))
                                .color(palette.text),
                        );
                    }
                }
            }

            if user_id != app.model.me {
                section(ui, "Nota (visível apenas para você)");
                note(app, ui, user_id, out);
                if popout {
                    ui.add_space(12.0);
                    message_field(app, ui, user_id, &name, out);
                }
            }
        });
}

fn activity_title(activity: &Activity) -> String {
    match activity.kind {
        ActivityKind::Playing => "Jogando".into(),
        ActivityKind::Streaming => format!("Transmitindo {}", activity.name),
        ActivityKind::Listening if activity.is_spotify() => "Ouvindo no Spotify".into(),
        ActivityKind::Listening => format!("Ouvindo {}", activity.name),
        ActivityKind::Watching => format!("Assistindo {}", activity.name),
        ActivityKind::Competing => "Competindo".into(),
        ActivityKind::Custom => String::new(),
    }
}

fn custom_status(app: &App, ui: &mut Ui, custom: &Activity) {
    let palette = app.palette;
    Frame::new()
        .fill(crate::theme::current().rail.gamma_multiply(0.6))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if let Some(uri) = app.picture(custom.emoji_url.as_deref()) {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                    egui::Image::new(uri).paint_at(ui, rect);
                } else if let Some(emoji) = &custom.emoji {
                    ui.label(RichText::new(emoji).size(16.0));
                }
                if let Some(state) = &custom.state {
                    ui.label(
                        RichText::new(hyphens(state))
                            .font(font(Weight::Regular, 14.0))
                            .color(palette.text),
                    );
                }
            });
        });
}

/// A game, a song or a stream: its picture, its lines and the time.
fn activity_card(app: &App, ui: &mut Ui, activity: &Activity) {
    let palette = app.palette;
    let large = activity
        .large_image
        .clone()
        .or_else(|| activity.application.and_then(|a| app.app_icon(a)));
    if large.is_none()
        && let Some(application) = activity.application
    {
        ui.data_mut(|d| {
            d.get_temp_mut_or_default::<Vec<Id>>(egui::Id::new("want-app-icons"))
                .push(application)
        });
    }
    let now = jiff::Timestamp::now().as_millisecond();
    ui.add_space(4.0);
    Frame::new()
        .fill(crate::theme::current().rail.gamma_multiply(0.6))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                match app.picture(large.as_deref()) {
                    Some(uri) => {
                        egui::Image::new(uri)
                            .corner_radius(CornerRadius::same(8))
                            .paint_at(ui, rect);
                    }
                    None => {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(8), palette.selected);
                        let icon = if activity.kind == ActivityKind::Listening {
                            Icon::Music
                        } else {
                            Icon::Gif
                        };
                        paint_icon(ui, icon, rect.center(), 28.0, palette.muted);
                    }
                }
                if let Some(uri) = app.picture(activity.small_image.as_deref()) {
                    let small = Rect::from_center_size(
                        rect.right_bottom() - vec2(4.0, 4.0),
                        Vec2::splat(22.0),
                    );
                    ui.painter()
                        .circle_filled(small.center(), 13.0, palette.user_panel);
                    egui::Image::new(uri)
                        .corner_radius(CornerRadius::same(11))
                        .paint_at(ui, small);
                    if let Some(text) = &activity.small_text {
                        ui.interact(small, ui.id().with("small-asset"), Sense::hover())
                            .on_hover_text(text);
                    }
                }
                if let Some(text) = &activity.large_text {
                    ui.interact(rect, ui.id().with("large-asset"), Sense::hover())
                        .on_hover_text(text);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let width = ui.available_width();
                    let line = |ui: &mut Ui, text: &str, weight: Weight, color: Color32| {
                        let (rect, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
                        Elide::new(ui, rect.right()).text(
                            rect.left_center(),
                            Align2::LEFT_CENTER,
                            hyphens(text),
                            font(weight, 14.0),
                            color,
                        );
                    };
                    if activity.is_spotify() {
                        line(
                            ui,
                            activity.details.as_deref().unwrap_or(&activity.name),
                            Weight::SemiBold,
                            palette.strong,
                        );
                        if let Some(state) = &activity.state {
                            line(ui, &format!("de {state}"), Weight::Regular, palette.text);
                        }
                        if let Some(album) = &activity.large_text {
                            line(ui, &format!("em {album}"), Weight::Regular, palette.text);
                        }
                    } else {
                        line(ui, &activity.name, Weight::SemiBold, palette.strong);
                        if let Some(details) = &activity.details {
                            line(ui, details, Weight::Regular, palette.text);
                        }
                        if let Some(state) = &activity.state {
                            line(ui, state, Weight::Regular, palette.text);
                        }
                        if let (Some(start), None) = (activity.start, activity.end) {
                            let elapsed = ((now - start) / 1000).max(0);
                            let text = format!("{} decorrido", clock(elapsed));
                            line(ui, &text, Weight::Medium, palette.online);
                        }
                    }
                });
            });
            // Spotify and anything with an end: the progress bar and times.
            if let (Some(start), Some(end)) = (activity.start, activity.end)
                && end > start
            {
                ui.add_space(8.0);
                let total = (end - start) / 1000;
                let done = ((now - start) / 1000).clamp(0, total);
                let (bar, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
                ui.painter()
                    .rect_filled(bar, CornerRadius::same(2), Color32::from_white_alpha(40));
                let filled = Rect::from_min_size(
                    bar.min,
                    vec2(
                        bar.width() * done as f32 / total.max(1) as f32,
                        bar.height(),
                    ),
                );
                ui.painter()
                    .rect_filled(filled, CornerRadius::same(2), Color32::WHITE);
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(clock(done))
                            .font(font(Weight::Regular, 12.0))
                            .color(palette.text),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(clock(total))
                                .font(font(Weight::Regular, 12.0))
                                .color(palette.text),
                        );
                    });
                });
            }
            if let Some(url) = &activity.url {
                ui.add_space(6.0);
                if ui
                    .link(RichText::new("Assistir").color(Color32::from_rgb(0x00, 0xa8, 0xfc)))
                    .clicked()
                {
                    let _ = open::that_detached(url);
                }
            }
        });
    if activity.start.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(1));
    }
}

fn note(app: &App, ui: &mut Ui, user: Id, out: &mut Vec<Action>) {
    let palette = app.palette;
    let id = egui::Id::new(("note", user));
    let saved = profile(app, user)
        .and_then(|p| p.note.clone())
        .unwrap_or_default();
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or(saved);
    let response = ui.add(
        egui::TextEdit::multiline(&mut text)
            .id(id.with("field"))
            .hint_text(RichText::new("Clique para adicionar uma nota").color(palette.muted))
            .frame(Frame::NONE)
            .desired_rows(1)
            .font(font(Weight::Regular, 12.0))
            .text_color(palette.text)
            .desired_width(f32::INFINITY),
    );
    if response.has_focus() || response.changed() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    if response.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        out.push(Action::Chat(ChatAction::SaveNote { user, note: text }));
    }
}

/// "Conversar com @name": Enter opens the direct message and sends.
fn message_field(app: &App, ui: &mut Ui, user: Id, name: &str, out: &mut Vec<Action>) {
    let palette = app.palette;
    let id = egui::Id::new(("card-message", user));
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let focused = ui.memory(|m| m.has_focus(id.with("field")));
    let enter = focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    Frame::new()
        .fill(palette.rail)
        .corner_radius(CornerRadius::same(8))
        .stroke(Stroke::new(1.0, palette.divider))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .id(id.with("field"))
                    .hint_text(RichText::new(format!("Conversar com @{name}")).color(palette.muted))
                    .frame(Frame::NONE)
                    .font(font(Weight::Regular, 14.0))
                    .text_color(palette.text)
                    .desired_width(f32::INFINITY),
            );
            if response.changed() {
                ui.data_mut(|d| d.insert_temp(id, text.clone()));
            }
        });
    if enter {
        ui.data_mut(|d| d.remove::<String>(id));
        out.push(Action::Chat(ChatAction::MessageUser { user, text }));
    }
}

fn paint_status(ui: &Ui, user: &User, at: egui::Pos2, dot: f32, behind: Color32, offline: Color32) {
    let palette = crate::theme::Palette::dark();
    let color = if user.status == Status::Offline {
        offline
    } else {
        status_color(&palette, user.status)
    };
    ui.painter().circle_filled(at, dot, color);
    match user.status {
        Status::Offline => {
            ui.painter().circle_filled(at, dot * 0.5, behind);
        }
        Status::Idle => {
            ui.painter()
                .circle_filled(at - vec2(dot * 0.45, dot * 0.45), dot * 0.6, behind);
        }
        Status::DoNotDisturb => {
            ui.painter().rect_filled(
                Rect::from_center_size(at, vec2(dot * 1.2, dot * 0.45)),
                CornerRadius::same(2),
                behind,
            );
        }
        Status::Online => {}
    }
}

/// Discord's mark beside "member since", drawn as a small rounded square.
fn paint_small_logo(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(5), color);
    paint_icon(ui, Icon::Threads, rect.center(), 11.0, Color32::WHITE);
}

/// "24 de out. de 2017".
fn short_date(time: jiff::Timestamp) -> String {
    let date = time.to_zoned(jiff::tz::TimeZone::system()).date();
    let long = long_date(date);
    let mut parts: Vec<&str> = long.split(' ').collect();
    let short;
    if parts.len() == 5 {
        short = format!("{}.", &parts[2].chars().take(3).collect::<String>());
        parts[2] = &short;
    }
    parts.join(" ")
}

fn clock(seconds: i64) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn connection_name(kind: &str) -> &str {
    match kind {
        "steam" => "Steam",
        "spotify" => "Spotify",
        "github" => "GitHub",
        "twitch" => "Twitch",
        "youtube" => "YouTube",
        "xbox" => "Xbox",
        "playstation" => "PlayStation",
        "battlenet" => "Battle.net",
        "epicgames" => "Epic Games",
        "riotgames" => "Riot Games",
        "reddit" => "Reddit",
        "twitter" => "X",
        "tiktok" => "TikTok",
        "instagram" => "Instagram",
        other => other,
    }
}

/// "No que você está pensando?": the bubble beside the user's own avatar,
/// with their custom status when they have one.
fn thought(
    app: &App,
    ui: &mut Ui,
    user: &crate::model::User,
    avatar: egui::Pos2,
    banner: Rect,
    out: &mut Vec<Action>,
) {
    let palette = app.palette;
    let custom = user
        .activities
        .iter()
        .find(|a| a.kind == ActivityKind::Custom)
        .and_then(|a| a.state.clone());
    let left = avatar.x + AVATAR / 2.0 + 14.0;
    let bubble = Rect::from_min_max(
        pos2(left, banner.bottom() - 30.0),
        pos2(banner.right() - 12.0, banner.bottom() + 14.0),
    );
    let response = ui.interact(bubble, ui.id().with("thought"), Sense::click());
    let fill = if response.hovered() {
        crate::theme::mix(palette.rail, palette.hover, 0.5)
    } else {
        palette.rail
    };
    // The two little dots that make it a thought.
    ui.painter()
        .circle_filled(pos2(left - 9.0, bubble.top() + 4.0), 5.0, fill);
    ui.painter()
        .circle_filled(pos2(left - 16.0, bubble.top() - 4.0), 2.5, fill);
    ui.painter().rect_filled(bubble, CornerRadius::same(14), fill);
    let y = bubble.center().y;
    match custom.clone() {
        Some(text) => {
            super::widgets::Elide::new(ui, bubble.right() - 10.0).text(
                pos2(bubble.left() + 12.0, y),
                Align2::LEFT_CENTER,
                text,
                font(Weight::Medium, 13.0),
                palette.text,
            );
        }
        None => {
            ui.painter()
                .circle_filled(pos2(bubble.left() + 16.0, y), 8.0, palette.hover);
            super::widgets::paint_icon(
                ui,
                crate::icons::Icon::Plus,
                pos2(bubble.left() + 16.0, y),
                12.0,
                palette.text,
            );
            let mut job = egui::text::LayoutJob::single_section(
                "No que você está pensando?".into(),
                egui::TextFormat {
                    font_id: font(Weight::Medium, 13.0),
                    color: palette.muted,
                    italics: true,
                    ..Default::default()
                },
            );
            job.wrap.max_width = bubble.right() - bubble.left() - 40.0;
            job.wrap.max_rows = 2;
            let galley = ui.painter().layout_job(job);
            ui.painter().galley(
                pos2(bubble.left() + 30.0, y - galley.size().y / 2.0),
                galley,
                palette.muted,
            );
        }
    }
    if response.on_hover_text("Definir um status personalizado").clicked() {
        out.push(Action::Admin(crate::admin::AdminAction::Open(
            crate::admin::Modal::CustomStatus {
                text: custom.unwrap_or_default(),
            },
        )));
    }
}

/// The rest of the user's own card, as Discord's: their badges in a row,
/// the call they are in, and the account menu.
fn own_card(app: &App, ui: &mut Ui, badges: &[crate::model::Badge], out: &mut Vec<Action>) {
    let palette = app.palette;
    let boxed = crate::theme::current().rail.gamma_multiply(0.6);
    if !badges.is_empty() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            Frame::new()
                .fill(boxed)
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for badge in badges.iter().take(6) {
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                            if let Some(uri) = app.picture(Some(&badge.icon)) {
                                egui::Image::new(uri).paint_at(ui, rect);
                            }
                            if response.on_hover_text(&badge.description).clicked()
                                && let Some(link) = &badge.link
                            {
                                out.push(Action::Chat(ChatAction::OpenUrl(link.clone())));
                            }
                        }
                        if badges.len() > 6 {
                            ui.label(
                                RichText::new(format!("+{}", badges.len() - 6))
                                    .font(font(Weight::SemiBold, 12.0))
                                    .color(palette.text),
                            );
                        }
                    });
                });
        });
        ui.add_space(8.0);
    }

    // The call the account is in, here or on another device.
    let me = app.model.me;
    let voice = app
        .model
        .voice
        .iter()
        .find(|((_, user), _)| *user == me)
        .map(|((guild, _), v)| (*guild, v.channel));
    if let Some((guild, channel)) = voice {
        Frame::new()
            .fill(boxed)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
                ui.label(
                    RichText::new("Em voz")
                        .font(font(Weight::SemiBold, 12.0))
                        .color(palette.strong),
                );
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (faces, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
                    ui.painter()
                        .rect_filled(faces, CornerRadius::same(8), palette.rail);
                    let people = app.model.in_voice(channel);
                    let spots = [vec2(-10.0, -10.0), vec2(10.0, -4.0), vec2(-6.0, 12.0)];
                    for ((user, _), spot) in people.iter().zip(spots) {
                        if let Some(person) = app.model.user(*user) {
                            super::widgets::avatar(
                                ui,
                                app,
                                person,
                                faces.center() + spot,
                                10.0,
                                None,
                            );
                        }
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add_space(8.0);
                        let name = app
                            .model
                            .channel(channel)
                            .map(|c| c.name.clone())
                            .unwrap_or_default();
                        ui.label(
                            RichText::new(format!("🔊 {name}"))
                                .font(font(Weight::SemiBold, 14.0))
                                .color(palette.strong),
                        );
                        if let Some(g) = guild.and_then(|g| app.model.guild(g)) {
                            ui.label(
                                RichText::new(format!("em {}", g.name))
                                    .font(font(Weight::Regular, 12.0))
                                    .color(palette.text),
                            );
                        }
                    });
                });
                let here = app.call.as_ref().is_some_and(|c| c.channel == channel);
                let (label, action) = if here {
                    ("Abrir chamada", Action::ShowCall(true))
                } else {
                    ("Entrar na chamada de voz", Action::JoinVoice(channel))
                };
                if wide_button(ui, label, palette.hover, palette.strong).clicked() {
                    out.push(action);
                    out.push(Action::Chat(ChatAction::CloseProfile));
                }
            });
        ui.add_space(8.0);
    }

    // The account menu.
    let status = app.model.user(me).map(|u| u.status).unwrap_or_default();
    Frame::new()
        .fill(boxed)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            if menu_row(ui, app, MenuIcon::Icon(crate::icons::Icon::Pencil), "Editar perfil", false)
                .clicked()
            {
                out.push(Action::OpenSettings(crate::app::SettingsPage::Profiles));
                out.push(Action::Chat(ChatAction::CloseProfile));
            }
            separator(ui, palette);
            let row = menu_row(ui, app, MenuIcon::Status(status), status_name(status), true);
            egui::Popup::menu(&row)
                .align(egui::RectAlign::RIGHT_START)
                .show(|ui| {
                    ui.set_min_width(220.0);
                    for choice in [
                        crate::model::Status::Online,
                        crate::model::Status::Idle,
                        crate::model::Status::DoNotDisturb,
                        crate::model::Status::Offline,
                    ] {
                        let label = format!("●  {}", status_name(choice));
                        let text = RichText::new(label).color(super::widgets::status_color(&palette, choice));
                        if ui.button(text).clicked() {
                            out.push(Action::Admin(crate::admin::AdminAction::SetStatus(choice)));
                            ui.close();
                        }
                    }
                });
        });
    ui.add_space(8.0);
    Frame::new()
        .fill(boxed)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let row = menu_row(ui, app, MenuIcon::Icon(crate::icons::Icon::User), "Mudar de conta", true);
            egui::Popup::menu(&row)
                .align(egui::RectAlign::RIGHT_START)
                .show(|ui| {
                    ui.set_min_width(200.0);
                    if ui
                        .button(RichText::new("Sair").color(palette.danger))
                        .clicked()
                    {
                        out.push(Action::AskLogout);
                        out.push(Action::Chat(ChatAction::CloseProfile));
                        ui.close();
                    }
                });
        });
}

fn status_name(status: crate::model::Status) -> &'static str {
    match status {
        crate::model::Status::Online => "Disponível",
        crate::model::Status::Idle => "Ausente",
        crate::model::Status::DoNotDisturb => "Não perturbar",
        crate::model::Status::Offline => "Invisível",
    }
}

enum MenuIcon {
    Icon(crate::icons::Icon),
    Status(crate::model::Status),
}

/// One line of the account menu: an icon, the label, and a chevron when it
/// opens more.
fn menu_row(
    ui: &mut Ui,
    app: &App,
    icon: MenuIcon,
    label: &str,
    more: bool,
) -> egui::Response {
    let palette = app.palette;
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.hover);
    }
    let y = rect.center().y;
    let at = pos2(rect.left() + 16.0, y);
    match icon {
        MenuIcon::Icon(icon) => super::widgets::paint_icon(ui, icon, at, 16.0, palette.icon),
        MenuIcon::Status(status) => {
            ui.painter()
                .circle_filled(at, 6.0, super::widgets::status_color(&palette, status));
        }
    }
    ui.painter().text(
        pos2(rect.left() + 34.0, y),
        Align2::LEFT_CENTER,
        label,
        font(Weight::Medium, 14.0),
        if response.hovered() { palette.strong } else { palette.text },
    );
    if more {
        super::widgets::paint_icon(
            ui,
            crate::icons::Icon::ChevronRight,
            pos2(rect.right() - 14.0, y),
            16.0,
            palette.icon,
        );
    }
    response
}

fn separator(ui: &mut Ui, palette: crate::theme::Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 9.0), Sense::hover());
    ui.painter().hline(
        (rect.left() + 8.0)..=(rect.right() - 8.0),
        rect.center().y,
        egui::Stroke::new(1.0, palette.selected),
    );
}

fn wide_button(ui: &mut Ui, label: &str, fill: Color32, text: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let fill = if response.hovered() {
        crate::theme::mix(fill, Color32::WHITE, 0.08)
    } else {
        fill
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font(Weight::Medium, 14.0),
        text,
    );
    response
}
