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

const NAV_WIDTH: f32 = 240.0;
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
                        SettingsPage::Voice => voice(app, ui),
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

/// The sections in Discord's new layout: who you are with "Editar perfis"
/// on top, a search, then the pages with their icons under plain titles.
fn navigation(app: &mut App, ui: &mut Ui, page: SettingsPage) {
    let palette = app.palette;
    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
    // The account, which opens the profiles.
    let (rect, response) = ui.allocate_exact_size(vec2(NAV_WIDTH, 64.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if page == SettingsPage::Profiles {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), palette.selected);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), palette.hover);
    }
    let me = app.model.me;
    if let Some(person) = app.model.user(me) {
        super::widgets::avatar(
            ui,
            app,
            person,
            pos2(rect.left() + 32.0, rect.center().y),
            26.0,
            None,
        );
    }
    let name = app.model.name_in(None, me);
    super::widgets::Elide::new(ui, rect.right() - 8.0).text(
        pos2(rect.left() + 68.0, rect.center().y - 10.0),
        Align2::LEFT_CENTER,
        name,
        font(Weight::SemiBold, 16.0),
        palette.strong,
    );
    let edit = ui.painter().text(
        pos2(rect.left() + 68.0, rect.center().y + 11.0),
        Align2::LEFT_CENTER,
        "Editar perfis",
        font(Weight::Regular, 14.0),
        palette.muted,
    );
    paint_icon(
        ui,
        Icon::Pencil,
        pos2(edit.right() + 9.0, edit.center().y),
        12.0,
        palette.muted,
    );
    if response.clicked() {
        app.actions
            .push(Action::OpenSettings(SettingsPage::Profiles));
    }
    ui.add_space(10.0);
    // The search, which narrows the list to the pages that match.
    let id = ui.id().with("settings-search");
    let mut query: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let (rect, _) = ui.allocate_exact_size(vec2(NAV_WIDTH, 40.0), Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        palette.rail,
        Stroke::new(1.0, palette.divider),
        egui::StrokeKind::Inside,
    );
    paint_icon(
        ui,
        Icon::Search,
        pos2(rect.left() + 20.0, rect.center().y),
        18.0,
        palette.muted,
    );
    let field = Rect::from_min_max(
        pos2(rect.left() + 38.0, rect.top() + 9.0),
        pos2(rect.right() - 10.0, rect.bottom() - 9.0),
    );
    ui.put(
        field,
        egui::TextEdit::singleline(&mut query)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .hint_text(RichText::new("Buscar").color(palette.muted))
            .font(font(Weight::Regular, 16.0))
            .text_color(palette.text),
    );
    ui.data_mut(|d| d.insert_temp(id, query.clone()));
    ui.add_space(10.0);
    let query = query.trim().to_lowercase();
    let shown = |text: &str| query.is_empty() || text.to_lowercase().contains(&query);
    let item = |app: &mut App, ui: &mut Ui, target: SettingsPage, icon: Icon, text: &str| {
        let (rect, response) = ui.allocate_exact_size(vec2(NAV_WIDTH, 36.0), Sense::click());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        let selected = page == target;
        if selected {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), palette.selected);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), palette.hover);
        }
        let color = if selected || response.hovered() {
            palette.strong
        } else {
            palette.text
        };
        paint_icon(
            ui,
            icon,
            pos2(rect.left() + 22.0, rect.center().y),
            20.0,
            color,
        );
        ui.painter().text(
            pos2(rect.left() + 44.0, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            font(Weight::Medium, 16.0),
            color,
        );
        if response.clicked() {
            app.actions.push(Action::OpenSettings(target));
        }
    };
    let groups: [(&str, &[Entry]); 2] = [
        (
            "",
            &[
                (SettingsPage::Account, Icon::User, "Conta"),
                (SettingsPage::Notifications, Icon::Bell, "Notificações"),
            ],
        ),
        (
            "Experiência",
            &[
                (SettingsPage::Voice, Icon::Mic, "Voz e vídeo"),
                (SettingsPage::Appearance, Icon::Palette, "Aparência"),
                (
                    SettingsPage::Accessibility,
                    Icon::Accessibility,
                    "Acessibilidade",
                ),
                (SettingsPage::Keybinds, Icon::Keyboard, "Atalhos de teclado"),
                (SettingsPage::Linux, Icon::Monitor, "Configurações do Linux"),
                (SettingsPage::Advanced, Icon::Sliders, "Avançado"),
            ],
        ),
    ];
    let mut first = true;
    for (title, pages) in groups {
        let pages: Vec<_> = pages.iter().filter(|(_, _, text)| shown(text)).collect();
        if pages.is_empty() {
            continue;
        }
        if !first {
            separator(ui, &palette);
        }
        first = false;
        if !title.is_empty() {
            let (rect, _) = ui.allocate_exact_size(vec2(NAV_WIDTH, 28.0), Sense::hover());
            ui.painter().text(
                pos2(rect.left() + 10.0, rect.center().y),
                Align2::LEFT_CENTER,
                title,
                font(Weight::Medium, 14.0),
                palette.muted,
            );
        }
        for (target, icon, text) in pages {
            item(app, ui, *target, *icon, text);
            // Under "Conta", its parts, as Discord's side menu.
            if *target == SettingsPage::Account && page == SettingsPage::Account {
                for (i, part) in ACCOUNT_PARTS.iter().enumerate() {
                    let (rect, response) = ui.allocate_exact_size(vec2(NAV_WIDTH, 30.0), Sense::click());
                    let x = rect.left() + 22.0;
                    ui.painter().vline(x, rect.y_range(), Stroke::new(1.0, palette.selected));
                    let tint = if response.hovered() { palette.strong } else { palette.muted };
                    ui.painter().text(pos2(x + 14.0, rect.center().y), Align2::LEFT_CENTER, *part, font(Weight::Medium, 15.0), tint);
                    if response.clicked() {
                        ui.data_mut(|d| d.insert_temp(account_jump(), i));
                    }
                }
            }
        }
    }
    if !shown("Sair") {
        return;
    }
    separator(ui, &palette);
    let (rect, response) = ui.allocate_exact_size(vec2(NAV_WIDTH, 36.0), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), palette.hover);
    }
    paint_icon(
        ui,
        Icon::LogOut,
        pos2(rect.left() + 22.0, rect.center().y),
        20.0,
        palette.danger,
    );
    ui.painter().text(
        pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Sair",
        font(Weight::Medium, 16.0),
        palette.danger,
    );
    if response.clicked() {
        app.actions.push(Action::AskLogout);
    }
    separator(ui, &palette);
    ui.add_space(4.0);
    let (rect, _) = ui.allocate_exact_size(vec2(NAV_WIDTH, 20.0), Sense::hover());
    let version = concat!("Discofast ", env!("CARGO_PKG_VERSION"));
    ui.painter().text(
        pos2(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        version,
        font(Weight::Regular, 12.0),
        palette.muted,
    );
}

