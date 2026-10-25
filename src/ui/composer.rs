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
use crate::chat::{ChatAction, Dialog, GifView, PickerTab, PickerTarget};
use crate::discord::rest;
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
            autocomplete(app, ui, channel, field);
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
    if app
        .chat
        .recording
        .as_ref()
        .is_some_and(|r| r.channel == channel)
    {
        recording_bar(app, ui);
        return;
    }
    if app
        .interact
        .active
        .as_ref()
        .is_some_and(|a| a.channel == channel)
    {
        super::bots::command_bar(app, ui);
        return;
    }
    let palette = app.palette;
    let id = composer_id(channel);
    let focused = ui.memory(|m| m.has_focus(id));
    let mut send = false;
    let suggest = suggestions(app, channel).filter(|(_, _, list)| !list.is_empty());
    if focused && let Some((_, word, list)) = &suggest {
        // The list takes the arrows, Tab and Enter while it shows.
        let count = list.len();
        let at = app.chat.suggestion.min(count - 1);
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
            app.chat.suggestion = (at + 1) % count;
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
            app.chat.suggestion = (at + count - 1) % count;
        }
        let accept = ui.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Tab) || i.consume_key(Modifiers::NONE, Key::Enter)
        });
        if accept {
            let chosen = list[app.chat.suggestion.min(count - 1)].action(word);
            app.actions.push(chosen);
        }
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            app.chat.dismissed = Some(word.clone());
        }
    } else if focused {
        send = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if app.chat.picker.is_some() {
                act(app, ChatAction::ClosePicker);
            } else if app.chat.reply.contains_key(&channel) {
                act(app, ChatAction::CancelReply);
            } else if app.nav.popout.is_some() {
                app.actions
                    .push(Action::Nav(crate::nav::NavAction::ClosePopout));
            } else {
                // Nothing to close: Escape marks the channel read, as Discord.
                app.actions.push(Action::MarkRead(channel));
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
            // Discord's "+": a file, a poll, or the apps' commands.
            let plus = icon_button(ui, &palette, Icon::PlusCircle, 24.0, false, "Mais opções");
            let mut chosen = None;
            egui::Popup::menu(&plus)
                .align(egui::RectAlign::TOP_START)
                .show(|ui| {
                    ui.set_min_width(200.0);
                    if ui.button("Enviar um arquivo").clicked() {
                        chosen = Some(Action::Chat(ChatAction::AttachFiles));
                    }
                    if ui.button("Criar enquete").clicked() {
                        chosen = Some(Action::Interact(
                            crate::interact::InteractAction::OpenPollDraft,
                        ));
                    }
                    if ui.button("Usar apps").clicked() {
                        app.drafts.insert(channel, "/".into());
                        app.chat.focus_composer = true;
                        app.chat.cursor_to_end = true;
                    }
                });
            if let Some(action) = chosen {
                app.actions.push(action);
            }
        });
        let buttons = 5.0 * 24.0 + 4.0 * 16.0;
        let width = ui.available_width() - buttons - 16.0;
        let focus = std::mem::take(&mut app.chat.focus_composer)
            && app.chat.editing.is_none()
            && app.chat.dialog.is_none();
        let to_end = std::mem::take(&mut app.chat.cursor_to_end);
        // The checker and the user's own words, before the draft is
        // borrowed.
        let speller = app
            .settings
            .spellcheck
            .then(|| app.speller.clone())
            .flatten();
        let own: std::collections::HashSet<String> =
            app.settings.dictionary.iter().cloned().collect();
        let mut fix: Option<(std::ops::Range<usize>, String)> = None;
        let mut learn: Option<String> = None;
        let draft = app.drafts.entry(channel).or_default();
        let changed = ui
            .vertical(|ui| {
                ui.add_space(11.0);
                let response = egui::ScrollArea::vertical()
                    .id_salt(("composer-scroll", channel))
                    .max_height(ui.ctx().content_rect().height() * 0.4)
                    .show(ui, |ui| {
                        let output = egui::TextEdit::multiline(draft)
                            .id(id)
                            .hint_text(RichText::new(placeholder).color(palette.muted))
                            .font(font(Weight::Regular, 16.0))
                            .text_color(palette.text)
                            .frame(Frame::NONE)
                            .desired_rows(1)
                            .desired_width(width)
                            .show(ui);
                        if let Some(speller) = &speller {
                            let (f, l) =
                                spelling(ui, speller, &own, draft, &output, palette.danger);
                            fix = f;
                            learn = l;
                        }
                        output.response
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
        if let Some((range, word)) = fix
            && let Some(draft) = app.drafts.get_mut(&channel)
            && draft.get(range.clone()).is_some()
        {
            draft.replace_range(range, &word);
        }
        if let Some(word) = learn {
            act(app, ChatAction::AddWord(word));
        }
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
                // Discord records voice messages only on the phone; here
                // the microphone sits after the emoji.
                if icon_button(
                    ui,
                    &palette,
                    Icon::Mic,
                    24.0,
                    false,
                    "Gravar mensagem de voz",
                )
                .clicked()
                {
                    act(app, ChatAction::StartRecording);
                }
            });
        });
    });
    if send {
        app.actions.push(Action::Send);
    }
}

