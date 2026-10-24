//! The message field and what surrounds it: the reply bar, files waiting
//! to be sent, the character counter, "is typing", the emoji, GIF and
//! sticker picker, the limit dialogs, the picture viewer and dropping files.

use egui::{
    Align2, Color32, CornerRadius, Frame, Key, Margin, Modifiers, Rect, RichText, Sense, Shadow,
    Stroke, Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::message::size_text;
use super::widgets::{Elide, font, hyphens, icon_button, paint_icon};
use crate::app::{Action, App};
use crate::chat::{ChatAction, Dialog, PickerTab, PickerTarget};
use crate::icons::Icon;
use crate::model::{ChannelKind, CustomEmoji, Id, ReactionEmoji};

const LINK: Color32 = Color32::from_rgb(0x00, 0xa8, 0xfc);
pub const COMPOSER_ID: &str = "composer";

pub fn composer_id(channel: Id) -> egui::Id {
    egui::Id::new((COMPOSER_ID, channel))
}

fn act(app: &mut App, action: ChatAction) {
    app.actions.push(Action::Chat(action));
}

/// The bottom of the conversation. Returns the field's frame, for the
/// picker to sit above.
pub fn show(app: &mut App, ui: &mut Ui, channel: Id) -> Rect {
    let palette = app.palette;
    let Some(c) = app.model.channel(channel) else {
        return Rect::NOTHING;
    };
    let placeholder = match c.kind {
        ChannelKind::Direct | ChannelKind::Group => {
            format!("Conversar com @{}", app.model.direct_name(c))
        }
        _ => format!("Conversar em #{}", c.name),
    };
    let mut field = Rect::NOTHING;
    egui::Panel::bottom("composer")
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin {
            left: 16,
            right: 16,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
            let replying = app.chat.reply.get(&channel).copied();
            if let Some(message) = replying {
                reply_bar(app, ui, channel, message);
            }
            let radius = if replying.is_some() {
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: 8,
                    se: 8,
                }
            } else {
                CornerRadius::same(8)
            };
            let frame = Frame::new()
                .fill(palette.composer)
                .corner_radius(radius)
                .inner_margin(Margin::symmetric(16, 0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if app
                        .chat
                        .uploads
                        .get(&channel)
                        .is_some_and(|l| !l.is_empty())
                    {
                        uploads(app, ui, channel);
                    }
                    input(app, ui, channel, &placeholder);
                });
            field = frame.response.rect;
            counter(app, ui, channel, field);
            typing(app, ui, channel);
        });
    field
}

fn reply_bar(app: &mut App, ui: &mut Ui, channel: Id, message: Id) {
    let palette = app.palette;
    let author = app
        .model
        .channel(channel)
        .and_then(|c| c.messages.iter().find(|m| m.id == message))
        .map(|m| m.author);
    let guild = app.server();
    let name = author.map_or_else(|| "mensagem".into(), |a| app.model.name_in(guild, a));
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
    ui.painter().rect_filled(
        rect,
        CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        },
        palette.sidebar,
    );
    let mut job = egui::text::LayoutJob::default();
    let normal = egui::TextFormat {
        font_id: font(Weight::Regular, 14.0),
        color: palette.muted,
        ..Default::default()
    };
    job.append("Respondendo a ", 0.0, normal.clone());
    job.append(
        &name,
        0.0,
        egui::TextFormat {
            font_id: font(Weight::SemiBold, 14.0),
            color: palette.text,
            ..normal
        },
    );
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        pos2(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0),
        galley,
        palette.text,
    );
    let close = Rect::from_center_size(
        pos2(rect.right() - 20.0, rect.center().y),
        Vec2::splat(24.0),
    );
    let response = ui.interact(close, ui.id().with("cancel-reply"), Sense::click());
    paint_icon(
        ui,
        Icon::Close,
        close.center(),
        16.0,
        if response.hovered() {
            palette.strong
        } else {
            palette.icon
        },
    );
    if response.clicked() {
        act(app, ChatAction::CancelReply);
    }
}