/// A page in the navigation: where it goes, its icon and its name.
type Entry = (SettingsPage, Icon, &'static str);

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
    account_anchor(ui, 0);
    big_heading(ui, &palette, "Informações da conta");
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

    // Senha e segurança.
    divider(ui, &palette);
    account_anchor(ui, 1);
    big_heading(ui, &palette, "Senha e segurança");
    let web = |app: &mut App| {
        app.actions
            .push(Action::Chat(crate::chat::ChatAction::OpenUrl(
                "https://discord.com/channels/@me".into(),
            )));
    };
    if account_row(ui, &palette, None, "Senha", None, RowEnd::Button("Editar")).clicked() {
        web(app);
    }
    let mfa = if app.model.account.mfa { "Ativada" } else { "Desativada" };
    if account_row(ui, &palette, None, "Autenticação Multifatorial", None, RowEnd::Value(mfa)).clicked() {
        web(app);
    }
    if !app.admin.sessions_asked {
        app.actions
            .push(Action::Admin(crate::admin::AdminAction::LoadSessions));
    }
    let devices = app
        .admin
        .sessions
        .as_ref()
        .map_or_else(|| "…".to_owned(), |l| match l.len() {
            1 => "1 dispositivo".to_owned(),
            n => format!("{n} dispositivos"),
        });
    if account_row(ui, &palette, None, "Dispositivos conectados", None, RowEnd::Value(&devices)).clicked() {
        app.actions
            .push(Action::Admin(crate::admin::AdminAction::Open(crate::admin::Modal::Sessions)));
    }

    // Status da conta.
    divider(ui, &palette);
    account_anchor(ui, 2);
    big_heading(ui, &palette, "Status da conta");
    let adult = app.model.account.adult;
    let notice_id = ui.id().with("age-notice-closed");
    if adult == Some(true) && !ui.data(|d| d.get_temp::<bool>(notice_id).unwrap_or(false)) {
        let link = Color32::from_rgb(0x00, 0xa8, 0xfc);
        let closed = Frame::new()
            .fill(crate::theme::mix(palette.rail, palette.accent, 0.12))
            .stroke(Stroke::new(1.0, crate::theme::mix(palette.rail, palette.accent, 0.6)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                    ui.painter().circle_filled(dot.center(), 8.0, link);
                    ui.painter().text(dot.center(), Align2::CENTER_CENTER, "i", font(Weight::Bold, 11.0), Color32::WHITE);
                    ui.add_sized(
                        vec2(ui.available_width() - 30.0, 0.0),
                        egui::Label::new(
                            RichText::new("Os grupos de idade determinam se você pode acessar conteúdo e configurações destinados a adultos. Você está no grupo de idade adulto (18+), então nada muda para você.")
                                .font(font(Weight::Regular, 14.0))
                                .color(palette.text),
                        )
                        .wrap(),
                    );
                    let (x, response) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                    paint_icon(ui, Icon::Close, x.center(), 16.0, palette.icon);
                    response.clicked()
                })
                .inner
            })
            .inner;
        if closed {
            ui.data_mut(|d| d.insert_temp(notice_id, true));
        }
        ui.add_space(8.0);
    }
    let (group, note) = match adult {
        Some(true) => (
            "Adulto",
            "Você está no grupo de idade adulto (18+) e tem acesso a conteúdo, espaços e configurações com restrição de idade.",
        ),
        Some(false) => (
            "Adolescente",
            "Sua conta não está no grupo de idade adulto: conteúdo com restrição de idade fica bloqueado.",
        ),
        None => ("Não confirmado", "O Discord ainda não confirmou a sua faixa etária."),
    };
    account_row(ui, &palette, Some((Icon::User, palette.icon, palette.rail)), "Grupo etário", Some(note), RowEnd::Text(group));
    if account_row(
        ui,
        &palette,
        Some((Icon::Check, palette.online, crate::theme::mix(palette.rail, palette.online, 0.15))),
        "Status da Conta",
        Some("Agradecemos por respeitar os termos de serviço do Discord e as Diretrizes da Comunidade. Se você tiver uma violação, ela aparecerá aqui."),
        RowEnd::Value("Tudo certo!"),
    )
    .clicked()
    {
        web(app);
    }

    // Central da Família.
    divider(ui, &palette);
    account_anchor(ui, 3);
    big_heading(ui, &palette, "Central da Família");
    let (card, response) = ui.allocate_exact_size(vec2(ui.available_width(), 76.0), Sense::click());
    ui.painter().rect_filled(
        card,
        CornerRadius::same(12),
        if response.hovered() { palette.hover } else { palette.sidebar },
    );
    ui.painter().text(
        pos2(card.left() + 14.0, card.top() + 20.0),
        Align2::LEFT_CENTER,
        "Configurar Central da Família",
        font(Weight::SemiBold, 16.0),
        palette.strong,
    );
    let mut job = egui::text::LayoutJob::single_section(
        "Fique por dentro da experiência do seu adolescente no Discord. Veja atividades, gerencie as principais configurações de segurança e tenham boas conversas juntos.".into(),
        egui::TextFormat { font_id: font(Weight::Regular, 13.0), color: palette.muted, ..Default::default() },
    );
    job.wrap.max_width = card.width() - 60.0;
    job.wrap.max_rows = 2;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos2(card.left() + 14.0, card.top() + 32.0), galley, palette.muted);
    paint_icon(ui, Icon::ChevronRight, pos2(card.right() - 20.0, card.center().y), 20.0, palette.icon);
    if response.clicked() {
        app.actions
            .push(Action::Chat(crate::chat::ChatAction::OpenUrl(
                "https://discord.com/family-center".into(),
            )));
    }

    // Disabling and deleting.
    divider(ui, &palette);
    for (title, note, button, delete) in [
        ("Desative sua conta", "Desative temporariamente sua conta.", "Desativar conta", false),
        ("Encerrar sua conta", "Encerre sua conta permanentemente.", "Excluir conta", true),
    ] {
        let fill = if delete { palette.danger } else { palette.sidebar };
        let text = if delete { Color32::WHITE } else { palette.danger };
        if account_row(ui, &palette, None, title, Some(note), RowEnd::Danger(button, fill, text)).clicked() {
            app.actions
                .push(Action::Admin(crate::admin::AdminAction::Open(
                    crate::admin::Modal::CloseAccount { delete, password: String::new() },
                )));
        }
    }
}

