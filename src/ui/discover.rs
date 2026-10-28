//! Discord's "Descubra": Apps, Servidores and Missões beside the rail, and
//! the featured servers in cards under the big welcome.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};
use fastframe_fonts::Weight;

use super::widgets::{Elide, font, paint_icon};
use crate::admin::{AdminAction, DiscoverGuild};
use crate::app::{Action, App};
use crate::icons::Icon;
use crate::theme::SIDEBAR_RANGE;

/// The column beside the rail, in place of the channels.
pub fn sidebar(app: &mut App, ui: &mut Ui) -> Rect {
    let palette = app.palette;
    egui::Panel::left("discover-sidebar")
        .resizable(false)
        .exact_size(super::sidebar::width(app).clamp(*SIDEBAR_RANGE.start(), *SIDEBAR_RANGE.end()))
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius { nw: 8, ne: 0, sw: 0, se: 0 }),
        )
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let header = Rect::from_min_size(rect.min, vec2(rect.width(), crate::theme::HEADER_HEIGHT));
            ui.painter().text(
                pos2(header.left() + 16.0, header.center().y),
                Align2::LEFT_CENTER,
                "Descubra",
                font(Weight::Bold, 20.0),
                palette.strong,
            );
            ui.painter().hline(header.x_range(), header.bottom() - 0.5, Stroke::new(1.0, palette.divider));
            let mut y = header.bottom() + 8.0;
            for (icon, label, url) in [
                (Icon::Activities, "Apps", Some("https://discord.com/discovery/applications")),
                (Icon::Compass, "Servidores", None),
                (Icon::Star, "Missões", Some("https://discord.com/discovery/quests")),
            ] {
                let row = Rect::from_min_size(pos2(rect.left() + 8.0, y), vec2(rect.width() - 16.0, 44.0));
                y += 48.0;
                let response = ui.interact(row, ui.id().with(label), Sense::click());
                let selected = url.is_none();
                if selected || response.hovered() {
                    ui.painter().rect_filled(
                        row,
                        CornerRadius::same(8),
                        if selected { palette.selected } else { palette.hover },
                    );
                }
                let tint = if selected || response.hovered() { palette.strong } else { palette.muted };
                paint_icon(ui, icon, pos2(row.left() + 20.0, row.center().y), 22.0, tint);
                ui.painter().text(
                    pos2(row.left() + 46.0, row.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    font(Weight::Medium, 17.0),
                    tint,
                );
                if response.clicked()
                    && let Some(url) = url
                {
                    app.actions
                        .push(Action::Chat(crate::chat::ChatAction::OpenUrl(url.into())));
                }
            }
            // Room for the user panel, drawn over the foot.
            ui.allocate_rect(rect, Sense::hover());
        })
        .response
        .rect
}

/// The featured servers.
pub fn page(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::CentralPanel::default()
        .frame(Frame::new().fill(palette.chat))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("discover")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let width = ui.available_width();
                    hero(ui, palette, width);
                    Frame::new()
                        .inner_margin(Margin::symmetric(32, 24))
                        .show(ui, |ui| {
                            ui.set_width(width - 64.0);
                            ui.label(
                                egui::RichText::new("Servidores em Destaque")
                                    .font(font(Weight::Bold, 22.0))
                                    .color(palette.strong),
                            );
                            ui.add_space(16.0);
                            let Some(discover) = &app.admin.discover else {
                                return;
                            };
                            if discover.loading {
                                ui.spinner();
                                return;
                            }
                            if let Some(error) = &discover.error {
                                ui.label(
                                    egui::RichText::new(format!("Não foi possível carregar: {error}"))
                                        .color(palette.danger),
                                );
                                return;
                            }
                            let guilds = discover.guilds.clone();
                            cards(app, ui, &guilds);
                        });
                });
        });
}

/// "Encontre sua comunidade no Discord" on Discord's blue.
fn hero(ui: &mut Ui, palette: crate::theme::Palette, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 330.0), Sense::hover());
    let top = Color32::from_rgb(0x2b, 0x2d, 0x8a);
    let bottom = Color32::from_rgb(0x1b, 0x1e, 0x4f);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), Color32::from_rgb(0x3b, 0x2f, 0x9a));
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
    let left = rect.left() + 36.0;
    let mut job = egui::text::LayoutJob::single_section(
        "ENCONTRE SUA COMUNIDADE NO DISCORD".into(),
        egui::TextFormat {
            font_id: font(Weight::Bold, 44.0),
            color: Color32::WHITE,
            line_height: Some(46.0),
            ..Default::default()
        },
    );
    job.wrap.max_width = (width - 72.0).min(440.0);
    let galley = ui.painter().layout_job(job);
    let title_bottom = rect.top() + 48.0 + galley.size().y;
    ui.painter().galley(pos2(left, rect.top() + 48.0), galley, Color32::WHITE);
    let mut job = egui::text::LayoutJob::single_section(
        "Há sempre um lugar para você, seja para jogar, ouvir música ou aprender.".into(),
        egui::TextFormat {
            font_id: font(Weight::Regular, 16.0),
            color: Color32::from_white_alpha(200),
            ..Default::default()
        },
    );
    job.wrap.max_width = (width - 72.0).min(420.0);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos2(left, title_bottom + 16.0), galley, palette.text);
}

