//! The login screen, laid out like Discord's: a card over the backdrop with
//! the form on the left and the QR code on the right; the two-factor and
//! token steps take the card's place.

use egui::epaint::{Mesh, Vertex};
use egui::text::{LayoutJob, TextFormat};
use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Key, Margin, Pos2, Rect, Response, Sense, Shadow,
    Stroke, Ui, Vec2, pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{font, hyphens, paint_icon};
use crate::app::{Action, App};
use crate::icons::Icon;
use crate::login::{Qr, View};
use crate::theme::Palette;

const PAD: f32 = 32.0;
const FORM: f32 = 416.0;
const QR_COLUMN: f32 = 240.0;
const GAP: f32 = 64.0;
/// Below this width the QR code goes, as in Discord.
const WIDE: f32 = FORM + GAP + QR_COLUMN + 2.0 * PAD + 48.0;

const LINK: Color32 = Color32::from_rgb(0x00, 0xa8, 0xfc);
const LABEL: Color32 = Color32::from_rgb(0xb5, 0xba, 0xc1);
const ERROR: Color32 = Color32::from_rgb(0xfa, 0x77, 0x7c);
const NOTICE: Color32 = Color32::from_rgb(0x2d, 0xc7, 0x70);

pub fn show(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    egui::CentralPanel::default()
        .frame(Frame::NONE)
        .show(ui, |ui| {
            let screen = ui.max_rect();
            backdrop(ui, screen);
            let wide = screen.width() >= WIDE && app.login.view == View::Form;
            let inner = if wide { FORM + GAP + QR_COLUMN } else { FORM };
            egui::Area::new(ui.id().with("login-card"))
                .order(egui::Order::Middle)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    Frame::new()
                        .fill(palette.chat)
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(Margin::same(PAD as i8))
                        .shadow(Shadow {
                            offset: [0, 2],
                            blur: 10,
                            spread: 0,
                            color: Color32::from_black_alpha(60),
                        })
                        .show(ui, |ui| {
                            ui.set_width(inner);
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            match app.login.view.clone() {
                                View::Form if wide => {
                                    ui.horizontal_top(|ui| {
                                        ui.vertical(|ui| {
                                            ui.set_width(FORM);
                                            form(app, ui);
                                        });
                                        ui.add_space(GAP);
                                        ui.vertical(|ui| {
                                            ui.set_width(QR_COLUMN);
                                            qr(app, ui);
                                        });
                                    });
                                }
                                View::Form => form(app, ui),
                                View::Mfa { .. } => mfa(app, ui),
                                View::Token => token(app, ui),
                            }
                        });
                });
        });
}

fn form(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let width = ui.available_width();
    centered(
        ui,
        "Boas-vindas de volta!",
        font(Weight::SemiBold, 24.0),
        palette.strong,
        width,
    );
    ui.add_space(8.0);
    centered(
        ui,
        "Estamos muito animados em ver você novamente!",
        font(Weight::Regular, 16.0),
        LABEL,
        width,
    );
    ui.add_space(20.0);

    let login = &mut app.login;
    let focus = std::mem::take(&mut login.focus);
    let email = field(
        ui,
        &palette,
        "E-mail ou número de telefone",
        login.login_error.as_deref(),
        &mut login.email,
        false,
        focus,
    );
    ui.add_space(20.0);
    let password = field(
        ui,
        &palette,
        "Senha",
        login.password_error.as_deref(),
        &mut login.password,
        true,
        false,
    );
    ui.add_space(4.0);
    if link(ui, "Esqueceu sua senha?").clicked() {
        app.actions.push(Action::Forgot);
    }
    if let Some(notice) = &app.login.notice {
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(notice)
                    .font(font(Weight::Regular, 14.0))
                    .color(NOTICE),
            )
            .wrap(),
        );
    }
    ui.add_space(20.0);
    let enter =
        (email.lost_focus() || password.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter));
    if (button(ui, &palette, "Entrar", app.login.busy).clicked() || enter) && !app.login.busy {
        app.actions.push(Action::LoginSubmit);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Precisando de uma conta? ")
                .font(font(Weight::Regular, 14.0))
                .color(palette.muted),
        );
        if link(ui, "Registre-se").clicked() {
            app.actions.push(Action::Register);
        }
    });
}