/// The account page's parts, which "Conta"'s sub-items in the side menu
/// scroll to.
pub const ACCOUNT_PARTS: [&str; 4] = [
    "Informações da conta",
    "Senha e segurança",
    "Status da conta",
    "Central da Família",
];

fn account_jump() -> egui::Id {
    egui::Id::new("account-jump")
}

/// Scrolls here when the side menu asked for part `index`.
fn account_anchor(ui: &mut Ui, index: usize) {
    if ui.data(|d| d.get_temp::<usize>(account_jump())) == Some(index) {
        ui.data_mut(|d| d.remove::<usize>(account_jump()));
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }
}

fn big_heading(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .font(font(Weight::SemiBold, 22.0))
            .color(palette.strong),
    );
    ui.add_space(8.0);
}

/// What sits at the right of an account row.
enum RowEnd<'a> {
    Button(&'a str),
    /// A value with a chevron: the row opens more.
    Value(&'a str),
    Text(&'a str),
    Danger(&'a str, Color32, Color32),
}

/// One row of the account page, as Discord's: an optional icon tile, the
/// title and its note, and the value or button at the right.
fn account_row(
    ui: &mut Ui,
    palette: &Palette,
    icon: Option<(Icon, Color32, Color32)>,
    title: &str,
    note: Option<&str>,
    end: RowEnd,
) -> egui::Response {
    let width = ui.available_width();
    let left = if icon.is_some() { 66.0 } else { 0.0 };
    let right = 150.0;
    let note_galley = note.map(|n| {
        let mut job = egui::text::LayoutJob::single_section(
            n.into(),
            egui::TextFormat { font_id: font(Weight::Regular, 14.0), color: palette.muted, ..Default::default() },
        );
        job.wrap.max_width = width - left - right;
        ui.painter().layout_job(job)
    });
    let height = 24.0 + note_galley.as_ref().map_or(0.0, |g| g.size().y + 4.0) + 24.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height.max(56.0)), Sense::click());
    let y = rect.center().y;
    if let Some((icon, tint, fill)) = icon {
        let tile = Rect::from_min_size(pos2(rect.left(), y - 26.0), Vec2::splat(52.0));
        ui.painter().rect_filled(tile, CornerRadius::same(8), fill);
        paint_icon(ui, icon, tile.center(), 24.0, tint);
    }
    let top = match &note_galley {
        Some(g) => y - (20.0 + 4.0 + g.size().y) / 2.0,
        None => y - 10.0,
    };
    ui.painter().text(
        pos2(rect.left() + left, top + 10.0),
        Align2::LEFT_CENTER,
        title,
        font(Weight::Medium, 17.0),
        palette.strong,
    );
    if let Some(g) = note_galley {
        ui.painter().galley(pos2(rect.left() + left, top + 24.0), g, palette.muted);
    }
    let hovered = response.hovered();
    match end {
        RowEnd::Button(label) | RowEnd::Danger(label, ..) => {
            let (fill, text) = match end {
                RowEnd::Danger(_, fill, text) => (fill, text),
                _ => (palette.sidebar, palette.strong),
            };
            let galley = ui.painter().layout_no_wrap(label.into(), font(Weight::Medium, 15.0), text);
            let button = Rect::from_min_size(
                pos2(rect.right() - galley.size().x - 36.0, y - 20.0),
                vec2(galley.size().x + 36.0, 40.0),
            );
            let fill = if hovered { crate::theme::mix(fill, Color32::WHITE, 0.06) } else { fill };
            ui.painter().rect(button, CornerRadius::same(8), fill, Stroke::new(1.0, palette.selected), egui::StrokeKind::Inside);
            ui.painter().galley(button.center() - galley.size() / 2.0, galley, text);
        }
        RowEnd::Value(value) => {
            paint_icon(ui, Icon::ChevronRight, pos2(rect.right() - 10.0, y), 20.0, if hovered { palette.strong } else { palette.icon });
            ui.painter().text(pos2(rect.right() - 30.0, y), Align2::RIGHT_CENTER, value, font(Weight::Medium, 16.0), palette.strong);
        }
        RowEnd::Text(value) => {
            ui.painter().text(pos2(rect.right(), y), Align2::RIGHT_CENTER, value, font(Weight::Medium, 16.0), palette.strong);
        }
    }
    response
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
            RichText::new("Com \"Omarchy\" o Discofast segue o tema do desktop e muda junto quando você troca de tema.")
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
    let spellcheck = app.settings.spellcheck;
    switch_row(
        app,
        ui,
        Flag::SpellCheck,
        "Verificar ortografia",
        "Sublinha palavras com erro no campo de mensagem (português e inglês). Clique com o botão direito para ver sugestões.",
        spellcheck,
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

/// The call's voice filter and the app's sounds, as Discord's "Voice &
/// Video" page.
/// "Voz e vídeo" as Discord lays it out: the devices and their volumes,
/// the microphone test, the input profile, and the filter.
fn voice(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, &palette, "Voz e vídeo");
    let devices = devices(app, ui.ctx());
    // The live level, from the call or the microphone test.
    let level = app
        .mic_test
        .as_ref()
        .map(|t| &t.shared)
        .or(app.call.as_ref().map(|c| &c.shared))
        .map(|s| s.level());
    if level.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
    let s = &app.settings;
    let (input, output) = (s.input_device.clone(), s.output_device.clone());
    let (input_volume, output_volume) = (s.input_volume, s.output_volume);
    let (profile, sensitivity, auto) = (s.input_profile, s.sensitivity, s.auto_sensitivity);
    let (noise, echo, gain, sounds) = (
        s.noise_suppression,
        s.echo_cancellation,
        s.auto_gain,
        s.sounds,
    );
    let width = ui.available_width();
    let half = (width - 16.0) / 2.0;
    section(ui, &palette, "Voz");
    ui.add_space(4.0);
    // Microfone | Alto-falante
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
        ui.vertical(|ui| {
            ui.set_width(half);
            label(ui, &palette, "Microfone");
            if let Some(chosen) = device_picker(
                ui,
                &palette,
                Icon::Mic,
                &devices.inputs,
                devices.default_input.as_deref(),
                input.as_deref(),
            ) {
                app.actions.push(Action::SetInputDevice(chosen));
            }
        });
        ui.vertical(|ui| {
            ui.set_width(half);
            label(ui, &palette, "Alto-falante");
            if let Some(chosen) = device_picker(
                ui,
                &palette,
                Icon::Headphones,
                &devices.outputs,
                devices.default_output.as_deref(),
                output.as_deref(),
            ) {
                app.actions.push(Action::SetOutputDevice(chosen));
            }
        });
    });
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
        ui.vertical(|ui| {
            ui.set_width(half);
            label(ui, &palette, "Volume do Microfone");
            if let Some(v) = percent_slider(ui, &palette, input_volume, half) {
                app.actions.push(Action::SetInputVolume(v));
            }
        });
        ui.vertical(|ui| {
            ui.set_width(half);
            label(ui, &palette, "Volume do Alto-falante");
            if let Some(v) = percent_slider(ui, &palette, output_volume, half) {
                app.actions.push(Action::SetOutputVolume(v));
            }
        });
    });
    ui.add_space(16.0);
    // Teste do microfone and its meter.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
        let testing = app.mic_test.is_some();
        let in_call = app.call.is_some();
        let text = if testing {
            "Parar teste"
        } else {
            "Teste do microfone"
        };
        let button = egui::Button::new(
            RichText::new(text)
                .font(font(Weight::Medium, 14.0))
                .color(Color32::WHITE),
        )
        .fill(palette.accent)
        .corner_radius(8)
        .min_size(vec2(178.0, 40.0));
        let response = ui.add_enabled(!in_call, button);
        let response = if in_call {
            response.on_disabled_hover_text("Indisponível durante uma chamada.")
        } else {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        };
        if response.clicked() {
            app.actions.push(Action::ToggleMicTest);
        }
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        meter(ui, &palette, rect, level);
    });
    divider(ui, &palette);
    // Perfil de entrada
    label(ui, &palette, "Perfil de entrada");
    ui.add_space(4.0);
    for (value, name, note) in [
        (
            0,
            "Isolamento de Voz",
            "Só a sua linda voz: deixe o Discofast equalizar o ruído",
        ),
        (
            1,
            "Estúdio",
            "Áudio puro: microfone aberto e sem processamento",
        ),
        (
            2,
            "Personalizado",
            "Modo avançado: me dê todos os botões e mostradores!",
        ),
    ] {
        if radio(ui, &palette, profile == value, name, note) {
            app.actions.push(Action::SetInputProfile(value));
        }
    }
    if profile != 2 {
        ui.add_space(12.0);
        switch_row(app, ui, Flag::Sounds, "Sons do app", SOUNDS_NOTE, sounds);
        return;
    }
    ui.add_space(12.0);
    switch_row(
        app,
        ui,
        Flag::AutoSensitivity,
        "Ajustar Automaticamente a Sensibilidade de Entrada",
        "Controla quanto som o Discofast transmite do seu microfone.",
        auto,
    );
    // The divider of the switch row sits under it; the bar goes above.
    if let Some(db) = sensitivity_bar(ui, &palette, auto, sensitivity, level) {
        app.actions.push(Action::SetSensitivity(db));
    }
    ui.add_space(20.0);
    // Supressão de ruído, with its choice on the right.
    let row_width = ui.available_width();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(row_width - 150.0);
            ui.label(
                RichText::new("Supressão de ruído")
                    .font(font(Weight::Medium, 16.0))
                    .color(palette.strong),
            );
            ui.label(
                RichText::new("Reduz o ruído de fundo do seu microfone.")
                    .font(font(Weight::Regular, 14.0))
                    .color(palette.muted),
            );
        });
        // As Discord's Krisp, Standard and None: the neural one first.
        let names = ["Nenhuma", "Padrão", "Inteligente"];
        let (rect, response) = ui.allocate_exact_size(vec2(130.0, 40.0), Sense::click());
        picker_box(
            ui,
            &palette,
            rect,
            &response,
            None,
            names[usize::from(noise.min(2))],
        );
        egui::Popup::menu(&response).width(130.0).show(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
            for (level, name) in names.iter().enumerate().rev() {
                if super::call::menu_item(ui, &palette, name, palette.text).clicked() {
                    app.actions.push(Action::SetNoiseSuppression(level as u8));
                    ui.close();
                }
            }
        });
    });
    divider(ui, &palette);
    switch_row(
        app,
        ui,
        Flag::EchoCancellation,
        "Cancelamento de eco",
        "Evita que os outros se ouçam de volta quando você usa caixas de som.",
        echo,
    );
    switch_row(
        app,
        ui,
        Flag::AutoGain,
        "Controle automático de ganho",
        "Ajusta o volume da sua voz para os outros te ouvirem sempre por igual.",
        gain,
    );
    switch_row(app, ui, Flag::Sounds, "Sons do app", SOUNDS_NOTE, sounds);
}

