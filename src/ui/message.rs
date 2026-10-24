//! One message as Discord draws it: the reply it answers, the author line,
//! the markdown text with custom emoji, then attachments, embeds, stickers
//! and reactions; the action bar and context menu on hover.

use std::ops::Range;

use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Layout, Margin,
    Rect, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::player;
use super::widgets::{avatar, clock, font, hyphens, paint_icon, timestamp};
use crate::app::{Action, App};
use crate::chat::{ChatAction, PickerTab, PickerTarget};
use crate::icons::Icon;
use crate::markdown::{self, Block, Inline, Style};
use crate::model::{
    Attachment, CustomEmoji, Embed, EmbedMedia, Id, MediaKind, Message, ReactionEmoji,
};

pub const GUTTER: f32 = 72.0;
const LINK: Color32 = Color32::from_rgb(0x00, 0xa8, 0xfc);
const MEDIA_W: f32 = 400.0;
const MEDIA_H: f32 = 300.0;

pub struct Row<'a> {
    pub app: &'a App,
    pub guild: Option<Id>,
    pub channel: Id,
    pub out: &'a mut Vec<Action>,
}

impl Row<'_> {
    fn act(&mut self, action: ChatAction) {
        self.out.push(Action::Chat(action));
    }
}

/// Draws a message across the full width of `ui`.
pub fn show(row: &mut Row<'_>, ui: &mut Ui, message: &Message, continues: bool) {
    let app = row.app;
    let palette = app.palette;
    let width = ui.available_width();
    let left = ui.max_rect().left();
    let top = ui.cursor().top();
    let id = ui
        .id()
        .with(("message", message.id, message.nonce.as_deref()));
    // The row's own interaction goes first, so links and pictures on top
    // of it keep their clicks; its height is last frame's.
    let last_height: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(44.0);
    let row_response = ui.interact(
        Rect::from_min_size(pos2(left, top), vec2(width, last_height)),
        id,
        Sense::click(),
    );
    super::no_hand(ui, row_response.id);
    let background = ui.painter().add(Shape::Noop);

    let content = Rect::from_min_max(
        pos2(left + GUTTER, top + 2.0),
        pos2(left + width - 48.0, top + 100_000.0),
    );
    let system = system_text(app, row.guild, message);
    let mut header_top = top + 2.0;
    let inner = ui.scope_builder(
        UiBuilder::new()
            .max_rect(content)
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            if let Some(text) = &system {
                system_line(ui, app, text);
                return;
            }
            if message.reply.is_some() || message.command.is_some() {
                reply_line(row, ui, message, left);
                header_top = ui.cursor().top();
            }
            if !continues || message.reply.is_some() || message.command.is_some() {
                header(row, ui, message);
            }
            let editing = app
                .chat
                .editing
                .as_ref()
                .is_some_and(|e| e.message == message.id && e.channel == row.channel);
            if editing {
                edit_box(row, ui);
            } else {
                body(row, ui, message);
            }
            media(row, ui, message);
            if !message.reactions.is_empty() {
                ui.add_space(4.0);
                reactions(row, ui, message);
            }
        },
    );
    let bottom = inner
        .response
        .rect
        .bottom()
        .max(top + if continues { 22.0 } else { 44.0 })
        + 2.0;
    let rect = Rect::from_min_max(pos2(left, top), pos2(left + width, bottom));
    ui.advance_cursor_after_rect(rect);
    ui.data_mut(|d| d.insert_temp(id, rect.height()));

    let menu_open = row_response.context_menu_opened();
    let hovered = ui.rect_contains_pointer(rect.expand2(vec2(0.0, 1.0))) || menu_open;
    if message.mentions_me {
        ui.painter().set(
            background,
            Shape::rect_filled(rect, CornerRadius::ZERO, palette.mention),
        );
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            CornerRadius::ZERO,
            palette.mention_bar,
        );
    } else if hovered {
        ui.painter().set(
            background,
            Shape::rect_filled(rect, CornerRadius::ZERO, palette.message_hover),
        );
    }

    if system.is_some() {
        let icon = if message.kind == 7 {
            Icon::ReplyArrow
        } else {
            Icon::Pin
        };
        paint_icon(
            ui,
            icon,
            pos2(left + 36.0, top + 13.0),
            16.0,
            if message.kind == 7 {
                palette.online
            } else {
                palette.muted
            },
        );
    } else if !continues || message.reply.is_some() || message.command.is_some() {
        if let Some(author) = app.model.user(message.author) {
            avatar(
                ui,
                app,
                author,
                pos2(left + 36.0, header_top + 20.0),
                20.0,
                None,
            );
        }
    } else if hovered {
        ui.painter().text(
            pos2(left + GUTTER - 10.0, top + 13.0),
            Align2::RIGHT_CENTER,
            clock(message.time),
            font(Weight::Regular, 11.0),
            palette.muted,
        );
    }

    if hovered && !message.pending && system.is_none() {
        toolbar(row, ui, message, rect);
    }
    let own = message.author == app.model.me;
    let (channel, id) = (row.channel, message.id);
    let text = message.text.clone();
    let mut chosen = None;
    row_response.context_menu(|ui| {
        ui.set_min_width(180.0);
        if ui.button("Adicionar reação").clicked() {
            chosen = Some(ChatAction::OpenPicker {
                tab: PickerTab::Emoji,
                target: PickerTarget::Reaction {
                    channel,
                    message: id,
                },
            });
            ui.close();
        }
        if own && ui.button("Editar mensagem").clicked() {
            chosen = Some(ChatAction::StartEdit {
                channel,
                message: id,
            });
            ui.close();
        }
        if ui.button("Responder").clicked() {
            chosen = Some(ChatAction::Reply {
                channel,
                message: id,
            });
            ui.close();
        }
        if !text.is_empty() && ui.button("Copiar texto").clicked() {
            chosen = Some(ChatAction::Copy(text.clone()));
            ui.close();
        }
        if ui.button("Copiar ID da mensagem").clicked() {
            chosen = Some(ChatAction::Copy(id.to_string()));
            ui.close();
        }
        if own {
            ui.separator();
            let delete =
                egui::Button::new(egui::RichText::new("Excluir mensagem").color(palette.danger));
            if ui.add(delete).clicked() {
                chosen = Some(ChatAction::AskDelete {
                    channel,
                    message: id,
                });
                ui.close();
            }
        }
    });
    if let Some(action) = chosen {
        row.act(action);
    }
}