/// The composer while a voice message records, as the phone app shows
/// it: the bin to throw it away, a red dot and the time, the loudness
/// moving by, and the button that sends. Enter sends and Escape discards.
fn recording_bar(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let Some(recording) = &app.chat.recording else {
        return;
    };
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(50));
    let elapsed = recording.recorder.elapsed().as_secs_f32();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
    let cy = rect.center().y;

    let bin = Rect::from_center_size(pos2(rect.left() + 12.0, cy), Vec2::splat(28.0));
    let bin = ui.interact(bin, ui.id().with("voice-discard"), Sense::click());
    paint_icon(
        ui,
        Icon::Trash,
        bin.rect.center(),
        20.0,
        if bin.hovered() {
            palette.danger
        } else {
            palette.icon
        },
    );
    let discard = bin.on_hover_text("Descartar").clicked();

    // The dot breathes once a second while the microphone is open.
    let breath = 0.6 + 0.4 * (elapsed * std::f32::consts::TAU).cos().abs();
    ui.painter().circle_filled(
        pos2(rect.left() + 44.0, cy),
        5.0,
        palette.danger.gamma_multiply(breath),
    );
    let time = ui.painter().layout_no_wrap(
        crate::media::format_time(f64::from(elapsed)),
        font(Weight::Medium, 15.0),
        palette.strong,
    );
    ui.painter().galley(
        pos2(rect.left() + 58.0, cy - time.size().y / 2.0),
        time,
        palette.strong,
    );

    let send_center = pos2(rect.right() - 16.0, cy);
    let bars_left = rect.left() + 110.0;
    let bars_right = send_center.x - 32.0;
    const STEP: f32 = 5.0;
    let count = ((bars_right - bars_left) / STEP).max(0.0) as usize;
    for (i, level) in recording.recorder.levels(count).into_iter().enumerate() {
        let height = 4.0 + level * 26.0;
        let x = bars_left + i as f32 * STEP;
        ui.painter().rect_filled(
            Rect::from_center_size(pos2(x + 1.5, cy), vec2(3.0, height)),
            CornerRadius::same(2),
            palette.text,
        );
    }

    let send = Rect::from_center_size(send_center, Vec2::splat(32.0));
    let send = ui.interact(send, ui.id().with("voice-send"), Sense::click());
    ui.painter().circle_filled(
        send_center,
        16.0,
        if send.hovered() {
            Color32::from_rgb(0x47, 0x52, 0xc4)
        } else {
            palette.accent
        },
    );
    paint_icon(ui, Icon::Send, send_center, 16.0, Color32::WHITE);
    let send = send.on_hover_text("Enviar").clicked();

    if discard || ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        act(app, ChatAction::CancelRecording);
    } else if send || ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
        act(app, ChatAction::SendRecording);
    }
}