/// Files waiting to go, as tiles with a preview, spoiler and remove.
fn uploads(app: &mut App, ui: &mut Ui, channel: Id) {
    let palette = app.palette;
    let files = app.chat.uploads.get(&channel).cloned().unwrap_or_default();
    ui.add_space(16.0);
    egui::ScrollArea::horizontal()
        .id_salt("uploads")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for (index, file) in files.iter().enumerate() {
                    let (tile, _) = ui.allocate_exact_size(vec2(176.0, 176.0), Sense::hover());
                    ui.painter()
                        .rect_filled(tile, CornerRadius::same(4), palette.sidebar);
                    let preview =
                        Rect::from_min_size(tile.min + vec2(8.0, 8.0), vec2(160.0, 124.0));
                    if file.is_image() {
                        let uri = format!("file://{}", file.path.display());
                        let mut image = egui::Image::new(uri)
                            .fit_to_exact_size(preview.size())
                            .corner_radius(CornerRadius::same(3));
                        if file.spoiler {
                            image = image.tint(Color32::from_gray(30));
                        }
                        image.paint_at(ui, preview);
                    } else {
                        ui.painter()
                            .rect_filled(preview, CornerRadius::same(3), palette.rail);
                        paint_icon(ui, Icon::File, preview.center(), 48.0, palette.accent);
                    }
                    if file.spoiler {
                        super::message::spoiler_pill(ui, preview);
                    }
                    Elide::new(ui, tile.right() - 8.0).text(
                        pos2(tile.left() + 8.0, tile.bottom() - 30.0),
                        Align2::LEFT_CENTER,
                        hyphens(&file.name),
                        font(Weight::Regular, 13.0),
                        palette.text,
                    );
                    ui.painter().text(
                        pos2(tile.left() + 8.0, tile.bottom() - 13.0),
                        Align2::LEFT_CENTER,
                        size_text(file.size),
                        font(Weight::Regular, 11.0),
                        palette.muted,
                    );
                    // The tools at the top right, as in Discord.
                    let tools = Rect::from_min_size(
                        pos2(tile.right() - 64.0, tile.top() - 8.0),
                        vec2(72.0, 32.0),
                    );
                    ui.painter()
                        .rect_filled(tools, CornerRadius::same(4), palette.chat);
                    ui.painter().rect_stroke(
                        tools,
                        CornerRadius::same(4),
                        Stroke::new(1.0, palette.divider),
                        egui::StrokeKind::Inside,
                    );
                    let eye = Rect::from_min_size(tools.min, vec2(36.0, 32.0));
                    let trash = Rect::from_min_size(tools.min + vec2(36.0, 0.0), vec2(36.0, 32.0));
                    let eye_response =
                        ui.interact(eye, ui.id().with(("spoiler", index)), Sense::click());
                    paint_icon(
                        ui,
                        if file.spoiler {
                            Icon::EyeOff
                        } else {
                            Icon::Eye
                        },
                        eye.center(),
                        18.0,
                        if eye_response.hovered() {
                            palette.strong
                        } else {
                            palette.icon
                        },
                    );
                    if eye_response.on_hover_text("Marcar como spoiler").clicked() {
                        act(app, ChatAction::ToggleSpoiler(index));
                    }
                    let trash_response =
                        ui.interact(trash, ui.id().with(("remove", index)), Sense::click());
                    paint_icon(ui, Icon::Trash, trash.center(), 18.0, palette.danger);
                    if trash_response.on_hover_text("Remover anexo").clicked() {
                        act(app, ChatAction::RemoveUpload(index));
                    }
                }
            });
        });
    ui.add_space(8.0);
    let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        line.x_range(),
        line.center().y,
        Stroke::new(1.0, palette.divider),
    );
}

