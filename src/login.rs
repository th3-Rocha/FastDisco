//! The login screen's state: the form, the two-factor step, the token
//! step and the QR code beside them.

use egui::{Color32, ColorImage, TextureHandle, TextureOptions};

use crate::backend::Event;
use crate::discord::remote_auth;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    Form,
    Mfa { ticket: String },
    Token,
}

pub enum Qr {
    Waiting,
    Code(TextureHandle),
    Scanned {
        username: String,
        avatar: Option<String>,
    },
    Failed(String),
}

pub struct Login {
    pub view: View,
    pub email: String,
    pub password: String,
    pub code: String,
    pub token: String,
    pub busy: bool,
    pub login_error: Option<String>,
    pub password_error: Option<String>,
    pub code_error: Option<String>,
    pub token_error: Option<String>,
    /// "We sent you an email", under the form.
    pub notice: Option<String>,
    pub qr: Qr,
    /// Focus the first field on the next frame.
    pub focus: bool,
}

impl Default for Login {
    fn default() -> Self {
        Self {
            view: View::Form,
            email: String::new(),
            password: String::new(),
            code: String::new(),
            token: String::new(),
            busy: false,
            login_error: None,
            password_error: None,
            code_error: None,
            token_error: None,
            notice: None,
            qr: Qr::Waiting,
            focus: true,
        }
    }
}

impl Login {
    pub fn clear_errors(&mut self) {
        self.login_error = None;
        self.password_error = None;
        self.code_error = None;
        self.token_error = None;
        self.notice = None;
    }

    pub fn go(&mut self, view: View) {
        self.clear_errors();
        self.busy = false;
        self.code.clear();
        self.view = view;
        self.focus = true;
    }

    /// Takes a backend event about logging in. Returns true when the QR
    /// code must be started again.
    pub fn handle(&mut self, ctx: &egui::Context, event: Event) -> bool {
        match event {
            Event::MfaRequired { ticket } => self.go(View::Mfa { ticket }),
            Event::LoginFailed(error) => {
                self.busy = false;
                match error {
                    crate::discord::auth::Error::Fields {
                        login, password, ..
                    } => {
                        self.login_error = login;
                        self.password_error = password;
                    }
                    other => self.login_error = Some(other.message()),
                }
            }
            Event::MfaFailed(error) => {
                self.busy = false;
                self.code_error = Some(error.message());
            }
            Event::TokenFailed(message) => {
                self.busy = false;
                self.token_error = Some(message);
            }
            Event::ForgotSent => {
                self.clear_errors();
                self.notice = Some("Enviamos instruções para alterar sua senha. Confira sua caixa de entrada e o spam.".into());
            }
            Event::ForgotFailed(error) => {
                self.clear_errors();
                self.login_error = Some(match error {
                    crate::discord::auth::Error::Fields { login, .. } if login.is_some() => {
                        login.unwrap_or_default()
                    }
                    other => other.message(),
                });
            }
            Event::Qr(event) => match event {
                remote_auth::Event::Code(url) => self.qr = Qr::Code(qr_texture(ctx, &url)),
                remote_auth::Event::Scanned {
                    username, avatar, ..
                } => self.qr = Qr::Scanned { username, avatar },
                remote_auth::Event::Restart => {
                    self.qr = Qr::Waiting;
                    return true;
                }
                remote_auth::Event::Failed(message) => self.qr = Qr::Failed(message),
                remote_auth::Event::Token(_) => {}
            },
            _ => {}
        }
        false
    }
}

/// The QR code as a texture, one pixel per module; it is drawn scaled with
/// nearest filtering, so it stays sharp.
fn qr_texture(ctx: &egui::Context, url: &str) -> TextureHandle {
    let code = qrcode::QrCode::with_error_correction_level(url, qrcode::EcLevel::Q)
        .expect("a remote auth URL fits in a QR code");
    let width = code.width();
    let pixels = code
        .to_colors()
        .into_iter()
        .map(|color| match color {
            qrcode::Color::Dark => Color32::BLACK,
            qrcode::Color::Light => Color32::WHITE,
        })
        .collect();
    let image = ColorImage::new([width, width], pixels);
    ctx.load_texture("login-qr", image, TextureOptions::NEAREST)
}