/// Red waves under the words the dictionaries do not know (not the word
/// still being typed at the end), and on a right click on one, the
/// corrections and adding it to the user's dictionary. Returns a chosen
/// correction and a word to learn.
#[allow(clippy::type_complexity)]
fn spelling(
    ui: &mut Ui,
    speller: &crate::spell::Speller,
    own: &std::collections::HashSet<String>,
    text: &str,
    output: &egui::text_edit::TextEditOutput,
    color: Color32,
) -> (Option<(std::ops::Range<usize>, String)>, Option<String>) {
    let key = output.response.id.with("spelling");
    let cached: Option<(String, Vec<std::ops::Range<usize>>)> = ui.data(|d| d.get_temp(key));
    let wrong = match cached {
        Some((old, ranges)) if old == text => ranges,
        _ => {
            let ranges = speller.misspelled(text, own);
            ui.data_mut(|d| d.insert_temp(key, (text.to_owned(), ranges.clone())));
            ranges
        }
    };
    let typing_at_end = !text.ends_with(|c: char| !c.is_alphanumeric());
    let chars_before = |byte: usize| text[..byte].chars().count();
    let painter = ui.painter();
    for range in &wrong {
        if typing_at_end && range.end == text.len() && output.response.has_focus() {
            continue;
        }
        let a = output
            .galley
            .pos_from_cursor(egui::text::CCursor::new(chars_before(range.start)));
        let b = output
            .galley
            .pos_from_cursor(egui::text::CCursor::new(chars_before(range.end)));
        if (a.bottom() - b.bottom()).abs() > 1.0 {
            continue;
        }
        let y = output.galley_pos.y + a.bottom() - 1.0;
        let (x0, x1) = (
            output.galley_pos.x + a.left(),
            output.galley_pos.x + b.left(),
        );
        let mut points = Vec::new();
        let mut x = x0;
        let mut up = true;
        while x < x1 {
            points.push(pos2(x, if up { y - 1.5 } else { y + 0.5 }));
            x += 2.0;
            up = !up;
        }
        points.push(pos2(x1, y));
        painter.add(egui::Shape::line(points, Stroke::new(1.0, color)));
    }
    // The misspelt word a right click landed on, kept while its menu is open.
    let target_key = key.with("target");
    if output.response.secondary_clicked()
        && let Some(pointer) = output.response.interact_pointer_pos()
    {
        let cursor = output.galley.cursor_from_pos(pointer - output.galley_pos);
        let byte = text
            .char_indices()
            .nth(cursor.index.0)
            .map_or(text.len(), |(b, _)| b);
        let target = wrong
            .iter()
            .find(|r| r.start <= byte && byte <= r.end)
            .map(|r| (r.clone(), speller.suggest(&text[r.clone()])));
        ui.data_mut(|d| match target {
            Some(t) => {
                d.insert_temp(target_key, t);
            }
            None => {
                d.remove::<(std::ops::Range<usize>, Vec<String>)>(target_key);
            }
        });
    }
    let target: Option<(std::ops::Range<usize>, Vec<String>)> = ui.data(|d| d.get_temp(target_key));
    let mut fix = None;
    let mut learn = None;
    if let Some((range, suggestions)) = target {
        let shown = output.response.context_menu(|ui| {
            ui.set_min_width(180.0);
            if suggestions.is_empty() {
                ui.label(RichText::new("Nenhuma sugestão").italics());
            }
            for suggestion in &suggestions {
                if ui.button(RichText::new(suggestion).strong()).clicked() {
                    fix = Some((range.clone(), suggestion.clone()));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Adicionar ao dicionário").clicked() {
                learn = text.get(range.clone()).map(str::to_lowercase);
                ui.close();
            }
        });
        if shown.is_none() || fix.is_some() || learn.is_some() {
            ui.data_mut(|d| d.remove::<(std::ops::Range<usize>, Vec<String>)>(target_key));
        }
    }
    (fix, learn)
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
                    let gif_view = app.chat.picker.as_ref().map(|p| p.gif_view);
                    let mut back = false;
                    let query = app.chat.picker.as_mut().map(|p| &mut p.query);
                    if let Some(query) = query {
                        let hint = match tab {
                            PickerTab::Gif => "Pesquisar no Tenor",
                            PickerTab::Sticker => "Encontrar figurinhas",
                            PickerTab::Emoji => "Encontre o emoji perfeito",
                        };
                        let response = ui
                            .horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                if tab == PickerTab::Gif && gif_view != Some(GifView::Home) {
                                    back = icon_button(
                                        ui,
                                        &palette,
                                        Icon::Back,
                                        32.0,
                                        false,
                                        "Voltar",
                                    )
                                    .clicked();
                                }
                                Frame::new()
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
                                    })
                                    .inner
                            })
                            .inner;
                        let response = egui::InnerResponse {
                            inner: response.clone(),
                            response,
                        };
                        if ui.memory(|m| m.focused().is_none()) {
                            response.inner.request_focus();
                        }
                        if response.inner.changed() {
                            ui.data_mut(|d| {
                                d.insert_temp(egui::Id::new("picker-edited"), ui.input(|i| i.time))
                            });
                        }
                    }
                    if back {
                        app.actions
                            .push(Action::Chat(ChatAction::GifView(GifView::Home)));
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
    let Some(picker) = &app.chat.picker else {
        return;
    };
    let view = picker.gif_view;
    let query = picker.query.trim().to_owned();
    let searched = picker.searched.clone();
    // Typing searches a moment after the typing stops; clearing the field
    // goes back to the tiles.
    let edited: f64 = ui
        .data(|d| d.get_temp(egui::Id::new("picker-edited")))
        .unwrap_or(0.0);
    let now = ui.input(|i| i.time);
    let typed = matches!(view, GifView::Home | GifView::Search)
        && searched.as_deref().unwrap_or_default() != query;
    if typed && (!query.is_empty() || view == GifView::Search) {
        if now - edited > 0.4 {
            app.actions
                .push(Action::Chat(ChatAction::SearchGifs(query.clone())));
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
    match view {
        GifView::Home => gif_tiles(app, ui),
        GifView::Favorites => {
            let list: Vec<rest::Gif> = app
                .chat
                .favorites
                .iter()
                .map(|f| rest::Gif {
                    url: f.url.clone(),
                    src: f.src.clone(),
                    width: f.width.max(1),
                    height: f.height.max(1),
                })
                .collect();
            if list.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Nenhum favorito ainda")
                            .font(font(Weight::SemiBold, 16.0))
                            .color(app.palette.text),
                    );
                    ui.label(
                        RichText::new("Clique na estrela de um GIF para favoritá-lo.")
                            .color(app.palette.muted),
                    );
                });
            } else {
                gif_grid(app, ui, &list);
            }
        }
        GifView::Trending | GifView::Search => {
            let list = app.chat.gifs.clone();
            if list.is_empty() {
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 60.0), Sense::hover());
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Carregando GIFs…",
                    font(Weight::Medium, 14.0),
                    app.palette.muted,
                );
            } else {
                gif_grid(app, ui, &list);
            }
        }
    }
}