fn input(app: &mut App, ui: &mut Ui, channel: Id, placeholder: &str) {
    let palette = app.palette;
    let id = composer_id(channel);
    let focused = ui.memory(|m| m.has_focus(id));
    let mut send = false;
    if focused {
        send = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if app.chat.picker.is_some() {
                act(app, ChatAction::ClosePicker);
            } else {
                act(app, ChatAction::CancelReply);
            }
        }
        let empty = app.drafts.get(&channel).is_none_or(|d| d.is_empty());
        if empty && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
            act(app, ChatAction::EditLast);
        }
    }
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
        ui.add_space(0.0);
        ui.vertical(|ui| {
            ui.add_space(10.0);
            if icon_button(
                ui,
                &palette,
                Icon::PlusCircle,
                24.0,
                false,
                "Enviar um arquivo",
            )
            .clicked()
            {
                act(app, ChatAction::AttachFiles);
            }
        });
        let buttons = 4.0 * 24.0 + 3.0 * 16.0;
        let width = ui.available_width() - buttons - 16.0;
        let focus = std::mem::take(&mut app.chat.focus_composer)
            && app.chat.editing.is_none()
            && app.chat.dialog.is_none();
        let to_end = std::mem::take(&mut app.chat.cursor_to_end);
        let draft = app.drafts.entry(channel).or_default();
        let changed = ui
            .vertical(|ui| {
                ui.add_space(11.0);
                let response = egui::ScrollArea::vertical()
                    .id_salt(("composer-scroll", channel))
                    .max_height(ui.ctx().content_rect().height() * 0.4)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(draft)
                                .id(id)
                                .hint_text(RichText::new(placeholder).color(palette.muted))
                                .font(font(Weight::Regular, 16.0))
                                .text_color(palette.text)
                                .frame(Frame::NONE)
                                .desired_rows(1)
                                .desired_width(width),
                        )
                    })
                    .inner;
                ui.add_space(11.0);
                if focus {
                    response.request_focus();
                }
                if to_end && let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                    let end = egui::text::CCursor::new(draft.chars().count());
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(end)));
                    state.store(ui.ctx(), id);
                }
                response.changed()
            })
            .inner;
        if changed {
            act(app, ChatAction::Typed);
        }
        ui.vertical(|ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                if icon_button(
                    ui,
                    &palette,
                    Icon::Gift,
                    24.0,
                    false,
                    "Presentear com Nitro",
                )
                .clicked()
                {
                    act(app, ChatAction::OpenUrl("https://discord.com/nitro".into()));
                }
                for (icon, tab, hint) in [
                    (Icon::Gif, PickerTab::Gif, "Abrir seletor de GIFs"),
                    (
                        Icon::Sticker,
                        PickerTab::Sticker,
                        "Abrir seletor de figurinhas",
                    ),
                    (Icon::Smile, PickerTab::Emoji, "Selecionar emoji"),
                ] {
                    let open = app
                        .chat
                        .picker
                        .as_ref()
                        .is_some_and(|p| p.tab == tab && p.target == PickerTarget::Composer);
                    if icon_button(ui, &palette, icon, 24.0, open, hint).clicked() {
                        act(
                            app,
                            ChatAction::OpenPicker {
                                tab,
                                target: PickerTarget::Composer,
                            },
                        );
                    }
                }
            });
        });
    });
    if send {
        app.actions.push(Action::Send);
    }
}

/// Discord's counter: it appears near the limit and turns red past it.
fn counter(app: &App, ui: &Ui, channel: Id, field: Rect) {
    let length = app
        .drafts
        .get(&channel)
        .map_or(0, |d| d.trim().chars().count());
    let max = app.model.max_message();
    if length + 200 < max {
        return;
    }
    let left = max as i64 - length as i64;
    let color = if left < 0 {
        app.palette.danger
    } else {
        app.palette.muted
    };
    ui.painter().text(
        field.right_bottom() - vec2(12.0, 6.0),
        Align2::RIGHT_BOTTOM,
        left.to_string(),
        font(Weight::Medium, 12.0),
        color,
    );
}

fn typing(app: &App, ui: &mut Ui, channel: Id) {
    let palette = app.palette;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let who = app.chat.typing_in(channel);
    if who.is_empty() {
        return;
    }
    let guild = app.server();
    let names: Vec<String> = who.iter().map(|u| app.model.name_in(guild, *u)).collect();
    let mut job = egui::text::LayoutJob::default();
    let bold = egui::TextFormat {
        font_id: font(Weight::Bold, 13.0),
        color: palette.text,
        ..Default::default()
    };
    let normal = egui::TextFormat {
        font_id: font(Weight::Regular, 13.0),
        color: palette.text,
        ..Default::default()
    };
    match names.as_slice() {
        [one] => {
            job.append(one, 0.0, bold);
            job.append(" está digitando…", 0.0, normal);
        }
        [a, b] => {
            job.append(a, 0.0, bold.clone());
            job.append(" e ", 0.0, normal.clone());
            job.append(b, 0.0, bold);
            job.append(" estão digitando…", 0.0, normal);
        }
        [a, b, c] => {
            job.append(&format!("{a}, {b}"), 0.0, bold.clone());
            job.append(" e ", 0.0, normal.clone());
            job.append(c, 0.0, bold);
            job.append(" estão digitando…", 0.0, normal);
        }
        _ => job.append("Várias pessoas estão digitando…", 0.0, normal),
    }
    let time = ui.input(|i| i.time) as f32;
    for i in 0..3 {
        let phase = ((time * 5.0 - i as f32 * 0.7).sin() + 1.0) / 2.0;
        let center = pos2(rect.left() + 8.0 + i as f32 * 7.0, rect.center().y);
        ui.painter()
            .circle_filled(center, 2.5, palette.text.gamma_multiply(0.4 + 0.6 * phase));
    }
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        pos2(rect.left() + 32.0, rect.center().y - galley.size().y / 2.0),
        galley,
        palette.text,
    );
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));
}