const SOUNDS_NOTE: &str =
    "Entrar e sair de canais de voz, silenciar, ensurdecer e mensagens novas.";

/// A page's section title, as Discord's "Voz".
fn section(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .font(font(Weight::SemiBold, 24.0))
            .color(palette.strong),
    );
    ui.add_space(8.0);
}

/// The name over a control.
fn label(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.label(
        RichText::new(text)
            .font(font(Weight::Medium, 16.0))
            .color(palette.strong),
    );
    ui.add_space(6.0);
}

/// Discord's select box: an icon, the choice, the arrow.
fn picker_box(
    ui: &Ui,
    palette: &Palette,
    rect: Rect,
    response: &egui::Response,
    icon: Option<Icon>,
    text: &str,
) {
    let edge = if response.hovered() {
        palette.muted
    } else {
        palette.divider
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        palette.rail,
        Stroke::new(1.0, edge),
        egui::StrokeKind::Inside,
    );
    let mut left = rect.left() + 12.0;
    if let Some(icon) = icon {
        paint_icon(
            ui,
            icon,
            pos2(left + 9.0, rect.center().y),
            18.0,
            palette.text,
        );
        left += 28.0;
    }
    super::widgets::Elide::new(ui, rect.right() - 34.0).text(
        pos2(left, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font(Weight::Medium, 16.0),
        palette.strong,
    );
    paint_icon(
        ui,
        Icon::ChevronDown,
        pos2(rect.right() - 18.0, rect.center().y),
        18.0,
        palette.text,
    );
}

/// The microphones and speakers, listed once in the background.
pub(super) fn devices(app: &App, ctx: &egui::Context) -> crate::devices::Devices {
    let mut slot = app.devices.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        *slot = Some(Default::default());
        let shared = std::sync::Arc::clone(&app.devices);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let list = crate::devices::list();
            *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(list);
            ctx.request_repaint();
        });
    }
    slot.clone().unwrap_or_default()
}