/// The line of joins, pins and boosts, which have no author line.
fn system_text(app: &App, guild: Option<Id>, message: &Message) -> Option<String> {
    let name = app.model.name_in(guild, message.author);
    Some(match message.kind {
        7 => format!("{name} entrou no servidor."),
        6 => format!("{name} fixou uma mensagem neste canal."),
        8 => format!("{name} impulsionou o servidor!"),
        9..=11 => format!("{name} impulsionou o servidor, que subiu de nível!"),
        1 => format!("{name} adicionou alguém ao grupo."),
        2 => format!("{name} removeu alguém do grupo."),
        4 => format!("{name} mudou o nome do canal."),
        5 => format!("{name} mudou o ícone do grupo."),
        18 => format!("{name} começou um tópico."),
        _ => return None,
    })
}

fn system_line(ui: &mut Ui, app: &App, text: &str) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(hyphens(text))
            .font(font(Weight::Regular, 15.0))
            .color(app.palette.muted),
    );
    ui.add_space(2.0);
}

/// "↱ Name  quoted text", above the author line.
fn reply_line(row: &mut Row<'_>, ui: &mut Ui, message: &Message, left: f32) {
    let app = row.app;
    let palette = app.palette;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    // The spine from the avatar column up into the quote.
    let spine = Stroke::new(2.0, palette.selected);
    let x0 = left + 36.0;
    let y = rect.center().y;
    ui.painter()
        .line_segment([pos2(x0, y + 8.0), pos2(x0, y + 2.0)], spine);
    ui.painter()
        .line_segment([pos2(x0, y), pos2(rect.left() - 6.0, y)], spine);
    let mut x = rect.left();
    if let Some(command) = &message.command {
        let name = app.model.name_in(row.guild, message.author);
        ui.painter().text(
            pos2(x, y),
            Align2::LEFT_CENTER,
            format!("{name} usou {command}"),
            font(Weight::Medium, 14.0),
            palette.muted,
        );
        return;
    }
    let Some(reply) = &message.reply else { return };
    match reply.author {
        Some(author) => {
            if let Some(user) = app.model.user(author) {
                avatar(ui, app, user, pos2(x + 8.0, y), 8.0, None);
            }
            x += 20.0;
            let color = app
                .model
                .name_color(row.guild, author)
                .unwrap_or(palette.text);
            let name = ui.painter().layout_no_wrap(
                format!("@{}", app.model.name_in(row.guild, author)),
                font(Weight::Medium, 14.0),
                color,
            );
            let w = name.size().x;
            ui.painter()
                .galley(pos2(x, y - name.size().y / 2.0), name, color);
            x += w + 6.0;
            let excerpt = match &reply.text {
                Some(text) if !text.is_empty() => {
                    text.lines().next().unwrap_or_default().to_owned()
                }
                _ if reply.has_media => "Clique para ver o anexo".to_owned(),
                _ => String::new(),
            };
            super::widgets::Elide::new(ui, rect.right()).text(
                pos2(x, y),
                Align2::LEFT_CENTER,
                hyphens(&excerpt),
                font(Weight::Regular, 14.0),
                palette.muted,
            );
        }
        None => {
            ui.painter().text(
                pos2(x, y),
                Align2::LEFT_CENTER,
                "A mensagem original foi apagada",
                FontId::new(14.0, FontFamily::Proportional),
                palette.muted,
            );
        }
    }
}