/// The expression picker above the field: GIFs, stickers and emoji.
pub fn picker(app: &mut App, ui: &mut Ui, field: Rect) {
    let Some(picker) = &app.chat.picker else {
        return;
    };
    let palette = app.palette;
    let tab = picker.tab;
    let reaction = matches!(picker.target, PickerTarget::Reaction { .. });
    let size = vec2(440.0, 460.0);
    let content = ui.ctx().content_rect();
    let anchor = if field.is_positive() {
        field.right_top()
    } else {
        content.right_bottom() - vec2(16.0, 100.0)
    };
    let pos = pos2(
        (anchor.x - size.x).max(content.left() + 8.0),
        (anchor.y - size.y - 8.0).max(content.top() + 8.0),
    );
    let area = egui::Area::new(egui::Id::new("expression-picker"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius::same(8))
                .shadow(Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: Color32::from_black_alpha(90),
                })
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(size.x - 32.0);
                    ui.set_height(size.y - 32.0);
                    ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
                    if !reaction {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for (name, t) in [
                                ("GIFs", PickerTab::Gif),
                                ("Figurinhas", PickerTab::Sticker),
                                ("Emoji", PickerTab::Emoji),
                            ] {
                                let selected = t == tab;
                                let text = RichText::new(name)
                                    .font(font(Weight::Medium, 14.0))
                                    .color(if selected {
                                        palette.strong
                                    } else {
                                        palette.muted
                                    });
                                let button = egui::Button::new(text)
                                    .fill(if selected {
                                        palette.selected
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .corner_radius(CornerRadius::same(4));
                                if ui.add(button).clicked() && !selected {
                                    app.actions.push(Action::Chat(ChatAction::PickerTab(t)));
                                }
                            }
                        });
                    }
                    let query = app.chat.picker.as_mut().map(|p| &mut p.query);
                    if let Some(query) = query {
                        let hint = match tab {
                            PickerTab::Gif => "Pesquisar no Tenor",
                            PickerTab::Sticker => "Encontrar figurinhas",
                            PickerTab::Emoji => "Encontre o emoji perfeito",
                        };
                        let response = Frame::new()
                            .fill(palette.rail)
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(query)
                                        .id(egui::Id::new("picker-search"))
                                        .hint_text(RichText::new(hint).color(palette.muted))
                                        .frame(Frame::NONE)
                                        .font(font(Weight::Regular, 15.0))
                                        .desired_width(f32::INFINITY),
                                )
                            });
                        if ui.memory(|m| m.focused().is_none()) {
                            response.inner.request_focus();
                        }
                        if response.inner.changed() {
                            ui.data_mut(|d| {
                                d.insert_temp(egui::Id::new("picker-edited"), ui.input(|i| i.time))
                            });
                        }
                    }
                    match tab {
                        PickerTab::Emoji => emoji_tab(app, ui),
                        PickerTab::Gif => gif_tab(app, ui),
                        PickerTab::Sticker => sticker_tab(app, ui),
                    }
                });
        });
    let escape = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    if area.response.clicked_elsewhere() || escape {
        app.actions.push(Action::Chat(ChatAction::ClosePicker));
    }
}

enum EmojiItem {
    Unicode(&'static emojis::Emoji),
    Custom(CustomEmoji),
}

enum PickerRow {
    Header(String),
    Emojis(Vec<EmojiItem>),
}

const COLUMNS: usize = 9;
const CELL: f32 = 44.0;

fn group_name(group: emojis::Group) -> &'static str {
    use emojis::Group::*;
    match group {
        SmileysAndEmotion => "Carinhas e emoções",
        PeopleAndBody => "Pessoas",
        AnimalsAndNature => "Natureza",
        FoodAndDrink => "Comidas e bebidas",
        TravelAndPlaces => "Viagens e lugares",
        Activities => "Atividades",
        Objects => "Objetos",
        Symbols => "Símbolos",
        Flags => "Bandeiras",
    }
}