/// A device select; Some(choice) when one is picked, None inside for the
/// system's default.
fn device_picker(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    list: &[crate::devices::Device],
    default: Option<&str>,
    chosen: Option<&str>,
) -> Option<Option<String>> {
    let default_label = default
        .and_then(|d| crate::devices::Devices::label(list, d))
        .map_or_else(
            || "Padrão do sistema".to_owned(),
            |name| format!("Padrão do sistema: {name}"),
        );
    let shown = chosen
        .and_then(|c| crate::devices::Devices::label(list, c))
        .or_else(|| chosen.map(str::to_owned))
        .unwrap_or_else(|| default_label.clone());
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(&shown);
    picker_box(ui, palette, rect, &response, Some(icon), &shown);
    let mut picked = None;
    egui::Popup::menu(&response).width(rect.width()).show(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        if super::call::menu_item(ui, palette, &default_label, palette.text).clicked() {
            picked = Some(None);
            ui.close();
        }
        for device in list {
            let color = if chosen == Some(device.name.as_str()) {
                palette.strong
            } else {
                palette.text
            };
            if super::call::menu_item(ui, palette, &device.label, color).clicked() {
                picked = Some(Some(device.name.clone()));
                ui.close();
            }
        }
        if list.is_empty() {
            ui.label(
                RichText::new("Nenhum outro dispositivo encontrado.")
                    .font(font(Weight::Regular, 14.0))
                    .color(palette.muted),
            );
        }
    });
    picked
}