fn qr(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let width = ui.available_width();
    match &app.login.qr {
        Qr::Scanned { username, avatar } => {
            ui.add_space(24.0);
            let (rect, _) = ui.allocate_exact_size(vec2(width, 100.0), Sense::hover());
            let circle = Rect::from_center_size(rect.center(), Vec2::splat(100.0));
            match avatar {
                Some(uri) => {
                    egui::Image::new(uri.as_str())
                        .corner_radius(CornerRadius::same(50))
                        .paint_at(ui, circle);
                }
                None => {
                    ui.painter()
                        .circle_filled(circle.center(), 50.0, palette.accent);
                    let initial: String =
                        username.chars().take(1).collect::<String>().to_uppercase();
                    ui.painter().text(
                        circle.center(),
                        Align2::CENTER_CENTER,
                        initial,
                        font(Weight::SemiBold, 40.0),
                        Color32::WHITE,
                    );
                }
            }
            ui.add_space(24.0);
            centered(
                ui,
                "Confira seu celular!",
                font(Weight::SemiBold, 24.0),
                palette.strong,
                width,
            );
            ui.add_space(8.0);
            centered(
                ui,
                &format!("Entrando como {username}"),
                font(Weight::Regular, 16.0),
                LABEL,
                width,
            );
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                if link(ui, "Não é você? Recomeçar").clicked() {
                    app.actions.push(Action::RetryQr);
                }
            });
        }
        state => {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 176.0), Sense::hover());
            let plate = Rect::from_center_size(rect.center(), Vec2::splat(176.0));
            ui.painter()
                .rect_filled(plate, CornerRadius::same(4), Color32::WHITE);
            match state {
                Qr::Code(texture) => {
                    let image = plate.shrink(8.0);
                    let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
                    ui.painter().image(texture.id(), image, uv, Color32::WHITE);
                    // The app's mark in the middle; error correction covers it.
                    let mark = Rect::from_center_size(plate.center(), Vec2::splat(44.0));
                    ui.painter().rect_filled(
                        mark.expand(4.0),
                        CornerRadius::same(12),
                        Color32::WHITE,
                    );
                    ui.painter()
                        .rect_filled(mark, CornerRadius::same(12), palette.accent);
                    paint_icon(ui, Icon::Threads, mark.center(), 26.0, Color32::WHITE);
                }
                Qr::Failed(_) => {
                    paint_icon(ui, Icon::Close, plate.center(), 48.0, palette.danger);
                }
                _ => dots(ui, plate.center(), palette.accent),
            }
            ui.add_space(32.0);
            centered(
                ui,
                "Entrar com código QR",
                font(Weight::SemiBold, 24.0),
                palette.strong,
                width,
            );
            ui.add_space(8.0);
            if let Qr::Failed(message) = &app.login.qr {
                centered(ui, message, font(Weight::Regular, 16.0), ERROR, width);
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    if link(ui, "Tentar de novo").clicked() {
                        app.actions.push(Action::RetryQr);
                    }
                });
            } else {
                let job = {
                    let mut job = LayoutJob::default();
                    let body = TextFormat {
                        font_id: font(Weight::Regular, 16.0),
                        color: LABEL,
                        ..Default::default()
                    };
                    let strong = TextFormat {
                        font_id: font(Weight::SemiBold, 16.0),
                        color: LABEL,
                        ..Default::default()
                    };
                    job.append("Escaneie isto com o ", 0.0, body.clone());
                    job.append("app móvel do Discord", 0.0, strong);
                    job.append(" para fazer login imediatamente.", 0.0, body);
                    job
                };
                centered_job(ui, job, width);
            }
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                if link(ui, "Ou, entre com um token").clicked() {
                    app.actions.push(Action::ShowToken);
                }
            });
        }
    }
}