fn emoji_tab(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let query = app
        .chat
        .picker
        .as_ref()
        .map(|p| p.query.trim().to_lowercase())
        .unwrap_or_default();
    let matches = |name: &str| query.is_empty() || name.to_lowercase().contains(&query);
    let mut rows: Vec<PickerRow> = Vec::new();
    let push_section = |rows: &mut Vec<PickerRow>, title: String, items: Vec<EmojiItem>| {
        if items.is_empty() {
            return;
        }
        rows.push(PickerRow::Header(title));
        let mut items = items.into_iter().peekable();
        while items.peek().is_some() {
            rows.push(PickerRow::Emojis(items.by_ref().take(COLUMNS).collect()));
        }
    };
    // Server emoji: this server's, or every server's with Nitro.
    let here = app.server();
    for guild in &app.model.guilds {
        if app.model.premium == 0 && Some(guild.id) != here {
            continue;
        }
        let items = guild
            .emojis
            .iter()
            .filter(|e| matches(&e.name))
            .cloned()
            .map(EmojiItem::Custom)
            .collect();
        push_section(&mut rows, guild.name.clone(), items);
    }
    for group in emojis::Group::iter() {
        let items = group
            .emojis()
            .filter(|e| matches(e.name()) || e.shortcodes().any(|s| matches(s)))
            .map(EmojiItem::Unicode)
            .collect();
        push_section(&mut rows, group_name(group).to_owned(), items);
    }
    let mut hovered: Option<String> = None;
    let mut chosen: Option<ChatAction> = None;
    let height = ui.available_height() - 40.0;
    egui::ScrollArea::vertical()
        .id_salt("emoji-grid")
        .max_height(height)
        .auto_shrink([false, false])
        .show_rows(ui, CELL, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for row in &rows[range] {
                match row {
                    PickerRow::Header(title) => {
                        let (rect, _) = ui
                            .allocate_exact_size(vec2(ui.available_width(), CELL), Sense::hover());
                        ui.painter().text(
                            pos2(rect.left(), rect.bottom() - 10.0),
                            Align2::LEFT_BOTTOM,
                            title.to_uppercase(),
                            font(Weight::SemiBold, 12.0),
                            palette.muted,
                        );
                    }
                    PickerRow::Emojis(items) => {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            for item in items {
                                let (rect, response) =
                                    ui.allocate_exact_size(Vec2::splat(CELL), Sense::click());
                                if response.hovered() {
                                    ui.painter().rect_filled(
                                        rect.shrink(2.0),
                                        CornerRadius::same(4),
                                        palette.hover,
                                    );
                                }
                                match item {
                                    EmojiItem::Unicode(e) => {
                                        ui.painter().text(
                                            rect.center(),
                                            Align2::CENTER_CENTER,
                                            e.as_str(),
                                            font(Weight::Regular, 28.0),
                                            Color32::WHITE,
                                        );
                                        if response.hovered() {
                                            hovered = Some(format!(
                                                ":{}:",
                                                e.shortcode().unwrap_or(e.name())
                                            ));
                                        }
                                        if response.clicked() {
                                            chosen = Some(ChatAction::PickEmoji {
                                                text: e.as_str().to_owned(),
                                                reaction: ReactionEmoji::Unicode(
                                                    e.as_str().to_owned(),
                                                ),
                                            });
                                        }
                                    }
                                    EmojiItem::Custom(e) => {
                                        if let Some(uri) =
                                            app.picture(Some(&CustomEmoji::url(e.id, e.animated)))
                                        {
                                            egui::Image::new(uri).paint_at(
                                                ui,
                                                Rect::from_center_size(
                                                    rect.center(),
                                                    Vec2::splat(32.0),
                                                ),
                                            );
                                        }
                                        if response.hovered() {
                                            hovered = Some(format!(":{}:", e.name));
                                        }
                                        if response.clicked() {
                                            chosen = Some(ChatAction::PickEmoji {
                                                text: format!(":{}:", e.name),
                                                reaction: ReactionEmoji::Custom {
                                                    id: e.id,
                                                    name: e.name.clone(),
                                                    animated: e.animated,
                                                },
                                            });
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
            }
        });
    let (bottom, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter()
        .rect_filled(bottom, CornerRadius::same(4), palette.rail);
    let label = hovered.unwrap_or_else(|| "Passe o mouse sobre um emoji".into());
    ui.painter().text(
        pos2(bottom.left() + 10.0, bottom.center().y),
        Align2::LEFT_CENTER,
        label,
        font(Weight::SemiBold, 14.0),
        palette.text,
    );
    if let Some(action) = chosen {
        app.actions.push(Action::Chat(action));
    }
}

fn gif_tab(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    // Search a moment after the typing stops.
    let query = app
        .chat
        .picker
        .as_ref()
        .map(|p| p.query.trim().to_owned())
        .unwrap_or_default();
    let searched = app.chat.picker.as_ref().and_then(|p| p.searched.clone());
    let edited: f64 = ui
        .data(|d| d.get_temp(egui::Id::new("picker-edited")))
        .unwrap_or(0.0);
    let now = ui.input(|i| i.time);
    if searched.as_deref() != Some(query.as_str()) {
        if now - edited > 0.4 {
            app.actions
                .push(Action::Chat(ChatAction::SearchGifs(query.clone())));
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
    if app.chat.gifs.is_empty() {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::hover());
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Carregando GIFs…",
            font(Weight::Medium, 14.0),
            palette.muted,
        );
        return;
    }
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .id_salt("gif-grid")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let width = ui.available_width();
            let gap = 8.0;
            let column = (width - gap) / 2.0;
            let mut heights = [0.0f32; 2];
            let origin = ui.cursor().min;
            let mut rects = Vec::new();
            for gif in &app.chat.gifs {
                let col = if heights[0] <= heights[1] { 0 } else { 1 };
                let h = column * gif.height.max(1) as f32 / gif.width.max(1) as f32;
                let rect = Rect::from_min_size(
                    origin + vec2(col as f32 * (column + gap), heights[col]),
                    vec2(column, h),
                );
                heights[col] += h + gap;
                rects.push(rect);
            }
            let total = heights[0].max(heights[1]);
            let (area, _) = ui.allocate_exact_size(vec2(width, total), Sense::hover());
            let _ = area;
            for (gif, rect) in app.chat.gifs.iter().zip(rects) {
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let response = ui.interact(rect, ui.id().with(("gif", &gif.url)), Sense::click());
                match app.picture(Some(&gif.src)) {
                    Some(uri) => {
                        egui::Image::new(uri)
                            .corner_radius(CornerRadius::same(4))
                            .paint_at(ui, rect);
                    }
                    None => {
                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(4), palette.rail);
                    }
                }
                if response.hovered() {
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(4),
                        Stroke::new(2.0, palette.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    chosen = Some(gif.url.clone());
                }
            }
        });
    if let Some(url) = chosen {
        app.actions.push(Action::Chat(ChatAction::PickGif(url)));
    }
}

fn sticker_tab(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let query = app
        .chat
        .picker
        .as_ref()
        .map(|p| p.query.trim().to_lowercase())
        .unwrap_or_default();
    let here = app.server();
    let mut chosen = None;
    let mut any = false;
    egui::ScrollArea::vertical()
        .id_salt("sticker-grid")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for guild in &app.model.guilds {
                if app.model.premium == 0 && Some(guild.id) != here {
                    continue;
                }
                let list: Vec<_> = guild
                    .stickers
                    .iter()
                    .filter(|s| query.is_empty() || s.name.to_lowercase().contains(&query))
                    .collect();
                if list.is_empty() {
                    continue;
                }
                any = true;
                ui.label(
                    RichText::new(guild.name.to_uppercase())
                        .font(font(Weight::SemiBold, 12.0))
                        .color(palette.muted),
                );
                for chunk in list.chunks(4) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        for sticker in chunk {
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::splat(96.0), Sense::click());
                            if response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    CornerRadius::same(4),
                                    palette.hover,
                                );
                            }
                            match sticker.url().and_then(|u| app.picture(Some(&u))) {
                                Some(uri) => {
                                    egui::Image::new(uri).paint_at(ui, rect.shrink(6.0));
                                }
                                None => {
                                    ui.painter().text(
                                        rect.center(),
                                        Align2::CENTER_CENTER,
                                        &sticker.name,
                                        font(Weight::Medium, 11.0),
                                        palette.muted,
                                    );
                                }
                            }
                            if response.on_hover_text(&sticker.name).clicked() {
                                chosen = Some(sticker.id);
                            }
                        }
                    });
                }
            }
            if !any {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Nenhuma figurinha por aqui")
                            .font(font(Weight::SemiBold, 16.0))
                            .color(palette.text),
                    );
                    ui.label(
                        RichText::new("Este servidor ainda não tem figurinhas.")
                            .color(palette.muted),
                    );
                });
            }
        });
    if let Some(id) = chosen {
        app.actions.push(Action::Chat(ChatAction::PickSticker(id)));
    }
}