/// Discord's slider, blue up to a white knob, for 0–200%; the new value
/// while it is dragged.
pub(super) fn percent_slider(ui: &mut Ui, palette: &Palette, value: u16, width: f32) -> Option<u16> {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click_and_drag());
    let track = Rect::from_min_max(
        pos2(rect.left() + 2.0, rect.center().y - 2.0),
        pos2(rect.right() - 10.0, rect.center().y + 2.0),
    );
    let mut t = f32::from(value.min(200)) / 200.0;
    let mut changed = None;
    if let Some(pos) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
        let v = (t * 200.0).round() as u16;
        if v != value {
            changed = Some(v);
        }
    }
    let knob = pos2(track.left() + t * track.width(), track.center().y);
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), palette.divider);
    ui.painter().rect_filled(
        Rect::from_min_max(track.min, pos2(knob.x, track.max.y)),
        CornerRadius::same(2),
        palette.accent,
    );
    ui.painter().circle_filled(knob, 10.0, Color32::WHITE);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() || response.dragged() {
        response.on_hover_text_at_pointer(format!("{}%", (t * 200.0).round()));
    }
    changed
}

/// The microphone's level as Discord's row of bars.
fn meter(ui: &Ui, palette: &Palette, rect: Rect, level: Option<f32>) {
    let bars = ((rect.width() + 4.0) / 8.0).floor().max(1.0) as usize;
    // -80 dB to 0 dB across the row.
    let lit = level.map_or(0.0, |db| ((db + 80.0) / 80.0).clamp(0.0, 1.0));
    for i in 0..bars {
        let x = rect.left() + i as f32 * 8.0;
        let bar = Rect::from_min_size(pos2(x, rect.top() + 6.0), vec2(4.0, rect.height() - 12.0));
        let on = (i as f32 + 0.5) / bars as f32 <= lit;
        let color = if on { palette.online } else { palette.divider };
        ui.painter().rect_filled(bar, CornerRadius::same(2), color);
    }
}

