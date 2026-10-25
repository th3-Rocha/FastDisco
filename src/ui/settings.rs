//! User settings over the whole window, as Discord's: the sections on the
//! left, the page on the right, and the close button with "ESC" under it.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2, pos2,
    vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{font, hyphens, paint_icon};
use crate::app::{Action, App, Flag, SettingsPage};
use crate::icons::Icon;
use crate::settings::Theme;
use crate::theme::Palette;

const NAV_WIDTH: f32 = 218.0;
const CONTENT_WIDTH: f32 = 740.0;

pub fn show(app: &mut App, ui: &mut Ui) {
    let Some(page) = app.settings_page else {
        return;
    };
    let palette = app.palette;
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        app.actions.push(Action::CloseSettings);
    }
    let width = ui.available_width();
    // The navigation sits against the middle, as in Discord.
    let left = ((width - NAV_WIDTH - CONTENT_WIDTH - 120.0) / 2.0).max(0.0) + NAV_WIDTH + 20.0;
    egui::Panel::left("settings-nav")
        .resizable(false)
        .exact_size(left)
        .show_separator_line(false)
        .frame(Frame::new().fill(palette.sidebar).inner_margin(Margin {
            left: 0,
            right: 6,
            top: 60,
            bottom: 20,
        }))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                        ui.set_width(left - 6.0);
                        ui.allocate_ui(vec2(NAV_WIDTH, ui.available_height()), |ui| {
                            ui.set_width(NAV_WIDTH);
                            navigation(app, ui, page);
                        });
                    });
                });
        });
    egui::CentralPanel::default()
        .frame(Frame::new().fill(palette.chat).inner_margin(Margin {
            left: 40,
            right: 40,
            top: 60,
            bottom: 40,
        }))
        .show(ui, |ui| {
            let full = ui.max_rect();
            // Room on the right for the close button.
            let width = CONTENT_WIDTH.min(ui.available_width() - 70.0).max(300.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(width);
                    ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
                    match page {
                        SettingsPage::Account => account(app, ui),
                        SettingsPage::Profiles => profiles(app, ui),
                        SettingsPage::Appearance => appearance(app, ui),
                        SettingsPage::Accessibility => accessibility(app, ui),
                        SettingsPage::Notifications => notifications(app, ui),
                        SettingsPage::Keybinds => keybinds(app, ui),
                        SettingsPage::Linux => linux(app, ui),
                        SettingsPage::Advanced => advanced(app, ui),
                    }
                });
            close_button(app, ui, full);
        });
}

fn navigation(app: &mut App, ui: &mut Ui, page: SettingsPage) {
    let palette = app.palette;
    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
    let header = |ui: &mut Ui, text: &str| {
        ui.add_space(6.0);
        let (rect, _) = ui.allocate_exact_size(vec2(NAV_WIDTH, 24.0), Sense::hover());
        ui.painter().text(
            pos2(rect.left() + 10.0, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            font(Weight::Bold, 12.0),
            palette.muted,
        );
    };
    let item = |app: &mut App, ui: &mut Ui, target: SettingsPage, text: &str| {
        let (rect, response) = ui.allocate_exact_size(vec2(NAV_WIDTH, 32.0), Sense::click());
        let selected = page == target;
        if selected {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), palette.selected);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), palette.hover);
        }
        let color = if selected {
            palette.strong
        } else if response.hovered() {
            palette.text
        } else {
            palette.muted
        };
        ui.painter().text(
            pos2(rect.left() + 10.0, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            font(Weight::Medium, 16.0),
            color,
        );
        if response.clicked() {
            app.actions.push(Action::OpenSettings(target));
        }
    };
    header(ui, "CONFIGURAÇÕES DE USUÁRIO");
    item(app, ui, SettingsPage::Account, "Minha conta");
    item(app, ui, SettingsPage::Profiles, "Perfis");
    separator(ui, &palette);
    header(ui, "CONFIGURAÇÕES DO APP");
    item(app, ui, SettingsPage::Appearance, "Aparência");
    item(app, ui, SettingsPage::Accessibility, "Acessibilidade");
    item(app, ui, SettingsPage::Notifications, "Notificações");
    item(app, ui, SettingsPage::Keybinds, "Atalhos de teclado");
    item(app, ui, SettingsPage::Linux, "Configurações do Linux");
    item(app, ui, SettingsPage::Advanced, "Avançado");
    separator(ui, &palette);
    let (rect, response) = ui.allocate_exact_size(vec2(NAV_WIDTH, 32.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(4), palette.hover);
    }
    ui.painter().text(
        pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Sair",
        font(Weight::Medium, 16.0),
        palette.danger,
    );
    paint_icon(
        ui,
        Icon::External,
        pos2(rect.right() - 16.0, rect.center().y),
        16.0,
        palette.danger,
    );
    if response.clicked() {
        app.actions.push(Action::AskLogout);
    }
    separator(ui, &palette);
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(NAV_WIDTH, 20.0), Sense::hover());
    let version = concat!("FastDisco ", env!("CARGO_PKG_VERSION"));
    ui.painter().text(
        pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        version,
        font(Weight::Regular, 12.0),
        palette.muted,
    );
}