/// Discord's dialogs about limits, deleting and failures.
pub fn dialogs(app: &mut App, ui: &mut Ui) {
    let Some(dialog) = &app.chat.dialog else {
        return;
    };
    let palette = app.palette;
    let limit = size_text(app.model.max_upload());
    let max = app.model.max_message();
    let (title, body, confirm, danger): (String, String, Option<&str>, bool) = match dialog {
        Dialog::TooLong { length, .. } => (
            "Sua mensagem é longa demais".into(),
            format!("Mensagens podem ter até {max} caracteres e a sua tem {length}. Você pode enviá-la como um arquivo de texto."),
            Some("Enviar como arquivo"),
            false,
        ),
        Dialog::TooLarge { names } => (
            "Seus arquivos são muito poderosos".into(),
            format!("O tamanho máximo dos arquivos é {limit}, até 10 por mensagem.\n\nNão couberam: {}", names.join(", ")),
            None,
            false,
        ),
        Dialog::Delete { .. } => (
            "Excluir mensagem".into(),
            "Tem certeza que deseja excluir esta mensagem?\n\nDICA: você pode segurar Shift ao clicar em excluir mensagem para pular esta confirmação.".into(),
            Some("Excluir"),
            true,
        ),
        Dialog::Failed(reason) => ("Não foi possível enviar".into(), reason.clone(), None, false),
    };
    let mut close = false;
    let mut accept = false;
    let modal = egui::Modal::new(egui::Id::new("chat-dialog"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(
            Frame::new()
                .fill(palette.chat)
                .corner_radius(CornerRadius::same(8)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(440.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.label(
                    RichText::new(&title)
                        .font(font(Weight::SemiBold, 20.0))
                        .color(palette.strong),
                );
                ui.add_space(16.0);
                ui.label(
                    RichText::new(&body)
                        .font(font(Weight::Regular, 15.0))
                        .color(palette.text),
                );
                ui.add_space(8.0);
            });
            Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: 8,
                    se: 8,
                })
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                        let main = confirm.unwrap_or("OK");
                        let fill = if danger {
                            palette.danger
                        } else {
                            palette.accent
                        };
                        let button = egui::Button::new(
                            RichText::new(main)
                                .font(font(Weight::Medium, 14.0))
                                .color(Color32::WHITE),
                        )
                        .fill(fill)
                        .corner_radius(CornerRadius::same(3))
                        .min_size(vec2(96.0, 38.0));
                        if ui.add(button).clicked() {
                            if confirm.is_some() {
                                accept = true;
                            } else {
                                close = true;
                            }
                        }
                        if confirm.is_some() {
                            let cancel = egui::Button::new(
                                RichText::new("Cancelar")
                                    .font(font(Weight::Medium, 14.0))
                                    .color(palette.strong),
                            )
                            .frame(false)
                            .min_size(vec2(96.0, 38.0));
                            if ui.add(cancel).clicked() {
                                close = true;
                            }
                        }
                    });
                });
        });
    if ui.input(|i| i.key_pressed(Key::Enter)) && confirm.is_some() {
        accept = true;
    }
    if modal.should_close() {
        close = true;
    }
    if accept {
        let action = match dialog {
            Dialog::TooLong { .. } => ChatAction::SendAsFile,
            Dialog::Delete { .. } => ChatAction::ConfirmDelete,
            _ => ChatAction::CloseDialog,
        };
        app.actions.push(Action::Chat(action));
    } else if close {
        app.actions.push(Action::Chat(ChatAction::CloseDialog));
    }
}