fn mfa(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let width = ui.available_width();
    centered(
        ui,
        "Autenticação multifator",
        font(Weight::SemiBold, 24.0),
        palette.strong,
        width,
    );
    ui.add_space(8.0);
    centered(
        ui,
        "Faça login com o código do seu app de autenticação ou com um código de backup.",
        font(Weight::Regular, 16.0),
        LABEL,
        width,
    );
    ui.add_space(20.0);
    let login = &mut app.login;
    let focus = std::mem::take(&mut login.focus);
    let code = field(
        ui,
        &palette,
        "Insira o código de autenticação do Discord",
        login.code_error.as_deref(),
        &mut login.code,
        false,
        focus,
    );
    ui.add_space(20.0);
    let enter = code.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    if (button(ui, &palette, "Entrar", app.login.busy).clicked() || enter) && !app.login.busy {
        app.actions.push(Action::MfaSubmit);
    }
    ui.add_space(8.0);
    if link(ui, "Voltar para o login").clicked() {
        app.actions.push(Action::LoginBack);
    }
}

fn token(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    let width = ui.available_width();
    centered(
        ui,
        "Entrar com token",
        font(Weight::SemiBold, 24.0),
        palette.strong,
        width,
    );
    ui.add_space(8.0);
    centered(
        ui,
        "O token dá acesso total à sua conta. Nunca o compartilhe com ninguém.",
        font(Weight::Regular, 16.0),
        LABEL,
        width,
    );
    ui.add_space(20.0);
    let login = &mut app.login;
    let focus = std::mem::take(&mut login.focus);
    let field = field(
        ui,
        &palette,
        "Token",
        login.token_error.as_deref(),
        &mut login.token,
        true,
        focus,
    );
    ui.add_space(20.0);
    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    if (button(ui, &palette, "Entrar", app.login.busy).clicked() || enter) && !app.login.busy {
        app.actions.push(Action::TokenSubmit);
    }
    ui.add_space(8.0);
    if link(ui, "Voltar para o login").clicked() {
        app.actions.push(Action::LoginBack);
    }
}

/// A label in capitals with Discord's red asterisk, or the error in its
/// place, and the input under it.
fn field(
    ui: &mut Ui,
    palette: &Palette,
    label: &str,
    error: Option<&str>,
    text: &mut String,
    password: bool,
    focus: bool,
) -> Response {
    let width = ui.available_width();
    let mut job = LayoutJob::default();
    let color = if error.is_some() { ERROR } else { LABEL };
    let caps = TextFormat {
        font_id: font(Weight::Bold, 12.0),
        color,
        ..Default::default()
    };
    job.append(&hyphens(&label.to_uppercase()), 0.0, caps.clone());
    match error {
        Some(error) => {
            let italic = TextFormat {
                font_id: font(Weight::Medium, 12.0),
                color,
                italics: true,
                ..Default::default()
            };
            job.append(" - ", 0.0, caps);
            job.append(error, 0.0, italic);
        }
        None => job.append(
            " *",
            0.0,
            TextFormat {
                font_id: font(Weight::Bold, 12.0),
                color: ERROR,
                ..Default::default()
            },
        ),
    }
    job.wrap.max_width = width;
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
    ui.add_space(8.0);

    let (rect, _) = ui.allocate_exact_size(vec2(width, 40.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(3), palette.rail);
    let edit = egui::TextEdit::singleline(text)
        .password(password)
        .font(font(Weight::Regular, 16.0))
        .text_color(palette.text)
        .frame(Frame::NONE)
        .vertical_align(Align::Center)
        .desired_width(width - 20.0);
    let response = ui.put(rect.shrink2(vec2(10.0, 0.0)), edit);
    if focus {
        response.request_focus();
    }
    response
}

/// The full-width blurple button; three pulsing dots while waiting.
fn button(ui: &mut Ui, palette: &Palette, text: &str, busy: bool) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 44.0), Sense::click());
    let fill = if response.is_pointer_button_down_on() {
        Color32::from_rgb(0x3c, 0x45, 0xa5)
    } else if response.hovered() {
        Color32::from_rgb(0x47, 0x52, 0xc4)
    } else {
        palette.accent
    };
    ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
    if busy {
        dots(ui, rect.center(), Color32::WHITE);
    } else {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            font(Weight::Medium, 16.0),
            Color32::WHITE,
        );
    }
    response
}

