//! Threads and forums on screen: the followed threads under their channel
//! in the list, the header's thread popout, the card under a message that
//! started a thread, the new thread form, and a forum's posts (cards, or
//! a grid of pictures for a media channel).

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2,
    vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{Elide, font, paint_icon};
use crate::app::{Action, App};
use crate::icons::Icon;
use crate::model::{ChannelKind, Id, MediaKind};
use crate::text::hyphens;
use crate::threads::ThreadAction;

fn act(action: ThreadAction) -> Action {
    Action::Threads(action)
}

/// The followed threads under a channel in the list, joined to it by
/// Discord's curved line. A collapsed category keeps only the open and
/// unread ones.
pub fn rows(app: &mut App, ui: &mut Ui, parent: Id, collapsed: bool) {
    let palette = app.palette;
    let threads = app.model.joined_threads(parent);
    let threads: Vec<Id> = threads
        .into_iter()
        .filter(|t| {
            !collapsed
                || app.channel == Some(*t)
                || app.model.channel(*t).is_some_and(|c| c.unread())
        })
        .collect();
    let count = threads.len();
    for (index, id) in threads.into_iter().enumerate() {
        let Some(channel) = app.model.channel(id) else {
            continue;
        };
        let (slot, response) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
        let rect = Rect::from_min_max(
            pos2(slot.left() + 36.0, slot.top() + 1.0),
            pos2(slot.right() - 8.0, slot.bottom() - 1.0),
        );
        let selected = app.channel == Some(id);
        let unread = channel.unread();
        if selected {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), palette.selected);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), palette.hover);
        }
        // The spine down from the channel, curving into each thread.
        let x = slot.left() + 26.0;
        let stroke = Stroke::new(1.5, palette.selected);
        let top = slot.top() - if index == 0 { 6.0 } else { 0.0 };
        let mid = slot.center().y;
        ui.painter()
            .line_segment([pos2(x, top), pos2(x, mid - 6.0)], stroke);
        let curve: Vec<_> = (0..=6)
            .map(|i| {
                let a = std::f32::consts::FRAC_PI_2 * i as f32 / 6.0;
                pos2(x + 6.0 - 6.0 * a.cos(), mid - 6.0 + 6.0 * a.sin())
            })
            .collect();
        ui.painter().add(egui::Shape::line(curve, stroke));
        ui.painter()
            .line_segment([pos2(x + 6.0, mid), pos2(rect.left() + 2.0, mid)], stroke);
        if index + 1 < count {
            ui.painter()
                .line_segment([pos2(x, mid - 6.0), pos2(x, slot.bottom())], stroke);
        }
        let tint = if selected || unread {
            palette.strong
        } else if response.hovered() {
            palette.text
        } else {
            palette.muted
        };
        Elide::new(ui, rect.right() - 8.0).text(
            pos2(rect.left() + 8.0, rect.center().y),
            Align2::LEFT_CENTER,
            &channel.name,
            font(if unread || selected { Weight::SemiBold } else { Weight::Medium }, 15.0),
            tint,
        );
        if response.clicked() {
            app.actions.push(Action::OpenChannel(id));
        }
        let moderator = channel
            .guild
            .is_some_and(|g| app.model.can(g, crate::model::permission::MANAGE_THREADS));
        let mine = channel.thread.as_ref().and_then(|t| t.owner) == Some(app.model.me);
        let locked = channel.thread.as_ref().is_some_and(|t| t.locked);
        let mut chosen = None;
        response.context_menu(|ui| {
            ui.set_min_width(180.0);
            if (moderator || mine) && ui.button("Fechar tópico").clicked() {
                chosen = Some(act(ThreadAction::Archive(id, true)));
                ui.close();
            }
            if moderator
                && ui
                    .button(if locked { "Destrancar tópico" } else { "Trancar tópico" })
                    .clicked()
            {
                chosen = Some(act(ThreadAction::Lock(id, !locked)));
                ui.close();
            }
            if ui.button("Sair do tópico").clicked() {
                chosen = Some(act(ThreadAction::Join(id, false)));
                ui.close();
            }
            if ui.button("Copiar ID do tópico").clicked() {
                chosen = Some(Action::Chat(crate::chat::ChatAction::Copy(id.to_string())));
                ui.close();
            }
        });
        if let Some(action) = chosen {
            app.actions.push(action);
        }
    }
}