fn header(row: &mut Row<'_>, ui: &mut Ui, message: &Message) {
    let app = row.app;
    let guild = row.guild;
    let palette = app.palette;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let color = app
        .model
        .name_color(guild, message.author)
        .unwrap_or(palette.strong);
    let name = ui.painter().layout_no_wrap(
        app.model.name_in(guild, message.author),
        font(Weight::Medium, 16.0),
        color,
    );
    let name_rect = Rect::from_min_size(rect.min, name.size());
    let mut x = rect.left() + name.size().x;
    // Discord underlines the name under the pointer; a click opens the profile.
    let name_response = ui.interact(
        name_rect,
        ui.id().with(("author", message.id)),
        Sense::click(),
    );
    if name_response.hovered() {
        ui.painter().hline(
            name_rect.x_range(),
            name_rect.bottom() - 1.0,
            Stroke::new(1.0, color),
        );
    }
    if name_response.clicked() {
        row.act(ChatAction::ShowProfile {
            user: message.author,
            anchor: name_rect,
            side: crate::chat::Side::Right,
        });
    }
    user_menu(row, &name_response, message.author);
    ui.painter().galley(rect.min, name, color);
    if app.model.user(message.author).is_some_and(|u| u.bot) {
        let tag = Rect::from_min_size(pos2(x + 4.0, rect.top() + 4.0), vec2(30.0, 15.0));
        ui.painter()
            .rect_filled(tag, CornerRadius::same(3), palette.accent);
        ui.painter().text(
            tag.center(),
            Align2::CENTER_CENTER,
            "APP",
            font(Weight::SemiBold, 10.0),
            Color32::WHITE,
        );
        x += 34.0;
    }
    let time = ui.painter().text(
        pos2(x + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        timestamp(message.time),
        font(Weight::Regular, 12.0),
        palette.muted,
    );
    // The full date under the pointer, as Discord's tooltip.
    ui.interact(time, ui.id().with(("time", message.id)), Sense::hover())
        .on_hover_text(full_date(message.time));
}

/// "sexta-feira, 24 de outubro de 2026 10:04".
pub fn full_date(time: jiff::Timestamp) -> String {
    const DAYS: [&str; 7] = [
        "segunda-feira",
        "terça-feira",
        "quarta-feira",
        "quinta-feira",
        "sexta-feira",
        "sábado",
        "domingo",
    ];
    let zoned = time.to_zoned(jiff::tz::TimeZone::system());
    let day = DAYS[zoned.weekday().to_monday_zero_offset() as usize];
    format!(
        "{day}, {} {}",
        super::widgets::long_date(zoned.date()),
        zoned.strftime("%H:%M")
    )
}

/// The right-click menu on a person, as Discord's.
pub fn user_menu(row: &mut Row<'_>, response: &egui::Response, user: Id) {
    let app = row.app;
    let username = app
        .model
        .user(user)
        .map(|u| u.username.clone())
        .unwrap_or_default();
    let me = user == app.model.me;
    let anchor = response.rect;
    let mut chosen = None;
    response.context_menu(|ui| {
        ui.set_min_width(180.0);
        if ui.button("Perfil").clicked() {
            chosen = Some(ChatAction::ShowProfile {
                user,
                anchor,
                side: crate::chat::Side::Right,
            });
            ui.close();
        }
        if ui.button("Mencionar").clicked() {
            chosen = Some(ChatAction::Mention(user));
            ui.close();
        }
        if !me && ui.button("Mensagem").clicked() {
            chosen = Some(ChatAction::MessageUser {
                user,
                text: String::new(),
            });
            ui.close();
        }
        ui.separator();
        if ui.button("Copiar nome de usuário").clicked() {
            chosen = Some(ChatAction::Copy(username.clone()));
            ui.close();
        }
        if ui.button("Copiar ID do usuário").clicked() {
            chosen = Some(ChatAction::Copy(user.to_string()));
            ui.close();
        }
    });
    if let Some(action) = chosen {
        row.act(action);
    }
}

/// The text, unless it is only the link of an embed that shows instead.
fn body(row: &mut Row<'_>, ui: &mut Ui, message: &Message) {
    let app = row.app;
    let trimmed = message.text.trim();
    let hidden = message
        .embeds
        .iter()
        .any(|e| matches!(e.kind.as_str(), "gifv" | "image") && e.url.as_deref() == Some(trimmed));
    if trimmed.is_empty() || hidden {
        if message.edited && !trimmed.is_empty() {
            return;
        }
        if message.failed {
            failed_note(ui, app);
        }
        return;
    }
    let parsed = markdown::parse(&message.text);
    let jumbo = markdown::only_emoji(&parsed);
    let style = TextStyle::message(app, message, jumbo);
    blocks(row, ui, message.id, &parsed, &style, message.edited);
    if message.failed {
        failed_note(ui, app);
    }
}

fn failed_note(ui: &mut Ui, app: &App) {
    ui.label(
        egui::RichText::new("Não foi possível enviar esta mensagem.")
            .size(12.0)
            .color(app.palette.danger),
    );
}

#[derive(Clone, Copy)]
pub struct TextStyle {
    pub size: f32,
    pub color: Color32,
    pub muted: Color32,
}

impl TextStyle {
    fn message(app: &App, message: &Message, jumbo: bool) -> Self {
        let color = if message.failed {
            app.palette.danger
        } else if message.pending {
            app.palette.muted
        } else {
            app.palette.text
        };
        Self {
            size: if jumbo { 48.0 } else { 16.0 },
            color,
            muted: app.palette.muted,
        }
    }

    pub fn embed(app: &App) -> Self {
        Self {
            size: 14.0,
            color: app.palette.text,
            muted: app.palette.muted,
        }
    }
}

/// Markdown blocks, top to bottom.
pub fn blocks(
    row: &mut Row<'_>,
    ui: &mut Ui,
    message: Id,
    list: &[Block],
    style: &TextStyle,
    edited: bool,
) {
    let count = list.len();
    for (i, block) in list.iter().enumerate() {
        let last = i + 1 == count;
        let edited = edited && last;
        match block {
            Block::Paragraph(spans) => text(
                row,
                ui,
                message,
                spans,
                style.size,
                style,
                Weight::Regular,
                edited,
            ),
            Block::Heading(level, spans) => {
                let size = match level {
                    1 => 24.0,
                    2 => 20.0,
                    _ => 16.0,
                };
                ui.add_space(if i == 0 { 0.0 } else { 8.0 });
                text(row, ui, message, spans, size, style, Weight::Bold, edited);
            }
            Block::Subtext(spans) => {
                let small = TextStyle {
                    color: style.muted,
                    ..*style
                };
                text(
                    row,
                    ui,
                    message,
                    spans,
                    13.0,
                    &small,
                    Weight::Regular,
                    edited,
                );
            }
            Block::Item {
                depth,
                marker,
                content,
            } => {
                ui.horizontal_top(|ui| {
                    ui.add_space(4.0 + *depth as f32 * 16.0);
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(20.0, style.size * 1.375), Sense::hover());
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        marker,
                        font(Weight::Regular, style.size),
                        style.color,
                    );
                    ui.vertical(|ui| {
                        text(
                            row,
                            ui,
                            message,
                            content,
                            style.size,
                            style,
                            Weight::Regular,
                            edited,
                        )
                    });
                });
            }
            Block::Quote(inner) => {
                let start = ui.cursor().top();
                let response = ui.horizontal_top(|ui| {
                    ui.add_space(12.0);
                    ui.vertical(|ui| blocks(row, ui, message, inner, style, edited));
                });
                let r = response.response.rect;
                let bar =
                    Rect::from_min_max(pos2(r.left(), start), pos2(r.left() + 4.0, r.bottom()));
                ui.painter().rect_filled(
                    bar,
                    CornerRadius::same(2),
                    Color32::from_rgb(0x4e, 0x50, 0x58),
                );
            }
            Block::Code { text: code, .. } => {
                ui.add_space(4.0);
                Frame::new()
                    .fill(crate::theme::current().sidebar)
                    .stroke(Stroke::new(1.0, crate::theme::current().divider))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width().min(560.0));
                        let mut job = LayoutJob::single_section(
                            code.trim_end().to_owned(),
                            TextFormat {
                                font_id: FontId::monospace(14.0),
                                color: Color32::from_rgb(0xdb, 0xde, 0xe1),
                                ..Default::default()
                            },
                        );
                        job.wrap.max_width = ui.available_width();
                        let galley = ui.painter().layout_job(job);
                        let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
                        ui.painter().galley(rect.min, galley, Color32::WHITE);
                    });
                ui.add_space(4.0);
            }
        }
    }
}

enum Target {
    Link(String),
    Spoiler(usize),
    User(Id),
    Channel(Id),
}