fn dots(ui: &Ui, center: Pos2, color: Color32) {
    let time = ui.input(|i| i.time) as f32;
    for i in 0..3 {
        let phase = ((time * 4.0 - i as f32 * 0.6).sin() + 1.0) / 2.0;
        let at = center + vec2((i as f32 - 1.0) * 12.0, 0.0);
        ui.painter()
            .circle_filled(at, 3.5, color.gamma_multiply(0.3 + 0.7 * phase));
    }
    ui.ctx().request_repaint();
}

fn link(ui: &mut Ui, text: &str) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(hyphens(text).into_owned(), font(Weight::Medium, 14.0), LINK);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    let hovered = response.hovered();
    ui.painter().galley(rect.min, galley, LINK);
    if hovered {
        ui.painter()
            .hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(1.0, LINK));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

fn centered(ui: &mut Ui, text: &str, font_id: egui::FontId, color: Color32, width: f32) {
    let job = LayoutJob::simple(hyphens(text).into_owned(), font_id, color, width);
    centered_job(ui, job, width);
}

fn centered_job(ui: &mut Ui, mut job: LayoutJob, width: f32) {
    job.wrap.max_width = width;
    job.halign = Align::Center;
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y), Sense::hover());
    ui.painter()
        .galley(pos2(rect.center().x, rect.top()), galley, Color32::WHITE);
}

/// Discord paints an illustration here; this is a plain gradient with a
/// few soft shapes so the card has something to sit on.
fn backdrop(ui: &Ui, rect: Rect) {
    let top = Color32::from_rgb(0x40, 0x4e, 0xed);
    let bottom = Color32::from_rgb(0x1e, 0x21, 0x5c);
    let mut mesh = Mesh::default();
    for (pos, color) in [
        (rect.left_top(), top),
        (rect.right_top(), Color32::from_rgb(0x58, 0x65, 0xf2)),
        (rect.right_bottom(), bottom),
        (rect.left_bottom(), Color32::from_rgb(0x2b, 0x2f, 0x8a)),
    ] {
        mesh.vertices.push(Vertex {
            pos,
            uv: egui::epaint::WHITE_UV,
            color,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
    let soft = Color32::from_white_alpha(10);
    let painter = ui.painter();
    painter.circle_filled(
        rect.left_bottom() + vec2(rect.width() * 0.12, -rect.height() * 0.1),
        rect.height() * 0.45,
        soft,
    );
    painter.circle_filled(
        rect.right_top() + vec2(-rect.width() * 0.08, rect.height() * 0.15),
        rect.height() * 0.35,
        soft,
    );
    painter.circle_filled(
        rect.center() + vec2(rect.width() * 0.3, rect.height() * 0.35),
        rect.height() * 0.2,
        soft,
    );
}

/// While the saved token is checked.
pub fn loading(app: &App, ui: &mut Ui) {
    let palette = app.palette;
    egui::CentralPanel::default()
        .frame(Frame::new().fill(palette.chat))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            dots(ui, rect.center(), palette.accent);
            ui.painter().text(
                rect.center() + vec2(0.0, 40.0),
                Align2::CENTER_CENTER,
                "Conectando",
                font(Weight::SemiBold, 12.0),
                palette.muted,
            );
        });
}