/// The header's thread popout: the channel's active or archived threads,
/// and "Criar" for a new one.
pub fn popout(app: &mut App, ui: &mut Ui, out: &mut Vec<Action>) {
    let palette = app.palette;
    let Some(channel) = app.channel else { return };
    let parent = app.thread_parent(channel);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
    paint_icon(
        ui,
        Icon::Threads,
        pos2(rect.left() + 26.0, rect.center().y),
        20.0,
        palette.icon,
    );
    ui.painter().text(
        pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Tópicos",
        font(Weight::SemiBold, 16.0),
        palette.strong,
    );
    // "Criar" at the right, as Discord's popout.
    let create = Rect::from_min_size(
        pos2(rect.right() - 76.0, rect.center().y - 15.0),
        vec2(64.0, 30.0),
    );
    let response = ui
        .interact(create, ui.id().with("thread-create"), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter().rect_filled(
        create,
        CornerRadius::same(4),
        if response.hovered() {
            crate::theme::mix(palette.accent, Color32::BLACK, 0.15)
        } else {
            palette.accent
        },
    );
    ui.painter().text(
        create.center(),
        Align2::CENTER_CENTER,
        "Criar",
        font(Weight::Medium, 14.0),
        Color32::WHITE,
    );
    if response.clicked() {
        out.push(act(ThreadAction::Start {
            parent,
            message: None,
        }));
    }
    // The two tabs.
    let archived = app.threads.show_archived;
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        for (on, label) in [(false, "Ativos"), (true, "Arquivados")] {
            let text = RichText::new(label)
                .font(font(Weight::Medium, 14.0))
                .color(if archived == on { palette.strong } else { palette.muted });
            if ui
                .add(egui::Button::new(text).frame(archived == on).fill(palette.selected))
                .clicked()
            {
                out.push(act(ThreadAction::ShowArchived(on)));
            }
            ui.add_space(4.0);
        }
    });
    ui.add_space(8.0);
    let list = app.threads.lists.get(&parent);
    if list.is_none() {
        out.push(act(ThreadAction::Load(parent)));
    }
    let ids: Vec<Id> = match list {
        None => Vec::new(),
        Some(l) if archived => l.archived.clone(),
        Some(l) => l.active.clone(),
    };
    let loading = list.is_none_or(|l| l.loading) && !archived;
    if ids.is_empty() {
        let note = if loading {
            "Carregando…"
        } else if archived {
            "Nenhum tópico arquivado."
        } else {
            "Não há tópicos ativos. Fique por dentro das conversas: crie um tópico."
        };
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(note).color(palette.muted));
        });
        ui.add_space(24.0);
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("thread-popout")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for id in ids {
                let Some(thread) = app.model.channel(id) else {
                    continue;
                };
                let (slot, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::click());
                let rect = slot.shrink2(vec2(12.0, 3.0));
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(8),
                    if response.hovered() { palette.hover } else { palette.chat },
                );
                Elide::new(ui, rect.right() - 12.0).text(
                    pos2(rect.left() + 14.0, rect.top() + 18.0),
                    Align2::LEFT_CENTER,
                    &thread.name,
                    font(Weight::SemiBold, 15.0),
                    palette.strong,
                );
                let meta = thread.thread.clone().unwrap_or_default();
                let owner = meta
                    .owner
                    .map(|o| app.model.name_in(thread.guild, o))
                    .unwrap_or_default();
                let when = thread
                    .last_message
                    .or(Some(id))
                    .map(|m| ago(m))
                    .unwrap_or_default();
                Elide::new(ui, rect.right() - 12.0).text(
                    pos2(rect.left() + 14.0, rect.bottom() - 18.0),
                    Align2::LEFT_CENTER,
                    format!(
                        "{owner} · {} mensagens · {when}",
                        meta.messages
                    ),
                    font(Weight::Regular, 13.0),
                    palette.muted,
                );
                if response.clicked() {
                    out.push(act(ThreadAction::Open(id)));
                }
            }
        });
}

/// "Há 3 horas", from a snowflake's time.
fn ago(id: Id) -> String {
    let millis = (id >> 22) as i64 + 1_420_070_400_000;
    let now = jiff::Timestamp::now().as_millisecond();
    let minutes = ((now - millis) / 60_000).max(0);
    match minutes {
        0 => "agora".into(),
        1..=59 => format!("há {minutes} min"),
        60..=1439 => format!("há {} h", minutes / 60),
        _ => format!("há {} d", minutes / 1440),
    }
}