/// A picture over the whole window; a click or Escape closes it.
pub fn viewer(app: &mut App, ui: &mut Ui) {
    let Some(url) = app.chat.viewer.clone() else {
        return;
    };
    let content = ui.ctx().content_rect();
    let mut close = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    egui::Area::new(egui::Id::new("image-viewer"))
        .order(egui::Order::Foreground)
        .fixed_pos(content.min)
        .show(ui.ctx(), |ui| {
            let (rect, response) = ui.allocate_exact_size(content.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, Color32::from_black_alpha(220));
            let room = rect.shrink2(vec2(rect.width() * 0.05, rect.height() * 0.08));
            match app.picture(Some(&url)) {
                Some(uri) => {
                    let image = egui::Image::new(uri)
                        .max_size(room.size())
                        .maintain_aspect_ratio(true);
                    let size = image
                        .load_and_calc_size(ui, room.size())
                        .unwrap_or(room.size());
                    image.paint_at(ui, Rect::from_center_size(room.center(), size));
                }
                None => {
                    ui.painter().text(
                        room.center(),
                        Align2::CENTER_CENTER,
                        "Carregando…",
                        font(Weight::Medium, 16.0),
                        Color32::WHITE,
                    );
                }
            }
            let link = ui.painter().layout_no_wrap(
                "Abrir no navegador".into(),
                font(Weight::Medium, 14.0),
                LINK,
            );
            let link_rect =
                Rect::from_min_size(pos2(room.left(), room.bottom() + 8.0), link.size());
            ui.painter().galley(link_rect.min, link, LINK);
            let link_response = ui.interact(link_rect, ui.id().with("viewer-open"), Sense::click());
            if link_response.clicked() {
                app.actions
                    .push(Action::Chat(ChatAction::OpenUrl(url.clone())));
            } else if response.clicked() {
                close = true;
            }
        });
    if close {
        app.actions.push(Action::Chat(ChatAction::CloseViewer));
    }
}