/// One run of styled text, with custom emoji drawn over placeholders and
/// links and spoilers that answer the pointer.
#[allow(clippy::too_many_arguments)]
fn text(
    row: &mut Row<'_>,
    ui: &mut Ui,
    message: Id,
    spans: &[Inline],
    size: f32,
    style: &TextStyle,
    weight: Weight,
    edited: bool,
) {
    let app = row.app;
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    let mut targets: Vec<(Range<usize>, Target)> = Vec::new();
    let mut emojis: Vec<(usize, String, f32)> = Vec::new();
    let line = size * 1.375;
    let format = |s: &Style, color: Color32| -> TextFormat {
        let w = if s.bold { Weight::Bold } else { weight };
        let font_id = if s.code {
            FontId::monospace(size * 0.875)
        } else {
            font(w, size)
        };
        let hidden = s
            .spoiler
            .is_some_and(|i| !app.chat.revealed.contains(&(message, i)));
        TextFormat {
            font_id,
            color: if hidden {
                crate::theme::current().rail
            } else {
                color
            },
            italics: s.italic,
            underline: if s.underline {
                Stroke::new(1.0, color)
            } else {
                Stroke::NONE
            },
            strikethrough: if s.strike {
                Stroke::new(1.0, color)
            } else {
                Stroke::NONE
            },
            background: if hidden {
                crate::theme::current().rail
            } else if s.spoiler.is_some() {
                Color32::from_white_alpha(20)
            } else if s.code {
                crate::theme::current().sidebar
            } else {
                Color32::TRANSPARENT
            },
            line_height: Some(line),
            ..Default::default()
        }
    };
    let push = |job: &mut LayoutJob, text: &str, f: TextFormat| {
        let start = job.text.len();
        job.append(&hyphens(text), 0.0, f);
        start..job.text.len()
    };
    for span in spans {
        match span {
            Inline::Text(t, s) => {
                let range = push(&mut job, t, format(s, style.color));
                if let Some(i) = s.spoiler {
                    targets.push((range, Target::Spoiler(i)));
                }
            }
            Inline::Link {
                text: t,
                url,
                style: s,
            } => {
                let range = push(&mut job, t, format(s, LINK));
                let target = match s.spoiler {
                    Some(i) if !app.chat.revealed.contains(&(message, i)) => Target::Spoiler(i),
                    _ => Target::Link(url.clone()),
                };
                targets.push((range, target));
            }
            Inline::User(id, s) => {
                let name = app.model.name_in(row.guild, *id);
                let range = push(
                    &mut job,
                    &format!("@{name}"),
                    TextFormat {
                        color: mention_text(),
                        background: mention_bg(),
                        ..format(s, mention_text())
                    },
                );
                targets.push((range, Target::User(*id)));
            }
            Inline::Everyone(word, s) => {
                push(
                    &mut job,
                    word,
                    TextFormat {
                        color: mention_text(),
                        background: mention_bg(),
                        ..format(s, mention_text())
                    },
                );
            }
            Inline::Role(id, s) => {
                let role = row
                    .guild
                    .and_then(|g| app.model.guild(g))
                    .and_then(|g| g.roles.iter().find(|r| r.id == *id));
                let name =
                    role.map_or_else(|| "@cargo-desconhecido".into(), |r| format!("@{}", r.name));
                let color = role.and_then(|r| r.color).unwrap_or(mention_text());
                push(
                    &mut job,
                    &name,
                    TextFormat {
                        color,
                        background: color.gamma_multiply(0.15),
                        ..format(s, color)
                    },
                );
            }
            Inline::Channel(id, s) => {
                let name = app
                    .model
                    .channel(*id)
                    .map_or_else(|| "#canal-desconhecido".into(), |c| format!("#{}", c.name));
                let range = push(
                    &mut job,
                    &name,
                    TextFormat {
                        color: mention_text(),
                        background: mention_bg(),
                        ..format(s, mention_text())
                    },
                );
                targets.push((range, Target::Channel(*id)));
            }
            Inline::Time {
                unix,
                format: f,
                style: s,
            } => {
                push(
                    &mut job,
                    &discord_time(*unix, *f),
                    TextFormat {
                        background: Color32::from_white_alpha(12),
                        ..format(s, style.color)
                    },
                );
            }
            Inline::Emoji {
                id,
                animated,
                name,
                style: s,
            } => {
                let emoji_size = if size >= 48.0 {
                    48.0
                } else {
                    22.0_f32.max(size * 1.375)
                };
                let start = job.text.len();
                // An em space as wide as the emoji holds its place.
                job.append(
                    "\u{2003}",
                    0.0,
                    TextFormat {
                        font_id: font(Weight::Regular, emoji_size),
                        line_height: Some(line.max(emoji_size)),
                        ..format(s, style.color)
                    },
                );
                let _ = name;
                emojis.push((start, CustomEmoji::url(*id, *animated), emoji_size));
            }
        }
    }
    if edited {
        job.append(
            " (editado)",
            0.0,
            TextFormat {
                font_id: font(Weight::Regular, 10.0),
                color: style.muted,
                line_height: Some(line),
                ..Default::default()
            },
        );
    }
    let galley = ui.painter().layout_job(job);
    let sense = if targets.is_empty() {
        Sense::hover()
    } else {
        Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(galley.size(), sense);
    super::no_hand(ui, response.id);
    ui.painter().galley(rect.min, galley.clone(), style.color);
    for (byte, url, emoji_size) in &emojis {
        for placed in &galley.rows {
            if let Some(glyph) = placed
                .row
                .glyphs
                .iter()
                .find(|g| g.cluster as usize == *byte)
            {
                let x = rect.left() + placed.pos.x + glyph.pos.x;
                let center_y = rect.top() + placed.pos.y + placed.row.size.y / 2.0;
                let target = Rect::from_min_size(
                    pos2(x, center_y - emoji_size / 2.0),
                    Vec2::splat(*emoji_size),
                );
                if let Some(uri) = app.picture(Some(url)) {
                    egui::Image::new(uri).paint_at(ui, target);
                }
                break;
            }
        }
    }
    if !targets.is_empty()
        && let Some(pointer) = response.hover_pos()
    {
        let cursor = galley.cursor_from_pos(pointer - rect.min);
        let byte = galley
            .job
            .text
            .char_indices()
            .nth(cursor.index.0)
            .map_or(galley.job.text.len(), |(b, _)| b);
        if let Some((_, target)) = targets.iter().find(|(range, _)| range.contains(&byte)) {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            if response.clicked() {
                let action = match target {
                    Target::Link(url) => ChatAction::OpenUrl(url.clone()),
                    Target::Spoiler(i) => ChatAction::RevealSpoiler(message, *i),
                    Target::User(user) => ChatAction::ShowProfile {
                        user: *user,
                        anchor: Rect::from_center_size(pointer, Vec2::splat(16.0)),
                        side: crate::chat::Side::Right,
                    },
                    Target::Channel(channel) => {
                        row.out.push(Action::OpenChannel(*channel));
                        return;
                    }
                };
                row.act(action);
            }
        }
    }
}

/// `<t:...>` in the style it asks for.
fn discord_time(unix: i64, format: char) -> String {
    let Ok(time) = jiff::Timestamp::from_second(unix) else {
        return "data inválida".into();
    };
    let zoned = time.to_zoned(jiff::tz::TimeZone::system());
    match format {
        't' => zoned.strftime("%H:%M").to_string(),
        'T' => zoned.strftime("%H:%M:%S").to_string(),
        'd' => zoned.strftime("%d/%m/%Y").to_string(),
        'D' => super::widgets::long_date(zoned.date()),
        'R' => {
            let seconds = time.as_second() - jiff::Timestamp::now().as_second();
            let (amount, unit) = match seconds.abs() {
                s if s < 60 => (s, "segundos"),
                s if s < 3600 => (s / 60, "minutos"),
                s if s < 86_400 => (s / 3600, "horas"),
                s if s < 2_592_000 => (s / 86_400, "dias"),
                s if s < 31_536_000 => (s / 2_592_000, "meses"),
                s => (s / 31_536_000, "anos"),
            };
            if seconds < 0 {
                format!("há {amount} {unit}")
            } else {
                format!("em {amount} {unit}")
            }
        }
        'F' => format!(
            "{} {}",
            super::widgets::long_date(zoned.date()),
            zoned.strftime("%H:%M")
        ),
        _ => format!(
            "{} {}",
            super::widgets::long_date(zoned.date()),
            zoned.strftime("%H:%M")
        ),
    }
}

fn edit_box(row: &mut Row<'_>, ui: &mut Ui) {
    let app = row.app;
    let palette = app.palette;
    let mut text = app
        .chat
        .editing
        .as_ref()
        .map(|e| e.text.clone())
        .unwrap_or_default();
    let focus = app.chat.editing.as_ref().is_some_and(|e| e.focus);
    let id = egui::Id::new("edit-message");
    let mut save = false;
    let mut cancel = false;
    Frame::new()
        .fill(palette.composer)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let has_focus = ui.memory(|m| m.has_focus(id));
            if has_focus {
                save = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                cancel = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            }
            let response = ui.add(
                egui::TextEdit::multiline(&mut text)
                    .id(id)
                    .frame(Frame::NONE)
                    .desired_rows(1)
                    .font(font(Weight::Regular, 16.0))
                    .text_color(palette.text)
                    .desired_width(ui.available_width()),
            );
            if focus {
                response.request_focus();
            }
        });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let small = |t: &str, c: Color32| egui::RichText::new(t).size(12.0).color(c);
        ui.label(small("esc para ", palette.text));
        if ui
            .add(egui::Label::new(small("cancelar", LINK)).sense(Sense::click()))
            .clicked()
        {
            cancel = true;
        }
        ui.label(small(" • enter para ", palette.text));
        if ui
            .add(egui::Label::new(small("salvar", LINK)).sense(Sense::click()))
            .clicked()
        {
            save = true;
        }
    });
    // Write the text back through an action-free path: the editing state
    // is the one thing views may change directly, like drafts.
    ui.data_mut(|d| d.insert_temp(egui::Id::new("edit-message-text"), text));
    if save {
        row.out.push(Action::Chat(ChatAction::SaveEdit));
    } else if cancel {
        row.out.push(Action::Chat(ChatAction::CancelEdit));
    }
}