/// A looping preview: an MP4 through the decoder, else a picture.
fn paint_gif(app: &App, ui: &Ui, src: &str, rect: Rect) -> bool {
    let radius = CornerRadius::same(4);
    if src.ends_with(".mp4") || src.contains(".mp4?") {
        if let Some(frame) = app.animation(src) {
            egui::Image::new((frame.id(), rect.size()))
                .corner_radius(radius)
                .paint_at(ui, rect);
            return true;
        }
    } else if let Some(uri) = app.picture(Some(src)) {
        egui::Image::new(uri)
            .corner_radius(radius)
            .paint_at(ui, rect);
        return true;
    }
    ui.painter().rect_filled(rect, radius, app.palette.rail);
    false
}

/// The first screen: favourites, trending, then the trending searches.
fn gif_tiles(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let mut tiles: Vec<(Option<Icon>, String, Option<String>, ChatAction)> = vec![
        (
            Some(Icon::Star),
            "Favoritos".into(),
            app.chat.favorites.first().map(|f| f.src.clone()),
            ChatAction::GifView(GifView::Favorites),
        ),
        (
            Some(Icon::Trending),
            "GIFs em alta".into(),
            app.chat.gif_home.as_ref().and_then(|h| h.trending.clone()),
            ChatAction::GifView(GifView::Trending),
        ),
    ];
    for (name, src) in app
        .chat
        .gif_home
        .as_ref()
        .map(|h| h.categories.clone())
        .unwrap_or_default()
    {
        tiles.push((None, name.clone(), Some(src), ChatAction::SearchGifs(name)));
    }
    let mut chosen = None;
    egui::ScrollArea::vertical()
        .id_salt("gif-tiles")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let width = ui.available_width();
            let gap = 8.0;
            let column = (width - gap) / 2.0;
            let height = 110.0;
            let origin = ui.cursor().min;
            let rows = tiles.len().div_ceil(2);
            let (_area, _) =
                ui.allocate_exact_size(vec2(width, rows as f32 * (height + gap)), Sense::hover());
            for (i, (icon, name, src, action)) in tiles.into_iter().enumerate() {
                let rect = Rect::from_min_size(
                    origin
                        + vec2(
                            (i % 2) as f32 * (column + gap),
                            (i / 2) as f32 * (height + gap),
                        ),
                    vec2(column, height),
                );
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let response = ui.interact(rect, ui.id().with(("gif-tile", i)), Sense::click());
                if let Some(src) = &src {
                    paint_gif(app, ui, src, rect);
                } else {
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(4), palette.rail);
                }
                // Discord's tiles: dimmed, the title in the middle.
                let dim = if response.hovered() { 90 } else { 140 };
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(4),
                    Color32::from_black_alpha(dim),
                );
                if response.hovered() {
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(4),
                        Stroke::new(2.0, palette.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                match icon {
                    Some(icon) => {
                        let label = ui.painter().layout_no_wrap(
                            name,
                            font(Weight::Bold, 16.0),
                            Color32::WHITE,
                        );
                        let total = 22.0 + 6.0 + label.size().x;
                        let left = rect.center().x - total / 2.0;
                        paint_icon(
                            ui,
                            icon,
                            pos2(left + 11.0, rect.center().y),
                            22.0,
                            Color32::WHITE,
                        );
                        ui.painter().galley(
                            pos2(left + 28.0, rect.center().y - label.size().y / 2.0),
                            label,
                            Color32::WHITE,
                        );
                    }
                    None => {
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            name,
                            font(Weight::Bold, 16.0),
                            Color32::WHITE,
                        );
                    }
                }
                if response.clicked() {
                    chosen = Some(action);
                }
            }
        });
    if let Some(action) = chosen {
        app.actions.push(Action::Chat(action));
    }
}

