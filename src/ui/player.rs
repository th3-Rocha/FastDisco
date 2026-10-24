//! Players in the conversation: videos (attachments, GIF videos, YouTube),
//! audio files and voice messages. The playing one is `app.chat.player`;
//! the rest show a poster and a play button.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::message::{Row, size_text, spoiler_pill};
use super::widgets::{font, hyphens, paint_icon};
use crate::chat::ChatAction;
use crate::icons::Icon;
use crate::media::{self, Player, State};
use crate::model::Attachment;

const CARD: Color32 = Color32::from_rgb(0x2b, 0x2d, 0x31);
const BORDER: Color32 = Color32::from_rgb(0x1e, 0x1f, 0x22);
const LINK: Color32 = Color32::from_rgb(0x00, 0xa8, 0xfc);

fn active<'a>(row: &Row<'a>, key: &str) -> Option<&'a Player> {
    row.app.chat.player.as_ref().filter(|p| p.key == key)
}

/// A video: the poster with a play button, or the playing picture with
/// controls along its bottom.
pub fn video(
    row: &mut Row<'_>,
    ui: &mut Ui,
    key: &str,
    source: &str,
    poster: Option<&str>,
    size: Vec2,
    spoiler: bool,
) {
    let app = row.app;
    let palette = app.palette;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let radius = CornerRadius::same(8);
    let hidden = spoiler && !app.chat.revealed_media.contains(key);
    match active(row, key) {
        Some(player) => {
            ui.painter().rect_filled(rect, radius, Color32::BLACK);
            match (&player.texture, &player.state) {
                (_, State::Failed(error)) => {
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        error,
                        font(Weight::Medium, 14.0),
                        palette.text,
                    );
                }
                (Some(texture), _) => {
                    let image_size = texture.size_vec2();
                    let scale = (rect.width() / image_size.x).min(rect.height() / image_size.y);
                    let shown = Rect::from_center_size(rect.center(), image_size * scale);
                    egui::Image::new((texture.id(), shown.size()))
                        .corner_radius(radius)
                        .paint_at(ui, shown);
                }
                (None, _) if !player.has_video && player.state != State::Loading => {
                    paint_icon(ui, Icon::Music, rect.center(), 48.0, palette.muted);
                }
                _ => dots(ui, rect.center()),
            }
            let controls =
                Rect::from_min_max(pos2(rect.left(), rect.bottom() - 36.0), rect.right_bottom());
            let show_controls = response.hovered()
                || ui.rect_contains_pointer(controls)
                || player.state != State::Playing;
            if show_controls {
                ui.painter().rect_filled(
                    controls,
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 8,
                        se: 8,
                    },
                    Color32::from_black_alpha(170),
                );
                bar(
                    row,
                    ui,
                    player,
                    controls.shrink2(vec2(8.0, 0.0)),
                    Color32::WHITE,
                );
            }
            if response.clicked() && !ui.rect_contains_pointer(controls) {
                row.out
                    .push(crate::app::Action::Chat(ChatAction::PlayerToggle));
            }
        }
        None => {
            match poster.and_then(|p| app.picture(Some(p))) {
                Some(uri) => {
                    let mut image = egui::Image::new(uri).corner_radius(radius);
                    if hidden {
                        image = image.tint(Color32::from_gray(25));
                    }
                    image.paint_at(ui, rect);
                }
                None => {
                    ui.painter().rect_filled(rect, radius, CARD);
                }
            }
            if hidden {
                spoiler_pill(ui, rect);
            } else {
                let hovered = response.hovered();
                let fill = if hovered {
                    Color32::from_black_alpha(220)
                } else {
                    Color32::from_black_alpha(170)
                };
                ui.painter().circle_filled(rect.center(), 24.0, fill);
                paint_icon(
                    ui,
                    Icon::Play,
                    rect.center() + vec2(2.0, 0.0),
                    24.0,
                    Color32::WHITE,
                );
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                let action = if hidden {
                    ChatAction::RevealMedia(key.to_owned())
                } else if source.starts_with("ytdl:") && !media::has_ytdlp() {
                    ChatAction::OpenUrl(key.to_owned())
                } else {
                    ChatAction::Play {
                        key: key.to_owned(),
                        source: source.to_owned(),
                    }
                };
                row.out.push(crate::app::Action::Chat(action));
            }
            if source.starts_with("ytdl:") {
                let open = Rect::from_min_size(
                    pos2(rect.right() - 36.0, rect.top() + 8.0),
                    Vec2::splat(28.0),
                );
                let response = ui.interact(open, ui.id().with(("open", key)), Sense::click());
                ui.painter().rect_filled(
                    open,
                    CornerRadius::same(4),
                    Color32::from_black_alpha(170),
                );
                paint_icon(ui, Icon::External, open.center(), 16.0, Color32::WHITE);
                if response.on_hover_text("Abrir no navegador").clicked() {
                    row.out.push(crate::app::Action::Chat(ChatAction::OpenUrl(
                        key.to_owned(),
                    )));
                }
            }
        }
    }
}