/// Attachments, embeds and stickers.
fn media(row: &mut Row<'_>, ui: &mut Ui, message: &Message) {
    let images: Vec<&Attachment> = message
        .attachments
        .iter()
        .filter(|a| !a.url.is_empty() && matches!(a.kind(), MediaKind::Image | MediaKind::Gif))
        .collect();
    if !images.is_empty() {
        ui.add_space(4.0);
        gallery(row, ui, &images);
    }
    for attachment in message
        .attachments
        .iter()
        .filter(|a| a.url.is_empty() || !matches!(a.kind(), MediaKind::Image | MediaKind::Gif))
    {
        ui.add_space(4.0);
        if attachment.url.is_empty() {
            file_card(row, ui, attachment, true);
            continue;
        }
        match attachment.kind() {
            MediaKind::Video => {
                let size = fit(
                    attachment.width,
                    attachment.height,
                    MEDIA_W.min(ui.available_width()),
                    MEDIA_H,
                );
                let poster = format!(
                    "{}{}format=jpeg",
                    attachment.proxy,
                    if attachment.proxy.contains('?') {
                        '&'
                    } else {
                        '?'
                    }
                );
                player::video(
                    row,
                    ui,
                    &attachment.url,
                    &attachment.url,
                    Some(&poster),
                    size,
                    attachment.spoiler,
                );
            }
            MediaKind::Audio if message.voice => player::voice(row, ui, attachment),
            MediaKind::Audio => player::audio(row, ui, attachment),
            _ => file_card(row, ui, attachment, false),
        }
    }
    for embed in &message.embeds {
        ui.add_space(4.0);
        self::embed(row, ui, embed);
    }
    for sticker in &message.stickers {
        ui.add_space(4.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(160.0), Sense::hover());
        match sticker.url().and_then(|u| row.app.picture(Some(&u))) {
            Some(uri) => {
                egui::Image::new(uri).paint_at(ui, rect);
            }
            None => {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(8),
                    crate::theme::current().sidebar,
                );
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &sticker.name,
                    font(Weight::Medium, 14.0),
                    row.app.palette.muted,
                );
            }
        }
    }
}

/// The size a picture is shown at: its own, fitted inside the box.
fn fit(width: Option<u32>, height: Option<u32>, max_w: f32, max_h: f32) -> Vec2 {
    let (w, h) = match (width, height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w as f32, h as f32),
        _ => (max_w, max_h * 0.75),
    };
    let scale = (max_w / w).min(max_h / h).min(1.0);
    vec2((w * scale).max(16.0), (h * scale).max(16.0))
}

/// Discord's media proxy resizes JPEG, PNG and WebP; GIFs keep their
/// original so they animate.
fn sized(proxy: &str, size: Vec2, ppp: f32) -> String {
    let sep = if proxy.contains('?') { '&' } else { '?' };
    format!(
        "{proxy}{sep}width={}&height={}",
        (size.x * ppp) as u32,
        (size.y * ppp) as u32
    )
}