/// Files dragged over the window, and dropped onto it.
pub fn drop_zone(app: &mut App, ui: &mut Ui) {
    let Some(channel) = app.channel else { return };
    let (hovering, dropped) =
        ui.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
    let paths: Vec<_> = dropped.iter().map(|f| f.path().to_path_buf()).collect();
    if !paths.is_empty() {
        app.actions.push(Action::Chat(ChatAction::AddFiles(paths)));
    }
    if !hovering {
        return;
    }
    let name = app.model.channel(channel).map(|c| match c.kind {
        ChannelKind::Direct | ChannelKind::Group => format!("@{}", app.model.direct_name(c)),
        _ => format!("#{}", c.name),
    });
    let content = ui.ctx().content_rect();
    let palette = app.palette;
    egui::Area::new(egui::Id::new("drop-zone"))
        .order(egui::Order::Foreground)
        .fixed_pos(content.min)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            ui.painter()
                .rect_filled(content, CornerRadius::ZERO, Color32::from_black_alpha(180));
            let card = Rect::from_center_size(content.center(), vec2(310.0, 220.0));
            ui.painter()
                .rect_filled(card, CornerRadius::same(10), palette.accent);
            ui.painter().rect_stroke(
                card.shrink(8.0),
                CornerRadius::same(8),
                Stroke::new(2.0, Color32::from_white_alpha(120)),
                egui::StrokeKind::Inside,
            );
            paint_icon(
                ui,
                Icon::Paperclip,
                card.center() - vec2(0.0, 40.0),
                48.0,
                Color32::WHITE,
            );
            ui.painter().text(
                card.center() + vec2(0.0, 20.0),
                Align2::CENTER_CENTER,
                format!("Enviar para {}", name.unwrap_or_default()),
                font(Weight::Bold, 20.0),
                Color32::WHITE,
            );
            ui.painter().text(
                card.center() + vec2(0.0, 50.0),
                Align2::CENTER_CENTER,
                "Você pode adicionar comentários antes de enviar.",
                font(Weight::Regular, 13.0),
                Color32::WHITE,
            );
        });
}

/// Typing anywhere writes in the field, as in Discord.
pub fn type_anywhere(app: &mut App, ctx: &egui::Context) {
    let Some(channel) = app.channel else { return };
    if app.chat.dialog.is_some()
        || app.chat.viewer.is_some()
        || app.chat.editing.is_some()
        || app.chat.picker.is_some()
    {
        return;
    }
    if ctx.memory(|m| m.focused().is_some()) {
        return;
    }
    let typed: String = ctx.input_mut(|i| {
        let mut text = String::new();
        i.events.retain(|event| match event {
            egui::Event::Text(t) | egui::Event::Paste(t)
                if !i.modifiers.ctrl || matches!(event, egui::Event::Paste(_)) =>
            {
                text.push_str(t);
                false
            }
            _ => true,
        });
        text
    });
    if typed.is_empty() {
        return;
    }
    app.drafts.entry(channel).or_default().push_str(&typed);
    app.chat.focus_composer = true;
    app.chat.cursor_to_end = true;
}