/// Discord's input sensitivity: yellow below the threshold, green above,
/// the live level brighter; with automatic sensitivity only the level.
/// The new threshold while it is dragged.
fn sensitivity_bar(
    ui: &mut Ui,
    palette: &Palette,
    auto: bool,
    threshold: f32,
    level: Option<f32>,
) -> Option<f32> {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, 28.0),
        if auto {
            Sense::hover()
        } else {
            Sense::click_and_drag()
        },
    );
    let track = Rect::from_min_max(
        pos2(rect.left() + 2.0, rect.center().y - 2.0),
        pos2(rect.right() - 10.0, rect.center().y + 2.0),
    );
    let at = |db: f32| track.left() + ((db + 100.0) / 100.0).clamp(0.0, 1.0) * track.width();
    let mut value = threshold;
    let mut changed = None;
    if !auto
        && let Some(pos) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        value = (((pos.x - track.left()) / track.width()).clamp(0.0, 1.0) * 100.0 - 100.0).round();
        if (value - threshold).abs() >= 1.0 {
            changed = Some(value);
        }
    }
    let yellow = Color32::from_rgb(0xf0, 0xb2, 0x32);
    let dim = |c: Color32| crate::theme::mix(c, palette.chat, 0.55);
    let split = if auto { track.left() } else { at(value) };
    let live = level.map(at);
    if auto {
        ui.painter()
            .rect_filled(track, CornerRadius::same(2), palette.divider);
        if let Some(x) = live {
            ui.painter().rect_filled(
                Rect::from_min_max(track.min, pos2(x, track.max.y)),
                CornerRadius::same(2),
                palette.online,
            );
        }
        return None;
    }
    let left = Rect::from_min_max(track.min, pos2(split, track.max.y));
    let right = Rect::from_min_max(pos2(split, track.min.y), track.max);
    ui.painter()
        .rect_filled(left, CornerRadius::same(2), dim(yellow));
    ui.painter()
        .rect_filled(right, CornerRadius::same(2), dim(palette.online));
    if let Some(x) = live {
        let lit_left = Rect::from_min_max(track.min, pos2(x.min(split), track.max.y));
        ui.painter()
            .rect_filled(lit_left, CornerRadius::same(2), yellow);
        if x > split {
            ui.painter().rect_filled(
                Rect::from_min_max(pos2(split, track.min.y), pos2(x, track.max.y)),
                CornerRadius::same(2),
                palette.online,
            );
        }
    }
    ui.painter()
        .circle_filled(pos2(split, track.center().y), 10.0, Color32::WHITE);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.hovered() || response.dragged() {
        response.on_hover_text_at_pointer(format!("{value:.0} dB"));
    }
    changed
}