fn gallery(row: &mut Row<'_>, ui: &mut Ui, images: &[&Attachment]) {
    let ppp = ui.ctx().pixels_per_point();
    let max_w = MEDIA_W.min(ui.available_width());
    if images.len() == 1 {
        let a = images[0];
        let size = fit(a.width, a.height, max_w, MEDIA_H);
        let url = if a.kind() == MediaKind::Gif {
            a.url.clone()
        } else {
            sized(&a.proxy, size, ppp)
        };
        picture(row, ui, &url, &a.url, size, a.spoiler);
        return;
    }
    // A grid of squares, two or three across, like Discord's mosaic.
    let columns = if images.len() == 2 || images.len() == 4 {
        2
    } else {
        3
    };
    let gap = 4.0;
    let cell = (max_w - gap * (columns as f32 - 1.0)) / columns as f32;
    for chunk in images.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for a in chunk {
                let size = Vec2::splat(cell);
                let url = if a.kind() == MediaKind::Gif {
                    a.url.clone()
                } else {
                    sized(&a.proxy, size * 2.0, ppp)
                };
                picture(row, ui, &url, &a.url, size, a.spoiler);
            }
        });
        ui.add_space(gap);
    }
}

/// A picture with a placeholder while it loads; click to see it large.
fn picture(row: &mut Row<'_>, ui: &mut Ui, url: &str, full: &str, size: Vec2, spoiler: bool) {
    let app = row.app;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let radius = CornerRadius::same(8);
    let hidden = spoiler && !app.chat.revealed_media.contains(full);
    match app.picture(Some(url)) {
        Some(uri) => {
            let mut image = egui::Image::new(uri).corner_radius(radius);
            if hidden {
                image = image.tint(Color32::from_gray(25));
            }
            image.paint_at(ui, rect);
        }
        None => {
            ui.painter()
                .rect_filled(rect, radius, crate::theme::current().sidebar);
            paint_icon(ui, Icon::Image, rect.center(), 32.0, app.palette.muted);
        }
    }
    if hidden {
        spoiler_pill(ui, rect);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if response.clicked() {
        row.act(if hidden {
            ChatAction::RevealMedia(full.to_owned())
        } else {
            ChatAction::ViewImage(full.to_owned())
        });
    }
}

pub fn spoiler_pill(ui: &Ui, rect: Rect) {
    let pill = Rect::from_center_size(rect.center(), vec2(80.0, 26.0));
    ui.painter()
        .rect_filled(pill, CornerRadius::same(13), Color32::from_black_alpha(200));
    ui.painter().text(
        pill.center(),
        Align2::CENTER_CENTER,
        "crate::theme::current().rail",
        font(Weight::Bold, 12.0),
        Color32::WHITE,
    );
}

pub fn size_text(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.2} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.2} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.2} KB", b as f64 / 1024.0),
        b => format!("{b} bytes"),
    }
}

/// A file Discord cannot preview: its name, size and a download button.
fn file_card(row: &mut Row<'_>, ui: &mut Ui, file: &Attachment, sending: bool) {
    let app = row.app;
    let palette = app.palette;
    let width = 432.0_f32.min(ui.available_width());
    Frame::new()
        .fill(crate::theme::current().sidebar)
        .stroke(Stroke::new(1.0, crate::theme::current().divider))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.horizontal(|ui| {
                let (icon, _) = ui.allocate_exact_size(vec2(30.0, 40.0), Sense::hover());
                paint_icon(ui, Icon::File, icon.center(), 30.0, palette.accent);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 36.0);
                    let name = egui::RichText::new(hyphens(&file.name))
                        .size(16.0)
                        .color(if sending { palette.muted } else { LINK });
                    let response = ui.add(egui::Label::new(name).truncate().sense(Sense::click()));
                    if response.clicked() && !sending {
                        row.out
                            .push(Action::Chat(ChatAction::OpenUrl(file.url.clone())));
                    }
                    let detail = if sending {
                        "Enviando…".to_owned()
                    } else {
                        size_text(file.size)
                    };
                    ui.label(egui::RichText::new(detail).size(12.0).color(palette.muted));
                });
                if !sending {
                    let download = super::widgets::icon_button(
                        ui,
                        &palette,
                        Icon::Download,
                        24.0,
                        false,
                        "Baixar",
                    );
                    if download.clicked() {
                        row.out
                            .push(Action::Chat(ChatAction::OpenUrl(file.url.clone())));
                    }
                }
            });
        });
}

/// A GIF sent as a video: it plays at once, silent and in a loop, as in
/// Discord, and a click shows it large. Without ffmpeg, or if it cannot be
/// decoded, the poster shows.
fn looping(row: &mut Row<'_>, ui: &mut Ui, video: &str, poster: Option<&str>, size: Vec2) {
    let app = row.app;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let radius = CornerRadius::same(8);
    let frame = (crate::media::available() && !app.animation_failed(video))
        .then(|| app.animation(video))
        .flatten();
    match frame {
        Some(frame) => {
            egui::Image::new((frame.id(), rect.size()))
                .corner_radius(radius)
                .paint_at(ui, rect);
        }
        None => match poster.and_then(|p| app.picture(Some(p))) {
            Some(uri) => {
                egui::Image::new(uri)
                    .corner_radius(radius)
                    .paint_at(ui, rect);
            }
            None => {
                ui.painter()
                    .rect_filled(rect, radius, crate::theme::current().sidebar);
            }
        },
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    // Like Discord, a click shows it large; the viewer links to the page.
    if response.clicked() {
        row.act(ChatAction::ViewImage(video.to_owned()));
    }
}

fn embed(row: &mut Row<'_>, ui: &mut Ui, embed: &Embed) {
    let ppp = ui.ctx().pixels_per_point();
    let max_w = MEDIA_W.min(ui.available_width());
    match embed.kind.as_str() {
        "image" => {
            if let Some(media) = embed.thumbnail.as_ref().or(embed.image.as_ref()) {
                let size = fit(media.width, media.height, max_w, MEDIA_H);
                let gif = media.url.to_lowercase().contains(".gif");
                let url = media
                    .proxy
                    .clone()
                    .filter(|_| !gif)
                    .map_or(media.url.clone(), |p| sized(&p, size, ppp));
                picture(row, ui, &url, &media.url, size, false);
            }
            return;
        }
        "gifv" => {
            let video = embed.video.as_ref();
            let size = fit(
                video.and_then(|v| v.width),
                video.and_then(|v| v.height),
                max_w,
                MEDIA_H,
            );
            if let Some(video) = video {
                let poster = embed
                    .thumbnail
                    .as_ref()
                    .map(|t| t.proxy.clone().unwrap_or(t.url.clone()));
                looping(row, ui, &video.url, poster.as_deref(), size);
            }
            return;
        }
        _ => {}
    }
    let bar = embed.color.unwrap_or(if embed.is_youtube() {
        Color32::from_rgb(0xff, 0x00, 0x00)
    } else {
        crate::theme::current().divider
    });
    let has_large = embed.image.is_some() || embed.video.is_some();
    let width = if has_large { 432.0_f32 } else { 520.0 }.min(ui.available_width());
    let start = ui.cursor().min;
    let response = Frame::new()
        .fill(crate::theme::current().sidebar)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin {
            left: 16,
            right: 16,
            top: 8,
            bottom: 16,
        })
        .show(ui, |ui| {
            ui.set_max_width(width - 32.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            let thumb = embed
                .thumbnail
                .as_ref()
                .filter(|_| !has_large && embed.kind != "video");
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width(width - 32.0 - if thumb.is_some() { 96.0 } else { 0.0 });
                    embed_text(row, ui, embed);
                });
                if let Some(thumb) = thumb {
                    ui.add_space(16.0);
                    let size = fit(thumb.width, thumb.height, 80.0, 80.0);
                    let url = thumb
                        .proxy
                        .clone()
                        .map_or(thumb.url.clone(), |p| sized(&p, size, ppp));
                    picture(row, ui, &url, &thumb.url, size, false);
                }
            });
            embed_media(row, ui, embed, width - 32.0);
            footer(row, ui, embed);
        });
    let rect = response.response.rect;
    let stripe = Rect::from_min_max(start, pos2(start.x + 4.0, rect.bottom()));
    ui.painter().rect_filled(
        stripe,
        CornerRadius {
            nw: 4,
            sw: 4,
            ne: 0,
            se: 0,
        },
        bar,
    );
}