fn separator(ui: &mut Ui, palette: &Palette) {
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(vec2(NAV_WIDTH, 1.0), Sense::hover());
    ui.painter().hline(
        (rect.left() + 10.0)..=(rect.right() - 10.0),
        rect.center().y,
        Stroke::new(1.0, palette.divider),
    );
    ui.add_space(6.0);
}

/// The round ✕ with "ESC" under it, at the content's top right.
fn close_button(app: &mut App, ui: &mut Ui, full: Rect) {
    let palette = app.palette;
    let center = pos2(
        (full.left() + CONTENT_WIDTH + 60.0).min(full.right() + 10.0),
        full.top() + 18.0,
    );
    let rect = Rect::from_center_size(center, Vec2::splat(36.0));
    let response = ui.interact(rect, ui.id().with("close-settings"), Sense::click());
    let color = if response.hovered() {
        palette.strong
    } else {
        palette.icon
    };
    ui.painter()
        .circle_stroke(center, 17.0, Stroke::new(2.0, color));
    if response.hovered() {
        ui.painter().circle_filled(center, 16.0, palette.hover);
    }
    paint_icon(ui, Icon::Close, center, 18.0, color);
    ui.painter().text(
        center + vec2(0.0, 30.0),
        Align2::CENTER_CENTER,
        "ESC",
        font(Weight::SemiBold, 13.0),
        palette.muted,
    );
    if response.clicked() {
        app.actions.push(Action::CloseSettings);
    }
}

fn title(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .font(font(Weight::Bold, 20.0))
            .color(palette.strong),
    );
    ui.add_space(12.0);
}

fn heading(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.add_space(12.0);
    ui.label(
        RichText::new(text)
            .font(font(Weight::Bold, 12.0))
            .color(palette.muted),
    );
}

fn divider(ui: &mut Ui, palette: &Palette) {
    ui.add_space(12.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, palette.divider),
    );
    ui.add_space(12.0);
}

/// Discord's switch: a row with a title, a note under it and the toggle.
fn switch_row(app: &mut App, ui: &mut Ui, flag: Flag, label: &str, note: &str, on: bool) {
    let palette = app.palette;
    let width = ui.available_width();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(width - 60.0);
            ui.label(
                RichText::new(label)
                    .font(font(Weight::Medium, 16.0))
                    .color(palette.strong),
            );
            if !note.is_empty() {
                ui.label(
                    RichText::new(note)
                        .font(font(Weight::Regular, 14.0))
                        .color(palette.muted),
                );
            }
        });
        let (rect, response) = ui.allocate_exact_size(vec2(40.0, 24.0), Sense::click());
        let t = ui.ctx().animate_bool(response.id, on);
        let fill = crate::theme::mix(palette.offline, palette.online, t);
        ui.painter().rect_filled(rect, CornerRadius::same(12), fill);
        let knob = pos2(rect.left() + 12.0 + t * 16.0, rect.center().y);
        ui.painter().circle_filled(knob, 9.0, Color32::WHITE);
        if response.clicked() {
            app.actions.push(Action::Toggle(flag));
        }
    });
    divider(ui, &palette);
}