/// Play, time, progress, mute; `rect` is the strip they share.
fn bar(row: &mut Row<'_>, ui: &mut Ui, player: &Player, rect: Rect, tint: Color32) {
    let y = rect.center().y;
    let play = Rect::from_center_size(pos2(rect.left() + 12.0, y), Vec2::splat(24.0));
    let response = ui.interact(play, ui.id().with(("play", &player.key)), Sense::click());
    let icon = if player.state == State::Playing {
        Icon::Pause
    } else {
        Icon::Play
    };
    paint_icon(ui, icon, play.center(), 18.0, tint);
    if response.clicked() {
        row.out
            .push(crate::app::Action::Chat(ChatAction::PlayerToggle));
    }
    let duration = player.duration.unwrap_or(0.0);
    let time = format!(
        "{} / {}",
        media::format_time(player.position()),
        media::format_time(duration)
    );
    let time_galley = ui
        .painter()
        .layout_no_wrap(time, font(Weight::Medium, 12.0), tint);
    let time_w = time_galley.size().x;
    ui.painter().galley(
        pos2(play.right() + 8.0, y - time_galley.size().y / 2.0),
        time_galley,
        tint,
    );
    let mute = Rect::from_center_size(pos2(rect.right() - 12.0, y), Vec2::splat(24.0));
    let mute_response = ui.interact(mute, ui.id().with(("mute", &player.key)), Sense::click());
    paint_icon(
        ui,
        if player.muted {
            Icon::VolumeOff
        } else {
            Icon::Volume
        },
        mute.center(),
        18.0,
        tint,
    );
    if mute_response.clicked() {
        row.out
            .push(crate::app::Action::Chat(ChatAction::PlayerMute));
    }
    let track = Rect::from_min_max(
        pos2(play.right() + 16.0 + time_w, y - 8.0),
        pos2(mute.left() - 12.0, y + 8.0),
    );
    if track.width() > 20.0 {
        progress(row, ui, player, track, duration);
    }
}

fn progress(row: &mut Row<'_>, ui: &mut Ui, player: &Player, track: Rect, duration: f64) {
    let palette = row.app.palette;
    let response = ui.interact(
        track,
        ui.id().with(("seek", &player.key)),
        Sense::click_and_drag(),
    );
    let line = Rect::from_center_size(track.center(), vec2(track.width(), 4.0));
    ui.painter()
        .rect_filled(line, CornerRadius::same(2), Color32::from_white_alpha(60));
    let fraction = if duration > 0.0 {
        (player.position() / duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let pointer = response.interact_pointer_pos().or(response.hover_pos());
    let shown = match (response.dragged(), pointer) {
        (true, Some(p)) => ((p.x - track.left()) / track.width()).clamp(0.0, 1.0),
        _ => fraction,
    };
    let filled = Rect::from_min_max(
        line.min,
        pos2(line.left() + line.width() * shown, line.bottom()),
    );
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), palette.accent);
    if response.hovered() || response.dragged() {
        ui.painter()
            .circle_filled(pos2(filled.right(), line.center().y), 6.0, Color32::WHITE);
    }
    if (response.clicked() || response.drag_stopped())
        && let Some(p) = pointer
        && duration > 0.0
    {
        let at = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64 * duration;
        row.out
            .push(crate::app::Action::Chat(ChatAction::PlayerSeek(at)));
    }
}

/// An audio file: its name and size, then the player strip.
pub fn audio(row: &mut Row<'_>, ui: &mut Ui, file: &Attachment) {
    let app = row.app;
    let palette = app.palette;
    let width = 432.0_f32.min(ui.available_width());
    Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.horizontal(|ui| {
                let (icon, _) = ui.allocate_exact_size(vec2(26.0, 34.0), Sense::hover());
                paint_icon(ui, Icon::FileAudio, icon.center(), 26.0, palette.accent);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    let name = egui::RichText::new(hyphens(&file.name))
                        .size(15.0)
                        .color(LINK);
                    if ui
                        .add(egui::Label::new(name).truncate().sense(Sense::click()))
                        .clicked()
                    {
                        row.out.push(crate::app::Action::Chat(ChatAction::OpenUrl(
                            file.url.clone(),
                        )));
                    }
                    ui.label(
                        egui::RichText::new(size_text(file.size))
                            .size(12.0)
                            .color(palette.muted),
                    );
                });
            });
            ui.add_space(8.0);
            let (strip, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
            ui.painter()
                .rect_filled(strip, CornerRadius::same(16), Color32::from_black_alpha(80));
            match active(row, &file.url) {
                Some(player) => bar(row, ui, player, strip.shrink2(vec2(8.0, 0.0)), palette.text),
                None => idle_strip(row, ui, strip, &file.url, file.duration, palette.text),
            }
        });
}