fn embed_text(row: &mut Row<'_>, ui: &mut Ui, embed: &Embed) {
    let app = row.app;
    let palette = app.palette;
    ui.add_space(4.0);
    if let Some(provider) = &embed.provider {
        ui.label(
            egui::RichText::new(hyphens(provider))
                .size(12.0)
                .color(palette.muted),
        );
    }
    if let Some(author) = &embed.author {
        ui.horizontal(|ui| {
            if let Some(uri) = app.picture(embed.author_icon.as_deref()) {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                egui::Image::new(uri)
                    .corner_radius(CornerRadius::same(12))
                    .paint_at(ui, rect);
                ui.add_space(8.0);
            }
            let text = egui::RichText::new(hyphens(author))
                .size(14.0)
                .strong()
                .color(palette.strong);
            let response = ui.add(egui::Label::new(text).sense(Sense::click()));
            if let Some(url) = &embed.author_url
                && response.clicked()
            {
                row.act(ChatAction::OpenUrl(url.clone()));
            }
        });
    }
    if let Some(title) = &embed.title {
        let color = if embed.url.is_some() {
            LINK
        } else {
            palette.strong
        };
        let parsed = markdown::parse(title);
        let style = TextStyle {
            size: 16.0,
            color,
            muted: palette.muted,
        };
        let start = ui.cursor().top();
        for block in &parsed {
            if let Block::Paragraph(spans) = block {
                text(row, ui, 0, spans, 16.0, &style, Weight::SemiBold, false);
            }
        }
        let rect = Rect::from_min_max(
            pos2(ui.min_rect().left(), start),
            pos2(ui.min_rect().right(), ui.cursor().top()),
        );
        if let Some(url) = &embed.url {
            let response = ui.interact(rect, ui.id().with(("embed-title", url)), Sense::click());
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.clicked() {
                row.act(ChatAction::OpenUrl(url.clone()));
            }
        }
    }
    if let Some(description) = &embed.description {
        let parsed = markdown::parse(description);
        blocks(row, ui, 0, &parsed, &TextStyle::embed(app), false);
    }
    if !embed.fields.is_empty() {
        let mut i = 0;
        while i < embed.fields.len() {
            // Inline fields sit up to three in a row.
            let mut count = 1;
            if embed.fields[i].inline {
                while count < 3 && i + count < embed.fields.len() && embed.fields[i + count].inline
                {
                    count += 1;
                }
            }
            let column = ui.available_width() / count as f32;
            ui.horizontal_top(|ui| {
                for field in &embed.fields[i..i + count] {
                    ui.vertical(|ui| {
                        ui.set_width(column - 8.0);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let name = markdown::parse(&field.name);
                        let strong = TextStyle {
                            size: 14.0,
                            color: palette.strong,
                            muted: palette.muted,
                        };
                        for block in &name {
                            if let Block::Paragraph(spans) = block {
                                text(row, ui, 0, spans, 14.0, &strong, Weight::SemiBold, false);
                            }
                        }
                        blocks(
                            row,
                            ui,
                            0,
                            &markdown::parse(&field.value),
                            &TextStyle::embed(app),
                            false,
                        );
                    });
                    ui.add_space(8.0);
                }
            });
            i += count;
        }
    }
}

fn embed_media(row: &mut Row<'_>, ui: &mut Ui, embed: &Embed, width: f32) {
    let ppp = ui.ctx().pixels_per_point();
    if embed.kind == "video" || embed.is_youtube() {
        let media: Option<&EmbedMedia> = embed.thumbnail.as_ref().or(embed.video.as_ref());
        let size = fit(
            media.and_then(|m| m.width),
            media.and_then(|m| m.height),
            width,
            MEDIA_H,
        );
        let poster = embed.thumbnail.as_ref().map(|t| {
            t.proxy
                .clone()
                .map_or(t.url.clone(), |p| sized(&p, size, ppp))
        });
        let page = embed.url.clone().unwrap_or_default();
        let (key, source) = if embed.is_youtube() {
            (page.clone(), format!("ytdl:{page}"))
        } else {
            match &embed.video {
                Some(v) if !v.url.contains("youtube.com/embed") => (v.url.clone(), v.url.clone()),
                _ => (page.clone(), format!("ytdl:{page}")),
            }
        };
        player::video(row, ui, &key, &source, poster.as_deref(), size, false);
        return;
    }
    if let Some(image) = &embed.image {
        let size = fit(image.width, image.height, width, MEDIA_H);
        let gif = image.url.to_lowercase().contains(".gif");
        let url = image
            .proxy
            .clone()
            .filter(|_| !gif)
            .map_or(image.url.clone(), |p| sized(&p, size, ppp));
        picture(row, ui, &url, &image.url, size, false);
    }
}