/// The card under a message that started a thread: its name, how many
/// messages, and a click opens it.
pub fn card(app: &App, ui: &mut Ui, message: &crate::model::Message, out: &mut Vec<Action>) {
    let Some(link) = &message.thread else { return };
    if message.kind == 18 {
        return;
    }
    let palette = app.palette;
    ui.add_space(4.0);
    let width = ui.available_width().min(480.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 52.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        if response.hovered() { palette.hover } else { palette.sidebar },
        Stroke::new(1.0, palette.divider),
        egui::StrokeKind::Inside,
    );
    let messages = app
        .model
        .channel(link.id)
        .and_then(|c| c.thread.as_ref())
        .map_or(link.messages, |t| t.messages.max(link.messages));
    Elide::new(ui, rect.right() - 12.0).text(
        pos2(rect.left() + 14.0, rect.top() + 17.0),
        Align2::LEFT_CENTER,
        hyphens(&link.name),
        font(Weight::SemiBold, 14.0),
        palette.strong,
    );
    let count = match messages {
        0 => "Ver tópico".to_owned(),
        1 => "1 mensagem".to_owned(),
        n => format!("{n} mensagens"),
    };
    let shown = ui.painter().text(
        pos2(rect.left() + 14.0, rect.bottom() - 16.0),
        Align2::LEFT_CENTER,
        count,
        font(Weight::SemiBold, 13.0),
        crate::ui::LINK,
    );
    paint_icon(
        ui,
        Icon::ChevronRight,
        pos2(shown.right() + 8.0, shown.center().y),
        12.0,
        crate::ui::LINK,
    );
    if response.clicked() {
        out.push(act(ThreadAction::Open(link.id)));
    }
}

/// The new thread dialog, for text channels: the name, and the first
/// message, and for a channel's own button whether it is private.
pub fn draft(app: &mut App, ui: &mut Ui) {
    let Some(draft) = app.threads.draft.clone() else {
        return;
    };
    if app
        .model
        .channel(draft.parent)
        .is_some_and(|c| c.kind.is_forum())
    {
        return;
    }
    let palette = app.palette;
    let mut out = Vec::new();
    let mut edited = draft.clone();
    let response = egui::Modal::new(egui::Id::new("thread-draft"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(Frame::new().fill(palette.chat).corner_radius(CornerRadius::same(8)))
        .show(ui.ctx(), |ui| {
            ui.set_width(460.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    paint_icon(ui, Icon::Threads, ui.cursor().min + vec2(12.0, 12.0), 20.0, palette.icon);
                    ui.add_space(30.0);
                    ui.label(
                        RichText::new("Novo tópico")
                            .font(font(Weight::SemiBold, 20.0))
                            .color(palette.strong),
                    );
                });
                ui.add_space(4.0);
                label(ui, &palette, "NOME DO TÓPICO");
                let name = text_field(ui, &palette, &mut edited.name, "Dê um nome a este tópico", false);
                if ui.memory(|m| m.focused().is_none()) {
                    name.request_focus();
                }
                label(ui, &palette, "MENSAGEM INICIAL");
                text_field(ui, &palette, &mut edited.text, "Comece a conversa (opcional)", true);
                if edited.message.is_none() {
                    ui.checkbox(
                        &mut edited.private,
                        RichText::new("Tópico privado: só quem for convidado ou mencionado pode ver")
                            .color(palette.text),
                    );
                }
                if let Some(error) = &edited.error {
                    ui.label(RichText::new(error).color(palette.danger));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let create = egui::Button::new(
                            RichText::new(if edited.busy { "Criando…" } else { "Criar tópico" })
                                .font(font(Weight::Medium, 14.0))
                                .color(Color32::WHITE),
                        )
                        .fill(palette.accent)
                        .corner_radius(CornerRadius::same(4))
                        .min_size(vec2(110.0, 36.0));
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                        if (ui.add_enabled(!edited.busy, create).clicked() || enter) && !edited.busy {
                            out.push(act(ThreadAction::SubmitDraft));
                        }
                        if ui
                            .add(egui::Button::new(RichText::new("Cancelar").color(palette.text)).frame(false))
                            .clicked()
                        {
                            out.push(act(ThreadAction::CancelDraft));
                        }
                    });
                });
            });
        });
    if response.should_close() {
        out.push(act(ThreadAction::CancelDraft));
    }
    if let Some(d) = &mut app.threads.draft {
        d.name = edited.name;
        d.text = edited.text;
        d.private = edited.private;
    }
    app.actions.extend(out);
}

fn label(ui: &mut Ui, palette: &crate::theme::Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .font(font(Weight::Bold, 12.0))
            .color(palette.muted),
    );
}