fn account(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, &palette, "Minha conta");
    let Some(me) = app.model.user(app.model.me).cloned() else {
        return;
    };
    // The card: banner, avatar, name and the button to the profile page.
    Frame::new()
        .fill(palette.rail)
        .corner_radius(CornerRadius::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (banner, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 100.0), Sense::hover());
            ui.painter().rect_filled(
                banner,
                CornerRadius {
                    nw: 8,
                    ne: 8,
                    sw: 0,
                    se: 0,
                },
                me.color(),
            );
            let center = pos2(banner.left() + 56.0, banner.bottom() + 12.0);
            ui.painter().circle_filled(center, 46.0, palette.rail);
            super::widgets::avatar(ui, app, &me, center, 40.0, Some(palette.rail));
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
            ui.painter().text(
                pos2(row.left() + 116.0, row.top() + 22.0),
                Align2::LEFT_CENTER,
                hyphens(&me.name),
                font(Weight::Bold, 20.0),
                palette.strong,
            );
            let button = Rect::from_min_size(
                pos2(row.right() - 196.0, row.top() + 8.0),
                vec2(180.0, 32.0),
            );
            let response = ui.interact(button, ui.id().with("edit-profile"), Sense::click());
            let fill = if response.hovered() {
                crate::theme::mix(palette.accent, Color32::BLACK, 0.15)
            } else {
                palette.accent
            };
            ui.painter()
                .rect_filled(button, CornerRadius::same(4), fill);
            ui.painter().text(
                button.center(),
                Align2::CENTER_CENTER,
                "Editar perfil de usuário",
                font(Weight::Medium, 14.0),
                Color32::WHITE,
            );
            if response.clicked() {
                app.actions
                    .push(Action::OpenSettings(SettingsPage::Profiles));
            }
            Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(16))
                .outer_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let account = app.model.account.clone();
                    let field = |ui: &mut Ui, label: &str, value: String| {
                        ui.label(
                            RichText::new(label)
                                .font(font(Weight::Bold, 12.0))
                                .color(palette.muted),
                        );
                        ui.label(
                            RichText::new(value)
                                .font(font(Weight::Regular, 16.0))
                                .color(palette.strong),
                        );
                        ui.add_space(16.0);
                    };
                    field(ui, "NOME EXIBIDO", me.name.clone());
                    field(ui, "NOME DE USUÁRIO", me.username.clone());
                    let email = account.email.map_or_else(
                        || "Você ainda não adicionou um e-mail.".into(),
                        |e| {
                            format!(
                                "{}{}",
                                mask_email(&e),
                                if account.verified {
                                    ""
                                } else {
                                    " (não verificado)"
                                }
                            )
                        },
                    );
                    field(ui, "E-MAIL", email);
                    let phone = account.phone.map_or_else(
                        || "Você ainda não adicionou um número de telefone.".into(),
                        |p| mask_phone(&p),
                    );
                    field(ui, "TELEFONE", phone);
                });
        });
    divider(ui, &palette);
    heading(ui, &palette, "SENHA E AUTENTICAÇÃO");
    let mfa = if app.model.account.mfa {
        "Autenticação multifator ativada"
    } else {
        "Autenticação multifator desativada"
    };
    ui.label(
        RichText::new(mfa)
            .font(font(Weight::Medium, 16.0))
            .color(if app.model.account.mfa {
                palette.online
            } else {
                palette.text
            }),
    );
    ui.label(
        RichText::new(
            "Senha, e-mail e autenticação mudam no site do Discord, que pede a sua senha.",
        )
        .font(font(Weight::Regular, 14.0))
        .color(palette.muted),
    );
    if ui
        .link(
            RichText::new("Abrir configurações da conta no navegador")
                .color(Color32::from_rgb(0x00, 0xa8, 0xfc)),
        )
        .clicked()
    {
        app.actions
            .push(Action::Chat(crate::chat::ChatAction::OpenUrl(
                "https://discord.com/channels/@me".into(),
            )));
    }
}