/// Results in two columns, each at its own height; a star to favourite.
fn gif_grid(app: &mut App, ui: &mut Ui, list: &[rest::Gif]) {
    let palette = app.palette;
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
            for gif in list {
                let col = if heights[0] <= heights[1] { 0 } else { 1 };
                let h = column * gif.height.max(1) as f32 / gif.width.max(1) as f32;
                rects.push(Rect::from_min_size(
                    origin + vec2(col as f32 * (column + gap), heights[col]),
                    vec2(column, h),
                ));
                heights[col] += h + gap;
            }
            let (_area, _) =
                ui.allocate_exact_size(vec2(width, heights[0].max(heights[1])), Sense::hover());
            for (gif, rect) in list.iter().zip(rects) {
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let response = ui.interact(rect, ui.id().with(("gif", &gif.url)), Sense::click());
                paint_gif(app, ui, &gif.src, rect);
                let favorite = app.chat.favorites.iter().any(|f| f.url == gif.url);
                let star = Rect::from_min_size(
                    pos2(rect.right() - 34.0, rect.top() + 6.0),
                    Vec2::splat(28.0),
                );
                let star_response =
                    ui.interact(star, ui.id().with(("star", &gif.url)), Sense::click());
                if response.hovered() || star_response.hovered() {
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(4),
                        Stroke::new(2.0, palette.accent),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.hovered() || star_response.hovered() || favorite {
                    ui.painter().rect_filled(
                        star,
                        CornerRadius::same(6),
                        Color32::from_black_alpha(150),
                    );
                    let (icon, tint) = if favorite {
                        (Icon::StarFill, Color32::from_rgb(0xf0, 0xb2, 0x32))
                    } else {
                        (Icon::Star, Color32::WHITE)
                    };
                    paint_icon(ui, icon, star.center(), 18.0, tint);
                }
                if star_response
                    .on_hover_text(if favorite {
                        "Remover dos favoritos"
                    } else {
                        "Adicionar aos favoritos"
                    })
                    .clicked()
                {
                    let gif = crate::discord::frecency::Favorite {
                        url: gif.url.clone(),
                        src: gif.src.clone(),
                        width: gif.width,
                        height: gif.height,
                        order: 0,
                    };
                    chosen = Some(ChatAction::Favorite {
                        gif,
                        add: !favorite,
                    });
                } else if response.clicked() {
                    chosen = Some(ChatAction::PickGif(gif.url.clone()));
                }
            }
        });
    if let Some(action) = chosen {
        app.actions.push(Action::Chat(action));
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
            // GIFs that are really MP4s loop here too, decoded larger.
            let video = url.contains(".mp4");
            let frame = video
                .then(|| app.animation(&format!("{url}{}", crate::animation::LARGE)))
                .flatten();
            let picture = if video { None } else { app.picture(Some(&url)) };
            match (frame, picture) {
                (Some(frame), _) => {
                    let size = frame.size_vec2();
                    let scale = (room.width() / size.x).min(room.height() / size.y).min(2.0);
                    let shown = Rect::from_center_size(room.center(), size * scale);
                    egui::Image::new((frame.id(), shown.size()))
                        .corner_radius(CornerRadius::same(4))
                        .paint_at(ui, shown);
                }
                (None, Some(uri)) => {
                    let image = egui::Image::new(uri)
                        .max_size(room.size())
                        .maintain_aspect_ratio(true);
                    let size = image
                        .load_and_calc_size(ui, room.size())
                        .unwrap_or(room.size());
                    image.paint_at(ui, Rect::from_center_size(room.center(), size));
                }
                (None, None) => {
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

/// One autocomplete row.
pub struct Suggestion {
    label: String,
    detail: Option<String>,
    hint: Hint,
    /// What replaces the word in the draft.
    insert: String,
    /// What the inserted text stands for when sent.
    raw: Option<String>,
}

enum Hint {
    /// A slash command: its entry in the channel's command list, and its
    /// app's picture.
    Command(usize, String),
    User(Id),
    Emoji(String),
    Custom(String),
    Channel,
    Everyone,
}

/// The word at the end of the draft, if the draft does not end in a space.
fn last_word(draft: &str) -> Option<&str> {
    if draft.ends_with(char::is_whitespace) {
        return None;
    }
    draft
        .rsplit(char::is_whitespace)
        .next()
        .filter(|w| !w.is_empty())
}

/// Discord's autocomplete for the word being typed: @people, :emoji: and
/// #channels. Returns the list's title, the word and the rows.
fn suggestions(app: &App, channel: Id) -> Option<(String, String, Vec<Suggestion>)> {
    let draft = app.drafts.get(&channel)?;
    // "/" at the start lists the apps' commands, as Discord.
    if let Some(query) = draft.strip_prefix('/')
        && !draft.contains(char::is_whitespace)
        && app.chat.dismissed.as_deref() != Some(draft.as_str())
    {
        let index = app.interact.commands.get(&app.command_scope(channel))?;
        let query = query.to_lowercase();
        let mut list: Vec<(u8, Suggestion)> = index
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                let command = index.commands.get(entry.command)?;
                let name = std::iter::once(command.name.as_str())
                    .chain(entry.path.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" ");
                let lower = name.to_lowercase();
                let rank = if lower.starts_with(&query) {
                    0
                } else if lower.contains(&query) {
                    1
                } else {
                    return None;
                };
                let app_name = index.apps.get(&command.application);
                let detail = match (app_name, entry.description.is_empty()) {
                    (Some(a), false) => format!("{}  •  {a}", entry.description),
                    (Some(a), true) => a.clone(),
                    (None, _) => entry.description.clone(),
                };
                Some((
                    rank,
                    Suggestion {
                        label: format!("/{name}"),
                        detail: Some(detail),
                        hint: Hint::Command(i, index.icon(command.application)),
                        insert: String::new(),
                        raw: None,
                    },
                ))
            })
            .collect();
        list.sort_by(|a, b| (a.0, &a.1.label).cmp(&(b.0, &b.1.label)));
        let list = list.into_iter().map(|(_, s)| s).take(25).collect();
        return Some(("COMANDOS DE APPS".into(), draft.clone(), list));
    }
    let word = last_word(draft)?.to_owned();
    if app.chat.dismissed.as_deref() == Some(word.as_str()) {
        return None;
    }
    let guild = app.server();
    let lower = word.to_lowercase();
    let mut list = Vec::new();
    if let Some(query) = lower.strip_prefix('@') {
        let mut users: Vec<Id> = match guild.and_then(|g| app.model.guild(g)) {
            Some(g) => g.members.keys().copied().collect(),
            None => app
                .model
                .channel(channel)
                .map(|c| c.recipients.clone())
                .unwrap_or_default(),
        };
        users.sort_unstable();
        users.dedup();
        let mut scored: Vec<(u8, String, Suggestion)> = users
            .into_iter()
            .filter_map(|id| {
                let user = app.model.user(id)?;
                let name = app.model.name_in(guild, id);
                let (n, u) = (name.to_lowercase(), user.username.to_lowercase());
                let rank = if n.starts_with(query) || u.starts_with(query) {
                    0
                } else if n.contains(query) || u.contains(query) {
                    1
                } else {
                    return None;
                };
                Some((
                    rank,
                    n,
                    Suggestion {
                        label: name.clone(),
                        detail: Some(user.username.clone()),
                        hint: Hint::User(id),
                        insert: format!("@{name}"),
                        raw: Some(format!("<@{id}>")),
                    },
                ))
            })
            .collect();
        scored.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        list.extend(scored.into_iter().map(|(_, _, s)| s).take(10));
        if guild.is_some() {
            for (word, detail) in [
                (
                    "everyone",
                    "Notificar todos que têm permissão para ver este canal.",
                ),
                (
                    "here",
                    "Notificar todos online que têm permissão para ver este canal.",
                ),
            ] {
                if word.starts_with(query) {
                    list.push(Suggestion {
                        label: format!("@{word}"),
                        detail: Some(detail.into()),
                        hint: Hint::Everyone,
                        insert: format!("@{word}"),
                        raw: None,
                    });
                }
            }
        }
        return Some(("MEMBROS".into(), word, list));
    }
    if let Some(query) = lower.strip_prefix(':') {
        // Discord waits for two letters before suggesting emoji.
        if query.chars().count() < 2 || query.ends_with(':') {
            return None;
        }
        if let Some(g) = guild.and_then(|g| app.model.guild(g)) {
            for e in g
                .emojis
                .iter()
                .filter(|e| e.name.to_lowercase().starts_with(query))
                .take(5)
            {
                list.push(Suggestion {
                    label: format!(":{}:", e.name),
                    detail: None,
                    hint: Hint::Custom(CustomEmoji::url(e.id, e.animated)),
                    insert: format!(":{}:", e.name),
                    raw: None,
                });
            }
        }
        for e in emojis::iter() {
            if list.len() >= 10 {
                break;
            }
            if let Some(code) = e.shortcodes().find(|c| c.starts_with(query)) {
                list.push(Suggestion {
                    label: format!(":{code}:"),
                    detail: None,
                    hint: Hint::Emoji(e.as_str().to_owned()),
                    insert: e.as_str().to_owned(),
                    raw: None,
                });
            }
        }
        return Some((format!("EMOJI CORRESPONDENTE A {word}"), word, list));
    }
    if let Some(query) = lower.strip_prefix('#') {
        let g = guild.and_then(|g| app.model.guild(g))?;
        let (loose, categories) = app.model.layout(g);
        for id in loose
            .into_iter()
            .chain(categories.into_iter().flat_map(|(_, l)| l))
        {
            let Some(c) = app.model.channel(id) else {
                continue;
            };
            if c.kind.is_textual() && c.name.to_lowercase().contains(query) && list.len() < 10 {
                let category = c
                    .parent
                    .and_then(|p| app.model.channel(p))
                    .map(|p| p.name.to_uppercase());
                list.push(Suggestion {
                    label: c.name.clone(),
                    detail: category,
                    hint: Hint::Channel,
                    insert: format!("#{}", c.name),
                    raw: Some(format!("<#{id}>")),
                });
            }
        }
        return Some(("CANAIS DE TEXTO".into(), word, list));
    }
    None
}

/// The suggestions above the field, as Discord lists them.
fn autocomplete(app: &mut App, ui: &mut Ui, channel: Id, field: Rect) {
    let Some((title, word, list)) = suggestions(app, channel) else {
        app.chat.suggestion = 0;
        return;
    };
    if list.is_empty() {
        return;
    }
    let palette = app.palette;
    let selected = app.chat.suggestion.min(list.len() - 1);
    let mut chosen = None;
    egui::Area::new(egui::Id::new("autocomplete"))
        .order(egui::Order::Foreground)
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(field.left_top() - vec2(0.0, 8.0))
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius::same(8))
                .shadow(Shadow {
                    offset: [0, 4],
                    blur: 12,
                    spread: 0,
                    color: Color32::from_black_alpha(90),
                })
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(field.width() - 16.0);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let (head, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
                    ui.painter().text(
                        pos2(head.left() + 8.0, head.center().y),
                        Align2::LEFT_CENTER,
                        hyphens(&title),
                        font(Weight::Bold, 12.0),
                        palette.muted,
                    );
                    for (i, s) in list.iter().enumerate() {
                        let (rect, response) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
                        if i == selected || response.hovered() {
                            ui.painter()
                                .rect_filled(rect, CornerRadius::same(4), palette.selected);
                        }
                        let icon = pos2(rect.left() + 20.0, rect.center().y);
                        match &s.hint {
                            Hint::User(id) => {
                                if let Some(user) = app.model.user(*id) {
                                    super::widgets::avatar(ui, app, user, icon, 12.0, None);
                                }
                            }
                            Hint::Emoji(text) => {
                                ui.painter().text(
                                    icon,
                                    Align2::CENTER_CENTER,
                                    text,
                                    font(Weight::Regular, 20.0),
                                    Color32::WHITE,
                                );
                            }
                            Hint::Custom(url) => {
                                if let Some(uri) = app.picture(Some(url)) {
                                    egui::Image::new(uri).paint_at(
                                        ui,
                                        Rect::from_center_size(icon, Vec2::splat(22.0)),
                                    );
                                }
                            }
                            Hint::Channel => paint_icon(ui, Icon::Hash, icon, 20.0, palette.muted),
                            Hint::Command(_, url) => {
                                // The app's round picture, as in Discord.
                                let r = Rect::from_center_size(icon, Vec2::splat(24.0));
                                match app.picture(Some(url)) {
                                    Some(uri) => egui::Image::new(uri)
                                        .corner_radius(CornerRadius::same(12))
                                        .paint_at(ui, r),
                                    None => {
                                        ui.painter().circle_filled(icon, 12.0, palette.chat);
                                    }
                                }
                            }
                            Hint::Everyone => {
                                ui.painter().text(
                                    icon,
                                    Align2::CENTER_CENTER,
                                    "@",
                                    font(Weight::Bold, 18.0),
                                    palette.muted,
                                );
                            }
                        }
                        let label = ui.painter().text(
                            pos2(rect.left() + 40.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            hyphens(&s.label),
                            font(Weight::Medium, 15.0),
                            palette.strong,
                        );
                        if let Some(detail) = &s.detail {
                            Elide::new(ui, rect.right() - 8.0).text(
                                pos2(label.right() + 8.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                hyphens(detail),
                                font(Weight::Regular, 13.0),
                                palette.muted,
                            );
                        }
                        if response.hovered() {
                            app.chat.suggestion = i;
                        }
                        if response.clicked() {
                            chosen = Some(s.action(&word));
                        }
                    }
                });
        });
    if let Some(action) = chosen {
        app.actions.push(action);
    }
}

impl Suggestion {
    /// Picking it: a command opens its form, anything else goes into the
    /// draft.
    fn action(&self, word: &str) -> Action {
        match self.hint {
            Hint::Command(entry, _) => {
                Action::Interact(crate::interact::InteractAction::StartCommand(entry))
            }
            _ => Action::Chat(ChatAction::Complete {
                word: word.to_owned(),
                insert: self.insert.clone(),
                raw: self.raw.clone(),
            }),
        }
    }
}