fn text_field(
    ui: &mut Ui,
    palette: &crate::theme::Palette,
    text: &mut String,
    hint: &str,
    multiline: bool,
) -> egui::Response {
    Frame::new()
        .fill(palette.rail)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            let edit = if multiline {
                egui::TextEdit::multiline(text).desired_rows(3)
            } else {
                egui::TextEdit::singleline(text)
            };
            ui.add(
                edit.frame(Frame::NONE)
                    .hint_text(RichText::new(hint).color(palette.muted))
                    .font(font(Weight::Regular, 15.0))
                    .text_color(palette.text)
                    .desired_width(f32::INFINITY),
            )
        })
        .inner
}

/// A forum or media channel: "Nova postagem", the tags to filter by, and
/// the posts, as cards or as a grid of pictures.
pub fn forum(app: &mut App, ui: &mut Ui, channel: Id) {
    let palette = app.palette;
    let Some(forum) = app.model.channel(channel).cloned() else {
        return;
    };
    let media = forum.kind == ChannelKind::Media;
    let mut out: Vec<Action> = Vec::new();
    egui::ScrollArea::vertical()
        .id_salt(("forum", channel))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin::symmetric(24, 16))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    let width = ui.available_width();
                    match app.threads.draft.clone().filter(|d| d.parent == channel) {
                        Some(draft) => post_form(app, ui, &forum, draft, &mut out),
                        None => {
                            // The search-like bar with "Nova postagem".
                            let (rect, response) =
                                ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
                            ui.painter().rect_filled(rect, CornerRadius::same(8), palette.composer);
                            paint_icon(ui, Icon::Forum, pos2(rect.left() + 22.0, rect.center().y), 20.0, palette.muted);
                            ui.painter().text(
                                pos2(rect.left() + 42.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                "Pesquise ou crie uma postagem…",
                                font(Weight::Regular, 15.0),
                                palette.muted,
                            );
                            let button = Rect::from_min_size(
                                pos2(rect.right() - 134.0, rect.center().y - 16.0),
                                vec2(126.0, 32.0),
                            );
                            ui.painter().rect_filled(button, CornerRadius::same(4), palette.accent);
                            ui.painter().text(
                                button.center(),
                                Align2::CENTER_CENTER,
                                "Nova postagem",
                                font(Weight::Medium, 14.0),
                                Color32::WHITE,
                            );
                            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                out.push(act(ThreadAction::Start {
                                    parent: channel,
                                    message: None,
                                }));
                            }
                        }
                    }
                    if !forum.tags.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            paint_icon(ui, Icon::Tag, ui.cursor().min + vec2(8.0, 12.0), 16.0, palette.muted);
                            ui.add_space(20.0);
                            for tag in &forum.tags {
                                let on = app.threads.filter.contains(&tag.id);
                                if tag_chip(ui, &palette, tag, on).clicked() {
                                    out.push(act(ThreadAction::ToggleFilter(tag.id)));
                                }
                            }
                        });
                    }
                    if let Some(topic) = &forum.topic {
                        ui.label(RichText::new(hyphens(topic)).color(palette.muted));
                    }
                    ui.add_space(4.0);
                    let list = app.threads.lists.get(&channel);
                    let mut posts: Vec<Id> = list.map(|l| l.active.clone()).unwrap_or_default();
                    if app.threads.show_archived
                        && let Some(l) = list
                    {
                        posts.extend(l.archived.iter().copied());
                    }
                    let filter = app.threads.filter.clone();
                    posts.retain(|p| {
                        filter.is_empty()
                            || app
                                .model
                                .channel(*p)
                                .and_then(|c| c.thread.as_ref())
                                .is_some_and(|t| t.tags.iter().any(|x| filter.contains(x)))
                    });
                    if posts.is_empty() {
                        let loading = list.is_none_or(|l| l.loading);
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(if loading {
                                    "Carregando postagens…"
                                } else {
                                    "Nenhuma postagem por aqui ainda."
                                })
                                .font(font(Weight::Medium, 16.0))
                                .color(palette.muted),
                            );
                        });
                    } else if media {
                        grid(app, ui, channel, &posts, &mut out);
                    } else {
                        for post in posts {
                            post_card(app, ui, channel, post, &mut out);
                        }
                    }
                    ui.add_space(8.0);
                    let archived = app.threads.show_archived;
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(if archived {
                                    "Ocultar postagens antigas"
                                } else {
                                    "Mostrar postagens antigas"
                                })
                                .color(palette.text),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        out.push(act(ThreadAction::ShowArchived(!archived)));
                    }
                });
        });
    app.actions.extend(out);
}