/// "m********@gmail.com", as Discord hides it.
fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((user, domain)) => format!(
            "{}{}@{domain}",
            user.chars().next().unwrap_or('*'),
            "*".repeat(user.chars().count().saturating_sub(1).max(4))
        ),
        None => "*".repeat(email.len()),
    }
}

fn mask_phone(phone: &str) -> String {
    let digits: Vec<char> = phone.chars().collect();
    let keep = digits.len().saturating_sub(4);
    digits
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i < keep && c.is_ascii_digit() {
                '*'
            } else {
                *c
            }
        })
        .collect()
}

fn profiles(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, &palette, "Perfis");
    let me = app.model.me;
    // The form starts from the current profile once it has arrived.
    if app.profile_form.is_none() {
        app.load_profile(me);
        let profile = match app.chat.profiles.get(&me) {
            Some((crate::chat::ProfileState::Ready(p), _)) => Some(p.clone()),
            _ => None,
        };
        if let Some(p) = profile {
            let name = app
                .model
                .user(me)
                .map(|u| u.name.clone())
                .unwrap_or_default();
            app.profile_form = Some(crate::app::ProfileForm {
                name,
                bio: p.bio.clone().unwrap_or_default(),
                pronouns: p.pronouns.clone().unwrap_or_default(),
                saving: false,
                message: None,
            });
        } else {
            ui.label(RichText::new("Carregando perfil…").color(palette.muted));
            return;
        }
    }
    let Some(form) = app.profile_form.as_mut() else {
        return;
    };
    let field = |ui: &mut Ui, label: &str, text: &mut String, multiline: bool, hint: &str| {
        ui.label(
            RichText::new(label)
                .font(font(Weight::Bold, 12.0))
                .color(palette.muted),
        );
        Frame::new()
            .fill(palette.rail)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                let edit = if multiline {
                    egui::TextEdit::multiline(text).desired_rows(4)
                } else {
                    egui::TextEdit::singleline(text)
                };
                ui.add(
                    edit.frame(Frame::NONE)
                        .hint_text(RichText::new(hint).color(palette.muted))
                        .font(font(Weight::Regular, 15.0))
                        .text_color(palette.text)
                        .desired_width(f32::INFINITY),
                );
            });
        ui.add_space(12.0);
    };
    field(
        ui,
        "NOME EXIBIDO",
        &mut form.name,
        false,
        "Como as pessoas te veem",
    );
    field(
        ui,
        "PRONOMES",
        &mut form.pronouns,
        false,
        "Adicione seus pronomes",
    );
    field(
        ui,
        "SOBRE MIM",
        &mut form.bio,
        true,
        "Você pode usar markdown e links, se quiser.",
    );
    let left = 190usize.saturating_sub(form.bio.chars().count());
    ui.label(
        RichText::new(format!("{left} caracteres restantes"))
            .font(font(Weight::Regular, 12.0))
            .color(palette.muted),
    );
    if let Some(message) = &form.message {
        let color = if message.starts_with("Salvo") {
            palette.online
        } else {
            palette.danger
        };
        ui.label(RichText::new(message).color(color));
    }
    ui.add_space(8.0);
    let saving = form.saving;
    let text = if saving {
        "Salvando…"
    } else {
        "Salvar alterações"
    };
    let button = egui::Button::new(
        RichText::new(text)
            .font(font(Weight::Medium, 14.0))
            .color(Color32::WHITE),
    )
    .fill(palette.online)
    .corner_radius(CornerRadius::same(4))
    .min_size(vec2(140.0, 36.0));
    if ui.add_enabled(!saving, button).clicked() {
        app.actions.push(Action::SaveProfile);
    }
}