/// Discord's round choice with a note under it; true when clicked.
fn radio(ui: &mut Ui, palette: &Palette, on: bool, name: &str, note: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    let dot = pos2(rect.left() + 12.0, rect.top() + 18.0);
    if on {
        ui.painter().circle_filled(dot, 11.0, palette.accent);
        ui.painter().circle_filled(dot, 4.5, Color32::WHITE);
    } else {
        let edge = if response.hovered() {
            palette.strong
        } else {
            palette.muted
        };
        ui.painter()
            .circle_stroke(dot, 10.0, Stroke::new(2.0, edge));
    }
    ui.painter().text(
        pos2(rect.left() + 36.0, dot.y),
        Align2::LEFT_CENTER,
        name,
        font(Weight::Medium, 16.0),
        palette.strong,
    );
    ui.painter().text(
        pos2(rect.left() + 36.0, dot.y + 22.0),
        Align2::LEFT_CENTER,
        note,
        font(Weight::Regular, 14.0),
        palette.muted,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
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
        "Fechar a janela deixa o Discofast rodando na bandeja do sistema.",
        tray,
    );
    match app.autostart {
        Some(on) => switch_row(
            app,
            ui,
            Flag::StartAtLogin,
            "Abrir o Discofast ao iniciar a sessão",
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

/// The version, where updating stands, and the buttons for it.
fn updates(app: &mut App, ui: &mut Ui) {
    use crate::updates::Status;
    let palette = app.palette;
    ui.label(
        RichText::new(format!("Discofast {}", env!("CARGO_PKG_VERSION")))
            .font(font(Weight::Medium, 15.0))
            .color(palette.strong),
    );
    let (text, color) = match &app.update {
        Status::Idle => ("Ainda não verificado.".to_owned(), palette.muted),
        Status::Checking => ("Procurando atualizações…".to_owned(), palette.muted),
        Status::UpToDate => (
            "Você está na versão mais recente.".to_owned(),
            palette.online,
        ),
        Status::Unsupported(why) => (
            format!("Esta cópia não se atualiza sozinha: {why}"),
            palette.muted,
        ),
        Status::Downloading(version, share) => (
            format!("Baixando a versão {version}… {:.0}%", share * 100.0),
            palette.text,
        ),
        Status::Ready(version) => (
            format!("A versão {version} está pronta. Reinicie para atualizar."),
            palette.online,
        ),
        Status::Failed(error) => (format!("A atualização falhou: {error}"), palette.danger),
    };
    ui.label(
        RichText::new(text)
            .font(font(Weight::Regular, 14.0))
            .color(color),
    );
    ui.horizontal(|ui| {
        let ready = matches!(app.update, Status::Ready(_));
        let busy = matches!(app.update, Status::Checking | Status::Downloading(..));
        if ready {
            let restart = egui::Button::new(
                RichText::new("Reiniciar para atualizar")
                    .font(font(Weight::Medium, 14.0))
                    .color(Color32::WHITE),
            )
            .fill(palette.online)
            .corner_radius(CornerRadius::same(4))
            .min_size(vec2(180.0, 34.0));
            if ui.add(restart).clicked() {
                app.actions.push(Action::InstallUpdate);
            }
        } else if ui
            .add_enabled(
                !busy,
                egui::Button::new("Verificar atualizações").min_size(vec2(180.0, 34.0)),
            )
            .clicked()
        {
            app.actions.push(Action::CheckUpdate);
        }
    });
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
    heading(ui, &palette, "ATUALIZAÇÕES");
    updates(app, ui);
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