fn tag_chip(
    ui: &mut Ui,
    palette: &crate::theme::Palette,
    tag: &crate::model::ForumTag,
    on: bool,
) -> egui::Response {
    let text = match &tag.emoji {
        Some(e) => format!("{e} {}", tag.name),
        None => tag.name.clone(),
    };
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(font(Weight::Medium, 13.0))
                .color(if on { Color32::WHITE } else { palette.text }),
        )
        .fill(if on { palette.accent } else { palette.selected })
        .corner_radius(CornerRadius::same(12)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// "Nova postagem" filled in: the title, the tags and the first message.
fn post_form(
    app: &mut App,
    ui: &mut Ui,
    forum: &crate::model::Channel,
    draft: crate::threads::Draft,
    out: &mut Vec<Action>,
) {
    let palette = app.palette;
    Frame::new()
        .fill(palette.sidebar)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(6.0, 8.0);
            let mut name = draft.name.clone();
            let mut text = draft.text.clone();
            label(ui, &palette, "TÍTULO DA POSTAGEM");
            text_field(ui, &palette, &mut name, "Título", false);
            if !forum.tags.is_empty() {
                label(ui, &palette, "TAGS");
                ui.horizontal_wrapped(|ui| {
                    let moderator = forum
                        .guild
                        .is_some_and(|g| app.model.can(g, crate::model::permission::MANAGE_CHANNELS));
                    for tag in forum.tags.iter().filter(|t| !t.moderated || moderator) {
                        if tag_chip(ui, &palette, tag, draft.tags.contains(&tag.id)).clicked() {
                            out.push(act(ThreadAction::ToggleDraftTag(tag.id)));
                        }
                    }
                });
            }
            label(ui, &palette, "MENSAGEM");
            text_field(ui, &palette, &mut text, "Escreva a primeira mensagem", true);
            if let Some(error) = &draft.error {
                ui.label(RichText::new(error).color(palette.danger));
            }
            ui.horizontal(|ui| {
                let post = egui::Button::new(
                    RichText::new(if draft.busy { "Postando…" } else { "Postar" })
                        .font(font(Weight::Medium, 14.0))
                        .color(Color32::WHITE),
                )
                .fill(palette.accent)
                .corner_radius(CornerRadius::same(4))
                .min_size(vec2(90.0, 34.0));
                if ui.add_enabled(!draft.busy, post).clicked() {
                    out.push(act(ThreadAction::SubmitDraft));
                }
                if ui
                    .add(egui::Button::new(RichText::new("Cancelar").color(palette.text)).frame(false))
                    .clicked()
                {
                    out.push(act(ThreadAction::CancelDraft));
                }
            });
            if let Some(d) = &mut app.threads.draft {
                d.name = name;
                d.text = text;
            }
        });
}