fn appearance(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, &palette, "Aparência");
    preview(app, ui);
    heading(ui, &palette, "TEMA");
    let omarchy = app.themes.follows_omarchy();
    let mut choices: Vec<(Theme, String, Palette)> = vec![
        (
            Theme::System,
            if omarchy {
                "Omarchy".into()
            } else {
                "Sistema".into()
            },
            {
                let mut probe = app.settings.clone();
                probe.theme = Theme::System;
                app.palette_for(&probe.theme)
            },
        ),
        (Theme::Dark, "Escuro".into(), Palette::dark()),
        (Theme::Light, "Claro".into(), Palette::light()),
    ];
    for theme in app.themes.picker_themes() {
        if theme.filename == fastframe_theme::omarchy::FILENAME {
            continue;
        }
        let name = fastframe_theme::display_name(&theme.filename).to_owned();
        choices.push((Theme::File(theme.filename.clone()), name, theme.palette));
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(16.0, 16.0);
        for (theme, name, swatch) in choices {
            let selected = app.settings.theme == theme;
            ui.vertical(|ui| {
                ui.set_width(72.0);
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::click());
                let center = rect.center();
                // Half the circle the background, half the accent.
                ui.painter().circle_filled(center, 30.0, swatch.chat);
                let half: Vec<egui::Pos2> = (0..=16)
                    .map(|i| {
                        let a = std::f32::consts::PI * (0.25 + i as f32 / 16.0);
                        center + vec2(a.cos(), a.sin()) * 30.0
                    })
                    .collect();
                ui.painter().add(egui::Shape::convex_polygon(
                    half,
                    swatch.sidebar,
                    Stroke::NONE,
                ));
                ui.painter()
                    .circle_filled(center + vec2(10.0, -10.0), 7.0, swatch.accent);
                let ring = if selected {
                    palette.accent
                } else if response.hovered() {
                    palette.selected
                } else {
                    palette.divider
                };
                ui.painter().circle_stroke(
                    center,
                    31.0,
                    Stroke::new(if selected { 3.0 } else { 1.0 }, ring),
                );
                if selected {
                    ui.painter()
                        .circle_filled(center + vec2(22.0, -22.0), 9.0, palette.accent);
                    ui.painter().text(
                        center + vec2(22.0, -22.0),
                        Align2::CENTER_CENTER,
                        "✓",
                        font(Weight::Bold, 11.0),
                        Color32::WHITE,
                    );
                }
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(hyphens(&name))
                            .font(font(Weight::Medium, 12.0))
                            .color(palette.text),
                    );
                });
                if response.on_hover_text(&name).clicked() {
                    app.actions.push(Action::SetTheme(theme.clone()));
                }
            });
        }
    });
    if omarchy {
        ui.label(
            RichText::new("Com \"Omarchy\" o FastDisco segue o tema do desktop e muda junto quando você troca de tema.")
                .font(font(Weight::Regular, 14.0))
                .color(palette.muted),
        );
    }
    divider(ui, &palette);
    heading(ui, &palette, "NÍVEL DE ZOOM");
    let mut zoom = app.settings.zoom * 100.0;
    ui.spacing_mut().slider_width = (ui.available_width() - 80.0).min(500.0);
    let response = ui.add(
        egui::Slider::new(&mut zoom, 50.0..=200.0)
            .step_by(10.0)
            .suffix("%"),
    );
    if response.changed() {
        app.actions.push(Action::SetZoom(zoom / 100.0));
    }
    ui.label(
        RichText::new("Ctrl + e Ctrl − também mudam o zoom; Ctrl 0 volta ao normal.")
            .font(font(Weight::Regular, 14.0))
            .color(palette.muted),
    );
}