fn footer(row: &mut Row<'_>, ui: &mut Ui, embed: &Embed) {
    let app = row.app;
    if embed.footer.is_none() && embed.timestamp.is_none() {
        return;
    }
    ui.horizontal(|ui| {
        if let Some(uri) = app.picture(embed.footer_icon.as_deref()) {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
            egui::Image::new(uri)
                .corner_radius(CornerRadius::same(10))
                .paint_at(ui, rect);
            ui.add_space(8.0);
        }
        let mut parts = Vec::new();
        if let Some(text) = &embed.footer {
            parts.push(hyphens(text).into_owned());
        }
        if let Some(time) = embed.timestamp {
            parts.push(timestamp(time));
        }
        ui.label(
            egui::RichText::new(parts.join(" • "))
                .size(12.0)
                .color(app.palette.muted),
        );
    });
}

fn reactions(row: &mut Row<'_>, ui: &mut Ui, message: &Message) {
    let app = row.app;
    let palette = app.palette;
    let (channel, id) = (row.channel, message.id);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for reaction in &message.reactions {
            let count = reaction.count.to_string();
            let count_galley = ui.painter().layout_no_wrap(
                count,
                font(Weight::Medium, 14.0),
                if reaction.me {
                    mention_text()
                } else {
                    palette.text
                },
            );
            let width = 6.0 + 16.0 + 6.0 + count_galley.size().x + 8.0;
            let (rect, response) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
            let fill = if reaction.me {
                Color32::from_rgba_unmultiplied(0x58, 0x65, 0xf2, 0x26)
            } else {
                crate::theme::current().sidebar
            };
            ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
            let border = if reaction.me {
                palette.accent
            } else if response.hovered() {
                palette.selected
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(8),
                Stroke::new(1.0, border),
                StrokeKind::Inside,
            );
            let emoji_rect = Rect::from_center_size(
                pos2(rect.left() + 14.0, rect.center().y),
                Vec2::splat(16.0),
            );
            match &reaction.emoji {
                ReactionEmoji::Unicode(text) => {
                    ui.painter().text(
                        emoji_rect.center(),
                        Align2::CENTER_CENTER,
                        text,
                        font(Weight::Regular, 14.0),
                        palette.text,
                    );
                }
                ReactionEmoji::Custom { id, animated, .. } => {
                    if let Some(uri) = app.picture(Some(&CustomEmoji::url(*id, *animated))) {
                        egui::Image::new(uri).paint_at(ui, emoji_rect);
                    }
                }
            }
            let color = if reaction.me {
                mention_text()
            } else {
                palette.text
            };
            ui.painter().galley(
                pos2(
                    rect.left() + 28.0,
                    rect.center().y - count_galley.size().y / 2.0,
                ),
                count_galley,
                color,
            );
            if response.clicked() {
                row.act(ChatAction::React {
                    channel,
                    message: id,
                    emoji: reaction.emoji.clone(),
                    add: !reaction.me,
                });
            }
        }
        let (rect, response) = ui.allocate_exact_size(vec2(32.0, 26.0), Sense::click());
        if response.hovered() || ui.rect_contains_pointer(ui.min_rect()) {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), crate::theme::current().sidebar);
            paint_icon(ui, Icon::ReactAdd, rect.center(), 16.0, palette.icon);
        }
        if response.clicked() {
            row.act(ChatAction::OpenPicker {
                tab: PickerTab::Emoji,
                target: PickerTarget::Reaction {
                    channel,
                    message: id,
                },
            });
        }
    });
}

/// The bar at the top right of a hovered message.
fn toolbar(row: &mut Row<'_>, ui: &mut Ui, message: &Message, rect: Rect) {
    let app = row.app;
    let palette = app.palette;
    let own = message.author == app.model.me;
    let shift = ui.input(|i| i.modifiers.shift);
    let mut buttons: Vec<(Icon, &str, ChatAction)> = vec![(
        Icon::ReactAdd,
        "Adicionar reação",
        ChatAction::OpenPicker {
            tab: PickerTab::Emoji,
            target: PickerTarget::Reaction {
                channel: row.channel,
                message: message.id,
            },
        },
    )];
    if own {
        buttons.push((
            Icon::Pencil,
            "Editar",
            ChatAction::StartEdit {
                channel: row.channel,
                message: message.id,
            },
        ));
    }
    buttons.push((
        Icon::Reply,
        "Responder",
        ChatAction::Reply {
            channel: row.channel,
            message: message.id,
        },
    ));
    if own && shift {
        buttons.push((
            Icon::Trash,
            "Excluir",
            ChatAction::Delete {
                channel: row.channel,
                message: message.id,
            },
        ));
    } else if !message.text.is_empty() {
        buttons.push((
            Icon::Copy,
            "Copiar texto",
            ChatAction::Copy(message.text.clone()),
        ));
    }
    let size = 32.0;
    let bar = Rect::from_min_size(
        pos2(
            rect.right() - 16.0 - size * buttons.len() as f32 - 4.0,
            rect.top() - 16.0,
        ),
        vec2(size * buttons.len() as f32 + 4.0, size + 4.0),
    );
    let bar = if bar.top() < ui.clip_rect().top() {
        bar.translate(vec2(0.0, ui.clip_rect().top() - bar.top()))
    } else {
        bar
    };
    ui.painter()
        .rect_filled(bar, CornerRadius::same(6), palette.chat);
    ui.painter().rect_stroke(
        bar,
        CornerRadius::same(6),
        Stroke::new(1.0, palette.divider),
        StrokeKind::Inside,
    );
    let mut x = bar.left() + 2.0;
    for (icon, hint, action) in buttons {
        let cell = Rect::from_min_size(pos2(x, bar.top() + 2.0), Vec2::splat(size));
        let response = ui.interact(
            cell,
            ui.id().with(("toolbar", message.id, hint)),
            Sense::click(),
        );
        if response.hovered() {
            ui.painter()
                .rect_filled(cell, CornerRadius::same(4), palette.hover);
        }
        let tint = if icon == Icon::Trash {
            palette.danger
        } else if response.hovered() {
            palette.strong
        } else {
            palette.icon
        };
        paint_icon(ui, icon, cell.center(), 20.0, tint);
        if response.on_hover_text(hint).clicked() {
            row.act(action);
        }
        x += size;
    }
}

/// A mention's tint: the accent, see-through.
fn mention_bg() -> Color32 {
    let a = crate::theme::current().accent;
    Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 0x4d)
}

/// A mention's text: the accent, lightened toward the text colour.
fn mention_text() -> Color32 {
    let p = crate::theme::current();
    crate::theme::mix(p.accent, p.strong, 0.6)
}