/// A forum post's card: its title and tags, the first message, and how
/// many messages it has had and when the last was.
fn post_card(app: &App, ui: &mut Ui, forum: Id, post: Id, out: &mut Vec<Action>) {
    let palette = app.palette;
    let Some(thread) = app.model.channel(post) else {
        return;
    };
    let meta = thread.thread.clone().unwrap_or_default();
    let first = app
        .threads
        .lists
        .get(&forum)
        .and_then(|l| l.first.get(&post))
        .cloned();
    let tags: Vec<String> = app
        .model
        .channel(forum)
        .map(|f| {
            f.tags
                .iter()
                .filter(|t| meta.tags.contains(&t.id))
                .map(|t| match &t.emoji {
                    Some(e) => format!("{e} {}", t.name),
                    None => t.name.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let picture = first.as_ref().and_then(|m| {
        m.attachments
            .iter()
            .find(|a| matches!(a.kind(), MediaKind::Image | MediaKind::Gif))
            .map(|a| a.proxy.clone())
    });
    let width = ui.available_width();
    let height = 104.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(8),
        if response.hovered() { palette.hover } else { palette.sidebar },
    );
    let text_right = if picture.is_some() { rect.right() - 108.0 } else { rect.right() - 16.0 };
    let mut x = rect.left() + 16.0;
    for tag in &tags {
        let galley = ui.painter().layout_no_wrap(tag.clone(), font(Weight::Medium, 12.0), palette.text);
        let chip = Rect::from_min_size(pos2(x, rect.top() + 10.0), galley.size() + vec2(12.0, 4.0));
        if chip.right() > text_right {
            break;
        }
        ui.painter().rect_filled(chip, CornerRadius::same(8), palette.selected);
        ui.painter().galley(chip.min + vec2(6.0, 2.0), galley, palette.text);
        x = chip.right() + 4.0;
    }
    let title_y = if tags.is_empty() { rect.top() + 20.0 } else { rect.top() + 38.0 };
    Elide::new(ui, text_right).text(
        pos2(rect.left() + 16.0, title_y),
        Align2::LEFT_CENTER,
        hyphens(&thread.name),
        font(Weight::SemiBold, 16.0),
        palette.strong,
    );
    if let Some(first) = &first {
        let author = app.model.name_in(thread.guild, first.author);
        let line = first.text.lines().next().unwrap_or_default();
        Elide::new(ui, text_right).text(
            pos2(rect.left() + 16.0, title_y + 22.0),
            Align2::LEFT_CENTER,
            format!("{author}: {line}"),
            font(Weight::Regular, 14.0),
            palette.muted,
        );
    }
    let y = rect.bottom() - 16.0;
    paint_icon(ui, Icon::MessageCircle, pos2(rect.left() + 24.0, y), 14.0, palette.muted);
    ui.painter().text(
        pos2(rect.left() + 36.0, y),
        Align2::LEFT_CENTER,
        format!("{}", meta.messages),
        font(Weight::Medium, 13.0),
        palette.muted,
    );
    ui.painter().text(
        pos2(rect.left() + 70.0, y),
        Align2::LEFT_CENTER,
        ago(thread.last_message.unwrap_or(post)),
        font(Weight::Regular, 13.0),
        palette.muted,
    );
    if meta.archived || meta.locked {
        ui.painter().text(
            pos2(rect.left() + 150.0, y),
            Align2::LEFT_CENTER,
            if meta.locked { "Trancada" } else { "Arquivada" },
            font(Weight::Regular, 13.0),
            palette.muted,
        );
    }
    if let Some(uri) = picture.as_deref().and_then(|p| app.picture(Some(p))) {
        let thumb = Rect::from_min_size(pos2(rect.right() - 96.0, rect.top() + 12.0), Vec2::splat(80.0));
        egui::Image::new(uri)
            .corner_radius(CornerRadius::same(6))
            .paint_at(ui, thumb);
    }
    if response.clicked() {
        out.push(act(ThreadAction::Open(post)));
    }
    ui.add_space(8.0);
}

/// A media channel's posts as square tiles of their first picture.
fn grid(app: &App, ui: &mut Ui, forum: Id, posts: &[Id], out: &mut Vec<Action>) {
    let palette = app.palette;
    let width = ui.available_width();
    let columns = ((width / 220.0).floor() as usize).max(1);
    let size = (width - 12.0 * (columns as f32 - 1.0)) / columns as f32;
    for chunk in posts.chunks(columns) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
            for post in chunk {
                let Some(thread) = app.model.channel(*post) else {
                    continue;
                };
                let first = app
                    .threads
                    .lists
                    .get(&forum)
                    .and_then(|l| l.first.get(post));
                let picture = first.and_then(|m| {
                    m.attachments
                        .iter()
                        .find(|a| a.kind() != MediaKind::File && a.kind() != MediaKind::Audio)
                        .map(|a| a.proxy.clone())
                });
                let (rect, response) = ui.allocate_exact_size(vec2(size, size + 40.0), Sense::click());
                let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                let tile = Rect::from_min_size(rect.min, Vec2::splat(size));
                ui.painter().rect_filled(tile, CornerRadius::same(8), palette.sidebar);
                match picture.as_deref().and_then(|p| app.picture(Some(p))) {
                    Some(uri) => {
                        egui::Image::new(uri)
                            .corner_radius(CornerRadius::same(8))
                            .paint_at(ui, tile);
                    }
                    None => paint_icon(ui, Icon::Image, tile.center(), 40.0, palette.muted),
                }
                if response.hovered() {
                    ui.painter().rect_filled(tile, CornerRadius::same(8), Color32::from_black_alpha(60));
                }
                Elide::new(ui, rect.right()).text(
                    pos2(rect.left() + 2.0, tile.bottom() + 18.0),
                    Align2::LEFT_CENTER,
                    hyphens(&thread.name),
                    font(Weight::SemiBold, 14.0),
                    palette.strong,
                );
                if response.clicked() {
                    out.push(act(ThreadAction::Open(*post)));
                }
            }
        });
        ui.add_space(12.0);
    }
}