/// The servers in a grid of Discord's cards: the splash, the icon over its
/// edge, the name, the description and the counts.
fn cards(app: &mut App, ui: &mut Ui, guilds: &[DiscoverGuild]) {
    let gap = 16.0;
    let columns = ((ui.available_width() + gap) / (300.0 + gap)).floor().max(1.0);
    let card_w = (ui.available_width() - gap * (columns - 1.0)) / columns;
    let card_h = 330.0;
    for row in guilds.chunks(columns as usize) {
        let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), card_h), Sense::hover());
        for (i, guild) in row.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(line.left() + i as f32 * (card_w + gap), line.top()),
                vec2(card_w, card_h),
            );
            card(app, ui, guild, rect);
        }
        ui.add_space(gap);
    }
}

fn card(app: &mut App, ui: &mut Ui, guild: &DiscoverGuild, rect: Rect) {
    let palette = app.palette;
    let response = ui.interact(rect, ui.id().with(("discover", guild.id)), Sense::click());
    let lift = if response.hovered() { -2.0 } else { 0.0 };
    let rect = rect.translate(vec2(0.0, lift));
    let fill = if response.hovered() { palette.hover } else { palette.sidebar };
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        fill,
        Stroke::new(1.0, palette.selected),
        egui::StrokeKind::Inside,
    );
    let splash = Rect::from_min_size(rect.min, vec2(rect.width(), 143.0));
    let top = CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 };
    ui.painter().rect_filled(splash, top, palette.selected);
    if let Some(uri) = app.picture(guild.splash.as_deref()) {
        egui::Image::new(uri).corner_radius(top).paint_at(ui, splash);
    }
    let icon = Rect::from_min_size(pos2(rect.left() + 12.0, splash.bottom() - 28.0), Vec2::splat(56.0));
    ui.painter()
        .rect_filled(icon.expand(4.0), CornerRadius::same(16), fill);
    match app.picture(guild.icon.as_deref()) {
        Some(uri) => egui::Image::new(uri)
            .corner_radius(CornerRadius::same(14))
            .paint_at(ui, icon),
        None => {
            ui.painter()
                .rect_filled(icon, CornerRadius::same(14), palette.accent);
        }
    }
    let mut x = rect.left() + 16.0;
    let name_y = icon.bottom() + 22.0;
    if guild.verified {
        ui.painter().circle_filled(pos2(x + 8.0, name_y), 8.0, palette.online);
        paint_icon(ui, Icon::Check, pos2(x + 8.0, name_y), 11.0, Color32::WHITE);
        x += 22.0;
    }
    Elide::new(ui, rect.right() - 16.0).text(
        pos2(x, name_y),
        Align2::LEFT_CENTER,
        guild.name.clone(),
        font(Weight::Bold, 17.0),
        palette.strong,
    );
    let mut job = egui::text::LayoutJob::single_section(
        guild.description.clone(),
        egui::TextFormat {
            font_id: font(Weight::Regular, 14.0),
            color: palette.text,
            ..Default::default()
        },
    );
    job.wrap.max_width = rect.width() - 32.0;
    job.wrap.max_rows = 3;
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .galley(pos2(rect.left() + 16.0, name_y + 14.0), galley, palette.text);
    let foot = rect.bottom() - 22.0;
    let online = format!("{} online", thousands(guild.online));
    ui.painter()
        .circle_filled(pos2(rect.left() + 20.0, foot), 4.0, palette.online);
    let written = ui.painter().text(
        pos2(rect.left() + 30.0, foot),
        Align2::LEFT_CENTER,
        online,
        font(Weight::Regular, 12.0),
        palette.muted,
    );
    ui.painter()
        .circle_filled(pos2(written.right() + 14.0, foot), 4.0, palette.offline);
    ui.painter().text(
        pos2(written.right() + 24.0, foot),
        Align2::LEFT_CENTER,
        format!("{} membros", thousands(guild.members)),
        font(Weight::Regular, 12.0),
        palette.muted,
    );
    if response.on_hover_text("Entrar no servidor").clicked() {
        app.actions
            .push(Action::Admin(AdminAction::JoinDiscoverable(guild.id)));
    }
}

/// 4610444 as "4.610.444", as Discord writes counts in Portuguese.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}