/// The strip of a player that is not playing: play and the length.
fn idle_strip(
    row: &mut Row<'_>,
    ui: &mut Ui,
    strip: Rect,
    url: &str,
    duration: Option<f32>,
    tint: Color32,
) {
    let play = Rect::from_center_size(
        pos2(strip.left() + 20.0, strip.center().y),
        Vec2::splat(24.0),
    );
    let response = ui.interact(
        play.union(strip),
        ui.id().with(("idle", url)),
        Sense::click(),
    );
    paint_icon(ui, Icon::Play, play.center(), 18.0, tint);
    let text = format!(
        "0:00 / {}",
        duration.map_or_else(|| "--:--".into(), |d| media::format_time(d as f64))
    );
    ui.painter().text(
        pos2(play.right() + 8.0, strip.center().y),
        Align2::LEFT_CENTER,
        text,
        font(Weight::Medium, 12.0),
        tint,
    );
    if response.clicked() {
        row.out.push(crate::app::Action::Chat(ChatAction::Play {
            key: url.to_owned(),
            source: url.to_owned(),
        }));
    }
}

/// A voice message: a round play button, the waveform and the time.
pub fn voice(row: &mut Row<'_>, ui: &mut Ui, file: &Attachment) {
    let app = row.app;
    let palette = app.palette;
    let (rect, _) = ui.allocate_exact_size(
        vec2(300.0_f32.min(ui.available_width()), 48.0),
        Sense::hover(),
    );
    ui.painter().rect_filled(rect, CornerRadius::same(24), CARD);
    let player = active(row, &file.url);
    let button =
        Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), Vec2::splat(32.0));
    let response = ui.interact(button, ui.id().with(("voice", &file.url)), Sense::click());
    ui.painter().circle_filled(
        button.center(),
        16.0,
        if response.hovered() {
            Color32::from_rgb(0x47, 0x52, 0xc4)
        } else {
            palette.accent
        },
    );
    let playing = player.is_some_and(|p| p.state == State::Playing);
    paint_icon(
        ui,
        if playing { Icon::Pause } else { Icon::Play },
        button.center(),
        16.0,
        Color32::WHITE,
    );
    if response.clicked() {
        let action = match player {
            Some(_) => ChatAction::PlayerToggle,
            None => ChatAction::Play {
                key: file.url.clone(),
                source: file.url.clone(),
            },
        };
        row.out.push(crate::app::Action::Chat(action));
    }
    let duration = player
        .and_then(|p| p.duration)
        .or(file.duration.map(f64::from))
        .unwrap_or(0.0);
    let position = player.map_or(0.0, |p| p.position());
    let remaining = media::format_time(if player.is_some() {
        duration - position
    } else {
        duration
    });
    let time = ui
        .painter()
        .layout_no_wrap(remaining, font(Weight::Medium, 12.0), palette.text);
    let time_w = time.size().x;
    ui.painter().galley(
        pos2(
            rect.right() - 16.0 - time_w,
            rect.center().y - time.size().y / 2.0,
        ),
        time,
        palette.text,
    );
    let wave = Rect::from_min_max(
        pos2(button.right() + 12.0, rect.top() + 10.0),
        pos2(rect.right() - 28.0 - time_w, rect.bottom() - 10.0),
    );
    let bars = ((wave.width() / 4.0) as usize).max(1);
    let fraction = if duration > 0.0 {
        (position / duration) as f32
    } else {
        0.0
    };
    for i in 0..bars {
        let sample = if file.waveform.is_empty() {
            0.4
        } else {
            let at = i * file.waveform.len() / bars;
            file.waveform[at.min(file.waveform.len() - 1)] as f32 / 255.0
        };
        let h = (sample * wave.height()).max(3.0);
        let x = wave.left() + i as f32 * 4.0 + 1.0;
        let bar = Rect::from_center_size(pos2(x, wave.center().y), vec2(2.0, h));
        let played = (i as f32 + 0.5) / bars as f32 <= fraction;
        ui.painter().rect_filled(
            bar,
            CornerRadius::same(1),
            if played {
                Color32::WHITE
            } else {
                palette.muted
            },
        );
    }
    let seek = ui.interact(
        wave,
        ui.id().with(("voice-seek", &file.url)),
        Sense::click(),
    );
    if seek.clicked()
        && player.is_some()
        && let Some(p) = seek.interact_pointer_pos()
    {
        let at = ((p.x - wave.left()) / wave.width()).clamp(0.0, 1.0) as f64 * duration;
        row.out
            .push(crate::app::Action::Chat(ChatAction::PlayerSeek(at)));
    }
}

fn dots(ui: &Ui, center: egui::Pos2) {
    let time = ui.input(|i| i.time) as f32;
    for i in 0..3 {
        let phase = ((time * 4.0 - i as f32 * 0.6).sin() + 1.0) / 2.0;
        ui.painter().circle_filled(
            center + vec2((i as f32 - 1.0) * 12.0, 0.0),
            4.0,
            Color32::from_white_alpha((80.0 + 175.0 * phase) as u8),
        );
    }
    ui.ctx().request_repaint();
}