/// A message in the chosen colours, as Discord's preview.
fn preview(app: &App, ui: &mut Ui) {
    let palette = app.palette;
    Frame::new()
        .fill(palette.chat)
        .stroke(Stroke::new(1.0, palette.divider))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(me) = app.model.user(app.model.me) else {
                return;
            };
            ui.horizontal_top(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                super::widgets::avatar(ui, app, me, rect.center(), 20.0, None);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(hyphens(&me.name))
                            .font(font(Weight::Medium, 16.0))
                            .color(palette.strong),
                    );
                    ui.label(
                        RichText::new("Olha que tema bonito! ✨")
                            .font(font(Weight::Regular, 16.0))
                            .color(palette.text),
                    );
                    ui.label(
                        RichText::new("Ele segue o Omarchy quando você troca de tema.")
                            .font(font(Weight::Regular, 16.0))
                            .color(palette.text),
                    );
                });
            });
        });
}

fn accessibility(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, &palette, "Acessibilidade");
    let (animate, embeds) = (app.settings.animate, app.settings.embeds);
    switch_row(
        app,
        ui,
        Flag::Animate,
        "Reproduzir GIFs automaticamente",
        "Com isto desligado, os GIFs se movem quando o mouse passa por cima.",
        animate,
    );
    switch_row(
        app,
        ui,
        Flag::Embeds,
        "Mostrar prévias de links e embeds",
        "Prévias de sites, vídeos e mensagens de bots.",
        embeds,
    );
}

fn notifications(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, &palette, "Notificações");
    let (on, sound) = (app.settings.notifications, app.settings.notification_sound);
    switch_row(
        app,
        ui,
        Flag::Notifications,
        "Ativar notificações na área de trabalho",
        "Para mensagens diretas e menções, quando a janela não está em foco.",
        on,
    );
    switch_row(
        app,
        ui,
        Flag::NotificationSound,
        "Tocar som",
        "Um som curto com cada notificação.",
        sound,
    );
}

fn keybinds(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, &palette, "Atalhos de teclado");
    super::nav::shortcut_rows(app, ui);
}

fn linux(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, &palette, "Configurações do Linux");
    if !app.autostart_asked {
        app.actions.push(Action::AskAutostart);
    }
    let tray = app.settings.close_to_tray;
    switch_row(
        app,
        ui,
        Flag::CloseToTray,
        "Minimizar para a bandeja",
        "Fechar a janela deixa o FastDisco rodando na bandeja do sistema.",
        tray,
    );
    match app.autostart {
        Some(on) => switch_row(
            app,
            ui,
            Flag::StartAtLogin,
            "Abrir o FastDisco ao iniciar a sessão",
            "Começa junto com o seu login.",
            on,
        ),
        None => {
            ui.label(
                RichText::new("Abrir ao iniciar a sessão não está disponível neste desktop.")
                    .color(palette.muted),
            );
        }
    }
}

fn advanced(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, &palette, "Avançado");
    let overlay = app.show_stats;
    switch_row(
        app,
        ui,
        Flag::Stats,
        "Painel de desempenho",
        "FPS, memória, CPU e mais no canto da janela. F12 também liga e desliga.",
        overlay,
    );
    heading(ui, &palette, "ARQUIVOS");
    if let Some(dir) = crate::settings::config_dir() {
        ui.label(
            RichText::new(dir.display().to_string())
                .font(egui::FontId::monospace(13.0))
                .color(palette.text),
        );
        if ui
            .link(
                RichText::new("Abrir pasta de configurações")
                    .color(Color32::from_rgb(0x00, 0xa8, 0xfc)),
            )
            .clicked()
        {
            app.actions
                .push(Action::Chat(crate::chat::ChatAction::OpenUrl(
                    dir.display().to_string(),
                )));
        }
    }
}
