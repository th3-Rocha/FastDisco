//! The server menu under the server's name, its dialogs, and the server
//! settings over the whole window.

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Shadow, Stroke, Ui, Vec2,
    pos2, vec2,
};
use fastframe_fonts::Weight;

use super::widgets::{font, hyphens, paint_icon};
use crate::admin::{AdminAction, GuildPage, Modal};
use crate::app::{Action, App};
use crate::icons::Icon;
use crate::model::{ListItem, permission};

const LINK: Color32 = Color32::from_rgb(0x00, 0xa8, 0xfc);

fn act(app: &mut App, action: AdminAction) {
    app.actions.push(Action::Admin(action));
}

/// Discord's server menu, below the header, as wide as the channel list.
pub fn menu(app: &mut App, ui: &mut Ui, header: Rect) {
    if !app.admin.menu {
        return;
    }
    let Some(guild) = app.server() else { return };
    let palette = app.palette;
    let can = |bit| app.model.can(guild, bit);
    let mut items: Vec<(&str, Icon, Option<AdminAction>, Color32)> = Vec::new();
    if can(permission::CREATE_INVITE) {
        items.push((
            "Convidar pessoas",
            Icon::UserPlus,
            Some(AdminAction::Open(Modal::Invite { link: None })),
            palette.accent,
        ));
    }
    if can(permission::MANAGE_GUILD)
        || can(permission::MANAGE_ROLES)
        || can(permission::MANAGE_EMOJIS)
        || can(permission::BAN)
        || can(permission::VIEW_AUDIT_LOG)
    {
        items.push((
            "Configurações do servidor",
            Icon::Settings,
            Some(AdminAction::OpenSettings(guild)),
            palette.text,
        ));
    }
    if can(permission::MANAGE_CHANNELS) {
        let create = Modal::CreateChannel {
            guild,
            voice: false,
            name: String::new(),
            category: None,
        };
        items.push((
            "Criar canal",
            Icon::PlusCircle,
            Some(AdminAction::Open(create)),
            palette.text,
        ));
        items.push((
            "Criar categoria",
            Icon::Plus,
            Some(AdminAction::Open(Modal::CreateCategory {
                guild,
                name: String::new(),
            })),
            palette.text,
        ));
    }
    items.push(("", Icon::Close, None, palette.text));
    items.push((
        "Configurações de notificação",
        Icon::Bell,
        Some(AdminAction::Open(Modal::Notify { guild })),
        palette.text,
    ));
    if can(permission::CHANGE_NICKNAME) {
        let me = app.model.me;
        let text = app
            .model
            .guild(guild)
            .and_then(|g| g.members.get(&me))
            .and_then(|m| m.nick.clone())
            .unwrap_or_default();
        items.push((
            "Editar perfil do servidor",
            Icon::Pencil,
            Some(AdminAction::Open(Modal::Nick { guild, text })),
            palette.text,
        ));
    }
    let owner = app
        .model
        .guild(guild)
        .is_some_and(|g| g.owner == app.model.me);
    if !owner {
        items.push(("", Icon::Close, None, palette.text));
        items.push((
            "Sair do servidor",
            Icon::External,
            Some(AdminAction::Open(Modal::Leave { guild })),
            palette.danger,
        ));
    }
    let mut chosen = None;
    let area = egui::Area::new(egui::Id::new("server-menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(header.left() + 10.0, header.bottom() + 4.0))
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(ui.visuals().window_fill)
                .corner_radius(CornerRadius::same(6))
                .shadow(Shadow {
                    offset: [0, 4],
                    blur: 12,
                    spread: 0,
                    color: Color32::from_black_alpha(100),
                })
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(header.width() - 36.0);
                    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                    for (label, icon, action, color) in items {
                        let Some(action) = action else {
                            let (rect, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 9.0),
                                Sense::hover(),
                            );
                            ui.painter().hline(
                                (rect.left() + 4.0)..=(rect.right() - 4.0),
                                rect.center().y,
                                Stroke::new(1.0, palette.divider),
                            );
                            continue;
                        };
                        let (rect, response) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
                        let hovered = response.hovered();
                        let fill = if color == palette.danger {
                            palette.danger
                        } else {
                            palette.accent
                        };
                        if hovered {
                            ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
                        }
                        let tint = if hovered { Color32::WHITE } else { color };
                        ui.painter().text(
                            pos2(rect.left() + 8.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            font(Weight::Medium, 14.0),
                            tint,
                        );
                        paint_icon(
                            ui,
                            icon,
                            pos2(rect.right() - 16.0, rect.center().y),
                            18.0,
                            tint,
                        );
                        if response.clicked() {
                            chosen = Some(action);
                        }
                    }
                });
        });
    if let Some(action) = chosen {
        act(app, action);
    } else if area.response.clicked_elsewhere() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        act(app, AdminAction::CloseMenu);
    }
}

/// The open dialog, if any.
pub fn modal(app: &mut App, ui: &mut Ui) {
    let Some(modal) = app.admin.modal.clone() else {
        return;
    };
    match modal {
        Modal::Invite { link } => return invite(app, ui, link),
        Modal::CreateGuild { .. } | Modal::JoinGuild { .. } => return create_guild(app, ui, modal),
        _ => {}
    }
    let palette = app.palette;
    let busy = app.admin.busy;
    let error = app.admin.error.clone();
    let mut submit = false;
    let mut close = false;
    let mut edited: Option<Modal> = None;
    let (title, button, danger) = match &modal {
        Modal::Invite { .. } | Modal::CreateGuild { .. } | Modal::JoinGuild { .. } => {
            ("Convidar amigos", "Copiar", false)
        }
        Modal::Nick { .. } => ("Alterar apelido", "Salvar", false),
        Modal::VoiceStatus { .. } => ("Definir status do canal", "Salvar", false),
        Modal::StartStage { .. } => ("Iniciar palco", "Iniciar palco", false),
        Modal::Notify { .. } => ("Configurações de notificação", "Concluído", false),
        Modal::Sessions => ("Dispositivos conectados", "Fechar", false),
        Modal::CloseAccount { delete: false, .. } => ("Desativar conta", "Desativar conta", true),
        Modal::CloseAccount { delete: true, .. } => ("Excluir conta", "Excluir conta", true),
        Modal::CustomStatus { .. } => ("Definir um status personalizado", "Salvar", false),
        Modal::CreateChannel { .. } => ("Criar canal", "Criar canal", false),
        Modal::CreateCategory { .. } => ("Criar categoria", "Criar categoria", false),
        Modal::Leave { .. } => ("Sair do servidor", "Sair do servidor", true),
        Modal::Kick { .. } => ("Expulsar membro", "Expulsar", true),
        Modal::Ban { .. } => ("Banir membro", "Banir", true),
        Modal::Timeout { .. } => ("Castigar membro", "Castigar", true),
        Modal::DeleteGuild { .. } => ("Excluir servidor", "Excluir servidor", true),
    };
    let response = egui::Modal::new(egui::Id::new("admin-modal"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(Frame::new().fill(palette.chat).corner_radius(CornerRadius::same(8)))
        .show(ui.ctx(), |ui| {
            ui.set_width(440.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                ui.label(RichText::new(title).font(font(Weight::SemiBold, 20.0)).color(palette.strong));
                ui.add_space(8.0);
                let field = |ui: &mut Ui, label: &str, text: &mut String, hint: &str| -> bool {
                    ui.label(RichText::new(label).font(font(Weight::Bold, 12.0)).color(palette.muted));
                    let inner = Frame::new().fill(palette.rail).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(10)).show(ui, |ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(text)
                                .frame(Frame::NONE)
                                .hint_text(RichText::new(hint).color(palette.muted))
                                .font(font(Weight::Regular, 15.0))
                                .text_color(palette.text)
                                .desired_width(f32::INFINITY),
                        );
                        if ui.memory(|m| m.focused().is_none()) {
                            r.request_focus();
                        }
                        r
                    });
                    inner.inner.changed()
                };
                match modal.clone() {
                    Modal::CreateGuild { .. } | Modal::JoinGuild { .. } => {}
                    Modal::Invite { link } => {
                        ui.label(RichText::new("Envie este link para seus amigos. Ele expira em 7 dias.").color(palette.muted));
                        let text = link.clone().unwrap_or_else(|| "Criando convite…".into());
                        Frame::new().fill(palette.rail).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(text).font(font(Weight::Medium, 15.0)).color(palette.strong));
                        });
                    }
                    Modal::Sessions => {
                        ui.label(RichText::new("Estes são os dispositivos conectados à sua conta. Saia de qualquer um que você não reconheça.").color(palette.muted));
                        match app.admin.sessions.clone() {
                            None => {
                                ui.spinner();
                            }
                            Some(list) => {
                                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                                    for session in list {
                                        Frame::new().fill(palette.sidebar).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(12)).show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.horizontal(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(RichText::new(&session.name).font(font(Weight::SemiBold, 15.0)).color(palette.strong));
                                                    let when = session.last_used.get(..10).unwrap_or_default();
                                                    let note = [session.location.as_str(), when].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ");
                                                    ui.label(RichText::new(note).font(font(Weight::Regular, 13.0)).color(palette.muted));
                                                });
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    let out = egui::Button::new(RichText::new("Sair").color(palette.danger)).corner_radius(CornerRadius::same(6));
                                                    if ui.add(out).clicked() {
                                                        act(app, AdminAction::SignOutSession(session.hash.clone()));
                                                    }
                                                });
                                            });
                                        });
                                        ui.add_space(6.0);
                                    }
                                });
                            }
                        }
                    }
                    Modal::CloseAccount { delete, mut password } => {
                        let note = if delete {
                            "Excluir a conta é permanente: suas mensagens ficam, mas a conta não pode ser recuperada depois do prazo do Discord."
                        } else {
                            "Desativar a conta a esconde até você entrar de novo. Você será desconectado."
                        };
                        ui.label(RichText::new(note).color(palette.text));
                        ui.label(RichText::new("SENHA").font(font(Weight::Bold, 12.0)).color(palette.muted));
                        let inner = Frame::new().fill(palette.rail).corner_radius(CornerRadius::same(4)).inner_margin(Margin::same(10)).show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut password)
                                    .password(true)
                                    .frame(Frame::NONE)
                                    .font(font(Weight::Regular, 15.0))
                                    .text_color(palette.text)
                                    .desired_width(f32::INFINITY),
                            )
                        });
                        if inner.inner.changed() {
                            edited = Some(Modal::CloseAccount { delete, password });
                        }
                    }
                    Modal::CustomStatus { mut text } => {
                        if field(ui, "NO QUE VOCÊ ESTÁ PENSANDO?", &mut text, "Defina um status personalizado") {
                            edited = Some(Modal::CustomStatus { text });
                        }
                        ui.label(RichText::new("Deixe vazio para apagá-lo.").font(font(Weight::Regular, 12.0)).color(palette.muted));
                    }
                    Modal::VoiceStatus { channel, mut text } => {
                        if field(ui, "STATUS", &mut text, "Do que vocês estão falando?") {
                            edited = Some(Modal::VoiceStatus { channel, text });
                        }
                        ui.label(RichText::new("Todos no canal de voz veem este status. Deixe vazio para apagá-lo.").font(font(Weight::Regular, 12.0)).color(palette.muted));
                    }
                    Modal::Notify { guild } => {
                        let mut out = Vec::new();
                        super::manage::notify_menu(app, ui, Some(guild), None, &mut out);
                        app.actions.extend(out);
                    }
                    Modal::StartStage { channel, mut topic } => {
                        if field(ui, "TEMA", &mut topic, "Do que vamos falar?") {
                            edited = Some(Modal::StartStage { channel, topic });
                        }
                        ui.label(RichText::new("Quem estiver no servidor vê o tema e pode entrar para ouvir.").font(font(Weight::Regular, 12.0)).color(palette.muted));
                    }
                    Modal::Nick { guild, mut text } => {
                        if field(ui, "APELIDO", &mut text, &app.model.user(app.model.me).map(|u| u.name.clone()).unwrap_or_default()) {
                            edited = Some(Modal::Nick { guild, text });
                        }
                        ui.label(RichText::new("Deixe vazio para voltar ao nome exibido.").font(font(Weight::Regular, 12.0)).color(palette.muted));
                    }
                    Modal::CreateChannel { guild, voice, mut name, category } => {
                        ui.label(RichText::new("TIPO DE CANAL").font(font(Weight::Bold, 12.0)).color(palette.muted));
                        for (is_voice, icon, label, note) in [
                            (false, Icon::Hash, "Texto", "Envie mensagens, imagens, GIFs, emoji, opiniões e piadas"),
                            (true, Icon::Speaker, "Voz", "Converse com voz, vídeo e compartilhamento de tela"),
                        ] {
                            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
                            let selected = voice == is_voice;
                            ui.painter().rect_filled(rect, CornerRadius::same(4), if selected { palette.selected } else { palette.sidebar });
                            paint_icon(ui, icon, pos2(rect.left() + 22.0, rect.center().y), 22.0, palette.text);
                            ui.painter().text(pos2(rect.left() + 48.0, rect.center().y - 8.0), Align2::LEFT_CENTER, label, font(Weight::Medium, 15.0), palette.strong);
                            ui.painter().text(pos2(rect.left() + 48.0, rect.center().y + 10.0), Align2::LEFT_CENTER, note, font(Weight::Regular, 12.0), palette.muted);
                            ui.painter().circle_stroke(pos2(rect.right() - 20.0, rect.center().y), 9.0, Stroke::new(2.0, palette.text));
                            if selected {
                                ui.painter().circle_filled(pos2(rect.right() - 20.0, rect.center().y), 5.0, palette.text);
                            }
                            if response.clicked() {
                                edited = Some(Modal::CreateChannel { guild, voice: is_voice, name: name.clone(), category });
                            }
                        }
                        if field(ui, "NOME DO CANAL", &mut name, if voice { "Novo canal" } else { "novo-canal" }) {
                            edited = Some(Modal::CreateChannel { guild, voice, name, category });
                        }
                    }
                    Modal::CreateCategory { guild, mut name } => {
                        if field(ui, "NOME DA CATEGORIA", &mut name, "Nova categoria") {
                            edited = Some(Modal::CreateCategory { guild, name });
                        }
                    }
                    Modal::Leave { guild } => {
                        let name = app.model.guild(guild).map(|g| g.name.clone()).unwrap_or_default();
                        ui.label(RichText::new(format!("Tem certeza que deseja sair de {name}? Você não poderá voltar a menos que seja convidado de novo.")).color(palette.text));
                    }
                    Modal::Kick { guild, user } => {
                        let name = app.model.name_in(Some(guild), user);
                        ui.label(RichText::new(format!("Tem certeza que deseja expulsar {name} do servidor?")).color(palette.text));
                    }
                    Modal::Ban { guild, user, mut history } => {
                        let name = app.model.name_in(Some(guild), user);
                        ui.label(RichText::new(format!("Tem certeza que deseja banir {name} do servidor?")).color(palette.text));
                        ui.label(RichText::new("EXCLUIR HISTÓRICO DE MENSAGENS").font(font(Weight::Bold, 12.0)).color(palette.muted));
                        let choices = [
                            (0, "Não excluir nada"),
                            (3_600, "Última hora"),
                            (21_600, "Últimas 6 horas"),
                            (43_200, "Últimas 12 horas"),
                            (86_400, "Últimas 24 horas"),
                            (259_200, "Últimos 3 dias"),
                            (604_800, "Últimos 7 dias"),
                        ];
                        let before = history;
                        egui::ComboBox::from_id_salt("ban-history")
                            .selected_text(choices.iter().find(|c| c.0 == history).map_or("", |c| c.1))
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for (seconds, label) in choices {
                                    ui.selectable_value(&mut history, seconds, label);
                                }
                            });
                        if history != before {
                            edited = Some(Modal::Ban { guild, user, history });
                        }
                    }
                    Modal::Timeout { guild, user, mut minutes } => {
                        let name = app.model.name_in(Some(guild), user);
                        ui.label(RichText::new(format!("{name} não poderá enviar mensagens, reagir nem falar na voz durante o castigo.")).color(palette.text));
                        ui.label(RichText::new("DURAÇÃO").font(font(Weight::Bold, 12.0)).color(palette.muted));
                        let before = minutes;
                        ui.horizontal_wrapped(|ui| {
                            for (value, label) in [
                                (1, "60 segundos"),
                                (5, "5 minutos"),
                                (10, "10 minutos"),
                                (60, "1 hora"),
                                (1_440, "1 dia"),
                                (10_080, "1 semana"),
                                (0, "Remover castigo"),
                            ] {
                                ui.selectable_value(&mut minutes, value, label);
                            }
                        });
                        if minutes != before {
                            edited = Some(Modal::Timeout { guild, user, minutes });
                        }
                    }
                    Modal::DeleteGuild { guild, mut typed } => {
                        let name = app.model.guild(guild).map(|g| g.name.clone()).unwrap_or_default();
                        ui.label(RichText::new(format!("Excluir {name} não pode ser desfeito.")).color(palette.text));
                        if field(ui, "DIGITE O NOME DO SERVIDOR", &mut typed, &name) {
                            edited = Some(Modal::DeleteGuild { guild, typed });
                        }
                    }
                }
                if let Some(error) = &error {
                    ui.label(RichText::new(error).color(palette.danger));
                }
            });
            Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 })
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 0.0);
                        let fill = if danger { palette.danger } else { palette.accent };
                        let label = if busy { "Aguarde…" } else { button };
                        let main = egui::Button::new(RichText::new(label).font(font(Weight::Medium, 14.0)).color(Color32::WHITE))
                            .fill(fill)
                            .corner_radius(CornerRadius::same(3))
                            .min_size(vec2(110.0, 38.0));
                        if ui.add_enabled(!busy, main).clicked() {
                            submit = true;
                        }
                        let cancel = egui::Button::new(RichText::new("Cancelar").font(font(Weight::Medium, 14.0)).color(palette.strong)).frame(false).min_size(vec2(90.0, 38.0));
                        if ui.add(cancel).clicked() {
                            close = true;
                        }
                    });
                });
        });
    if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !busy {
        submit = true;
    }
    if response.should_close() {
        close = true;
    }
    if let Some(modal) = edited {
        app.admin.modal = Some(modal);
    }
    if close {
        act(app, AdminAction::CloseModal);
    } else if submit {
        act(app, AdminAction::Submit);
    }
}

/// The server settings over the whole window.
pub fn settings(app: &mut App, ui: &mut Ui) {
    let Some(settings) = &app.admin.settings else {
        return;
    };
    let guild = settings.guild;
    let page = settings.page.unwrap_or(GuildPage::Overview);
    let palette = app.palette;
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        act(app, AdminAction::CloseSettings);
    }
    let width = ui.available_width();
    let left = ((width - 218.0 - 740.0 - 120.0) / 2.0).max(0.0) + 238.0;
    let name = app
        .model
        .guild(guild)
        .map(|g| g.name.clone())
        .unwrap_or_default();
    egui::Panel::left("guild-settings-nav")
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
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                ui.allocate_ui(vec2(218.0, ui.available_height()), |ui| {
                    ui.set_width(218.0);
                    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                    ui.label(
                        RichText::new(hyphens(&name.to_uppercase()))
                            .font(font(Weight::Bold, 12.0))
                            .color(palette.muted),
                    );
                    let can = |bit| app.model.can(guild, bit);
                    let pages = [
                        (
                            GuildPage::Overview,
                            "Visão geral",
                            can(permission::MANAGE_GUILD),
                        ),
                        (GuildPage::Roles, "Cargos", can(permission::MANAGE_ROLES)),
                        (GuildPage::Emoji, "Emoji", can(permission::MANAGE_EMOJIS)),
                        (GuildPage::Stickers, "Figurinhas", can(permission::MANAGE_EMOJIS)),
                        (GuildPage::Members, "Membros", true),
                        (
                            GuildPage::Invites,
                            "Convites",
                            can(permission::MANAGE_GUILD),
                        ),
                        (GuildPage::Bans, "Banimentos", can(permission::BAN)),
                        (GuildPage::AutoMod, "AutoMod", can(permission::MANAGE_GUILD)),
                        (
                            GuildPage::Audit,
                            "Registro de auditoria",
                            can(permission::VIEW_AUDIT_LOG),
                        ),
                    ];
                    let channel_label = app
                        .manage
                        .channel
                        .and_then(|c| app.model.channel(c))
                        .map(|c| format!("# {}", c.name));
                    let channel_page = (GuildPage::Channel, channel_label.as_deref().unwrap_or(""), channel_label.is_some());
                    let mut chosen = None;
                    for (target, label, allowed) in pages.into_iter().chain([channel_page]) {
                        if !allowed {
                            continue;
                        }
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(218.0, 32.0), Sense::click());
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
                        } else {
                            palette.muted
                        };
                        ui.painter().text(
                            pos2(rect.left() + 10.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            font(Weight::Medium, 16.0),
                            color,
                        );
                        if response.clicked() {
                            chosen = Some(target);
                        }
                    }
                    if app
                        .model
                        .guild(guild)
                        .is_some_and(|g| g.owner == app.model.me)
                    {
                        ui.add_space(8.0);
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(218.0, 32.0), Sense::click());
                        if response.hovered() {
                            ui.painter()
                                .rect_filled(rect, CornerRadius::same(4), palette.hover);
                        }
                        ui.painter().text(
                            pos2(rect.left() + 10.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            "Excluir servidor",
                            font(Weight::Medium, 16.0),
                            palette.danger,
                        );
                        if response.clicked() {
                            act(
                                app,
                                AdminAction::Open(Modal::DeleteGuild {
                                    guild,
                                    typed: String::new(),
                                }),
                            );
                        }
                    }
                    if let Some(target) = chosen {
                        act(app, AdminAction::Page(target));
                    }
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
            let width = 740.0_f32.min(ui.available_width() - 70.0).max(300.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(width);
                    ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
                    match page {
                        GuildPage::Overview => overview(app, ui),
                        GuildPage::Roles => roles(app, ui),
                        GuildPage::Emoji => emoji(app, ui),
                        GuildPage::Members => members(app, ui),
                        GuildPage::Invites => invites(app, ui),
                        GuildPage::Bans => bans(app, ui),
                        GuildPage::Audit => audit(app, ui),
                        GuildPage::Stickers => super::manage::stickers(app, ui, guild),
                        GuildPage::AutoMod => super::manage::automod(app, ui),
                        GuildPage::Channel => super::manage::channel(app, ui, guild),
                    }
                    if let Some(message) =
                        app.admin.settings.as_ref().and_then(|s| s.message.clone())
                    {
                        let color = if message.starts_with("Alterações") {
                            palette.online
                        } else {
                            palette.danger
                        };
                        ui.label(RichText::new(message).color(color));
                    }
                });
            // The round close button with ESC, as in the user settings.
            let center = pos2(
                (full.left() + 740.0 + 60.0).min(full.right() + 10.0),
                full.top() + 18.0,
            );
            let rect = Rect::from_center_size(center, Vec2::splat(36.0));
            let response = ui.interact(rect, ui.id().with("close-guild-settings"), Sense::click());
            let color = if response.hovered() {
                palette.strong
            } else {
                palette.icon
            };
            ui.painter()
                .circle_stroke(center, 17.0, Stroke::new(2.0, color));
            paint_icon(ui, Icon::Close, center, 18.0, color);
            ui.painter().text(
                center + vec2(0.0, 30.0),
                Align2::CENTER_CENTER,
                "ESC",
                font(Weight::SemiBold, 13.0),
                palette.muted,
            );
            if response.clicked() {
                act(app, AdminAction::CloseSettings);
            }
        });
}

fn title(ui: &mut Ui, app: &App, text: &str) {
    ui.label(
        RichText::new(text)
            .font(font(Weight::Bold, 20.0))
            .color(app.palette.strong),
    );
    ui.add_space(12.0);
}

fn save_button(app: &mut App, ui: &mut Ui, action: AdminAction) {
    let button = egui::Button::new(
        RichText::new("Salvar alterações")
            .font(font(Weight::Medium, 14.0))
            .color(Color32::WHITE),
    )
    .fill(app.palette.online)
    .corner_radius(CornerRadius::same(4))
    .min_size(vec2(140.0, 36.0));
    if ui.add(button).clicked() {
        act(app, action);
    }
}

fn overview(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, app, "Visão geral do servidor");
    let Some(settings) = &app.admin.settings else {
        return;
    };
    let guild = settings.guild;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(100.0), Sense::hover());
        match app.picture(app.model.guild(guild).and_then(|g| g.icon.as_deref())) {
            Some(uri) => {
                egui::Image::new(uri)
                    .corner_radius(CornerRadius::same(50))
                    .paint_at(ui, rect);
            }
            None => {
                ui.painter()
                    .circle_filled(rect.center(), 50.0, palette.accent);
            }
        }
        ui.add_space(20.0);
        ui.vertical(|ui| {
            ui.label(
                RichText::new("NOME DO SERVIDOR")
                    .font(font(Weight::Bold, 12.0))
                    .color(palette.muted),
            );
            if let Some(settings) = &mut app.admin.settings {
                Frame::new()
                    .fill(palette.rail)
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut settings.name)
                                .frame(Frame::NONE)
                                .font(font(Weight::Regular, 15.0))
                                .text_color(palette.text)
                                .desired_width(320.0),
                        );
                    });
            }
        });
    });
    ui.add_space(12.0);
    save_button(app, ui, AdminAction::SaveGuild);
    super::manage::pictures(app, ui, guild);
}

fn roles(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, app, "Cargos");
    let Some(settings) = &app.admin.settings else {
        return;
    };
    let guild = settings.guild;
    let selected = settings.role;
    let roles = app
        .model
        .guild(guild)
        .map(|g| g.roles.clone())
        .unwrap_or_default();
    ui.label(RichText::new("Use cargos para agrupar os membros e dar permissões. Clique em um cargo para editar o nome, a cor e as permissões.").color(palette.muted));
    if ui
        .add(
            egui::Button::new(RichText::new("Criar cargo").color(Color32::WHITE))
                .fill(palette.accent)
                .corner_radius(CornerRadius::same(4)),
        )
        .clicked()
    {
        app.actions
            .push(Action::Manage(crate::manage::ManageAction::CreateRole));
    }
    let mut pick = None;
    for role in roles.iter().filter(|r| r.id != guild) {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
        if Some(role.id) == selected {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), palette.selected);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), palette.hover);
        }
        ui.painter().circle_filled(
            pos2(rect.left() + 16.0, rect.center().y),
            7.0,
            role.color.unwrap_or(palette.muted),
        );
        ui.painter().text(
            pos2(rect.left() + 34.0, rect.center().y),
            Align2::LEFT_CENTER,
            hyphens(&role.name),
            font(Weight::Medium, 15.0),
            palette.text,
        );
        if response.clicked() {
            pick = Some(role.id);
        }
    }
    if let Some(role) = pick {
        act(app, AdminAction::SelectRole(role));
    }
    if selected.is_some() {
        ui.add_space(16.0);
        if let Some(settings) = &mut app.admin.settings {
            ui.label(
                RichText::new("NOME DO CARGO")
                    .font(font(Weight::Bold, 12.0))
                    .color(palette.muted),
            );
            Frame::new()
                .fill(palette.rail)
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut settings.role_name)
                            .frame(Frame::NONE)
                            .font(font(Weight::Regular, 15.0))
                            .text_color(palette.text)
                            .desired_width(f32::INFINITY),
                    );
                });
            ui.label(
                RichText::new("COR DO CARGO")
                    .font(font(Weight::Bold, 12.0))
                    .color(palette.muted),
            );
            ui.color_edit_button_srgb(&mut settings.role_color);
        }
        save_button(app, ui, AdminAction::SaveRole);
        if let Some(role) = selected {
            super::manage::role_permissions(app, ui, role);
            save_button(app, ui, AdminAction::SaveRole);
        }
    }
}

fn emoji(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, app, "Emoji");
    let Some(guild) = app.admin.settings.as_ref().map(|s| s.guild) else {
        return;
    };
    let list = app
        .model
        .guild(guild)
        .map(|g| g.emojis.clone())
        .unwrap_or_default();
    ui.label(RichText::new(format!("{} emoji neste servidor.", list.len())).color(palette.muted));
    for e in list {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
            if let Some(uri) = app.picture(Some(&crate::model::CustomEmoji::url(e.id, e.animated)))
            {
                egui::Image::new(uri).paint_at(ui, rect);
            }
            ui.add_space(12.0);
            ui.label(
                RichText::new(format!(":{}:", e.name))
                    .font(font(Weight::Medium, 15.0))
                    .color(palette.text),
            );
        });
    }
    super::manage::emoji_upload(app, ui, guild);
}

fn members(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, app, "Membros");
    let Some(guild) = app.admin.settings.as_ref().map(|s| s.guild) else {
        return;
    };
    let users: Vec<_> = app
        .model
        .guild(guild)
        .map(|g| {
            g.list
                .iter()
                .filter_map(|i| {
                    if let ListItem::Member(u) = i {
                        Some(*u)
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    ui.label(RichText::new("Os membros carregados da lista do servidor.").color(palette.muted));
    let (kick, ban) = (
        app.model.can(guild, permission::KICK),
        app.model.can(guild, permission::BAN),
    );
    let moderate = app.model.can(guild, permission::MODERATE_MEMBERS);
    let mut chosen = None;
    for user in users {
        let Some(u) = app.model.user(user).cloned() else {
            continue;
        };
        ui.horizontal(|ui| {
            ui.set_min_height(44.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
            super::widgets::avatar(ui, app, &u, rect.center(), 16.0, None);
            ui.add_space(8.0);
            ui.vertical(|ui| {
                let color = app
                    .model
                    .name_color(Some(guild), user)
                    .unwrap_or(palette.text);
                ui.label(
                    RichText::new(hyphens(&app.model.name_in(Some(guild), user)))
                        .font(font(Weight::Medium, 15.0))
                        .color(color),
                );
                ui.label(
                    RichText::new(&u.username)
                        .font(font(Weight::Regular, 12.0))
                        .color(palette.muted),
                );
            });
            if user != app.model.me {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ban
                        && ui
                            .button(RichText::new("Banir").color(palette.danger))
                            .clicked()
                    {
                        chosen = Some(Modal::Ban { guild, user, history: 0 });
                    }
                    if kick && ui.button("Expulsar").clicked() {
                        chosen = Some(Modal::Kick { guild, user });
                    }
                    if moderate && ui.button("Castigar").clicked() {
                        chosen = Some(Modal::Timeout { guild, user, minutes: 60 });
                    }
                });
            }
        });
    }
    if let Some(modal) = chosen {
        act(app, AdminAction::Open(modal));
    }
}

fn invites(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, app, "Convites");
    let Some(list) = app.admin.settings.as_ref().and_then(|s| s.invites.clone()) else {
        ui.label(RichText::new("Carregando convites…").color(palette.muted));
        return;
    };
    if list.is_empty() {
        ui.label(RichText::new("Nenhum convite ativo.").color(palette.muted));
    }
    let mut revoke = None;
    for invite in list {
        let code = invite["code"].as_str().unwrap_or_default().to_owned();
        let inviter = invite["inviter"]["global_name"]
            .as_str()
            .or(invite["inviter"]["username"].as_str())
            .unwrap_or("?");
        let uses = invite["uses"].as_u64().unwrap_or(0);
        let max = invite["max_uses"].as_u64().unwrap_or(0);
        ui.horizontal(|ui| {
            ui.set_min_height(36.0);
            ui.label(
                RichText::new(format!("discord.gg/{code}"))
                    .font(font(Weight::Medium, 15.0))
                    .color(LINK),
            );
            ui.label(
                RichText::new(format!(
                    "por {inviter} • {uses}{} usos",
                    if max > 0 {
                        format!("/{max}")
                    } else {
                        String::new()
                    }
                ))
                .color(palette.muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new("Revogar").color(palette.danger))
                    .clicked()
                {
                    revoke = Some(code.clone());
                }
            });
        });
    }
    if let Some(code) = revoke {
        act(app, AdminAction::Revoke(code));
    }
}

fn bans(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, app, "Banimentos");
    let Some(list) = app.admin.settings.as_ref().and_then(|s| s.bans.clone()) else {
        ui.label(RichText::new("Carregando banimentos…").color(palette.muted));
        return;
    };
    ui.label(RichText::new(format!("{} banidos", list.len())).color(palette.muted));
    let mut unban = None;
    for ban in list {
        let user = &ban["user"];
        let name = user["global_name"]
            .as_str()
            .or(user["username"].as_str())
            .unwrap_or("?");
        let reason = ban["reason"].as_str().unwrap_or("Sem motivo informado");
        ui.horizontal(|ui| {
            ui.set_min_height(36.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(name)
                        .font(font(Weight::Medium, 15.0))
                        .color(palette.text),
                );
                ui.label(
                    RichText::new(reason)
                        .font(font(Weight::Regular, 12.0))
                        .color(palette.muted),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Revogar banimento").clicked() {
                    unban = crate::discord::parse::id(&user["id"]);
                }
            });
        });
    }
    if let Some(user) = unban {
        act(app, AdminAction::Unban(user));
    }
}

fn audit(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    title(ui, app, "Registro de auditoria");
    let Some(log) = app.admin.settings.as_ref().and_then(|s| s.audit.clone()) else {
        ui.label(RichText::new("Carregando registro…").color(palette.muted));
        return;
    };
    let names: std::collections::HashMap<String, String> = log["users"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|u| {
            Some((
                u["id"].as_str()?.to_owned(),
                u["global_name"]
                    .as_str()
                    .or(u["username"].as_str())?
                    .to_owned(),
            ))
        })
        .collect();
    for entry in log["audit_log_entries"].as_array().into_iter().flatten() {
        let who = entry["user_id"]
            .as_str()
            .and_then(|id| names.get(id))
            .cloned()
            .unwrap_or_else(|| "Alguém".into());
        let what = crate::admin::audit_action(entry["action_type"].as_i64().unwrap_or(0));
        let when = crate::discord::parse::id(&entry["id"])
            .and_then(|id| {
                jiff::Timestamp::from_millisecond(((id >> 22) + 1_420_070_400_000) as i64).ok()
            })
            .map(super::widgets::timestamp)
            .unwrap_or_default();
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            ui.label(
                RichText::new(hyphens(&who))
                    .font(font(Weight::SemiBold, 14.0))
                    .color(palette.strong),
            );
            ui.label(
                RichText::new(what)
                    .font(font(Weight::Regular, 14.0))
                    .color(palette.text),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(when)
                        .font(font(Weight::Regular, 12.0))
                        .color(palette.muted),
                );
            });
        });
    }
}

/// Discord's invite dialog: who the invite is for and where it leads,
/// friends to send it to one by one, and the link to copy.
fn invite(app: &mut App, ui: &mut Ui, link: Option<String>) {
    let palette = app.palette;
    let channel = app.admin.invite_channel.and_then(|c| app.model.channel(c));
    let guild = channel
        .and_then(|c| c.guild)
        .or(app.server())
        .and_then(|g| app.model.guild(g));
    let title = match guild {
        Some(g) => format!("Convidar amigos para {}", g.name),
        None => "Convidar amigos".to_owned(),
    };
    let place = channel.map(|c| {
        let mark = if c.kind.is_voice() { "🔊" } else { "#" };
        format!("{mark}{}", c.name)
    });
    let state = egui::Id::new("invite-dialog");
    let (mut search, mut sent): (String, Vec<crate::model::Id>) =
        ui.data(|d| d.get_temp(state)).unwrap_or_default();
    let query = search.to_lowercase();
    let mut friends: Vec<(crate::model::Id, String, String)> = app
        .model
        .relationships
        .iter()
        .filter(|(_, kind)| **kind == crate::nav::relationship::FRIEND)
        .filter_map(|(id, _)| app.model.user(*id))
        .map(|u| (u.id, u.name.clone(), u.username.clone()))
        .filter(|(_, name, username)| {
            query.is_empty()
                || name.to_lowercase().contains(&query)
                || username.to_lowercase().contains(&query)
        })
        .collect();
    // As Discord: the most recent conversations first, then who is online,
    // then the rest by name.
    let last_talk = |id: crate::model::Id| {
        app.model
            .direct
            .iter()
            .filter_map(|c| app.model.channel(*c))
            .filter(|c| c.recipients == [id])
            .filter_map(|c| c.last_message)
            .max()
    };
    friends.sort_by_cached_key(|(id, name, _)| {
        let online = app.model.user(*id).is_some_and(|u| u.status != crate::model::Status::Offline);
        (std::cmp::Reverse(last_talk(*id)), !online, name.to_lowercase())
    });
    let mut out: Vec<Action> = Vec::new();
    let mut close = false;
    let response = egui::Modal::new(egui::Id::new("invite-modal"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(
            Frame::new()
                .fill(palette.chat)
                .corner_radius(CornerRadius::same(12))
                .stroke(egui::Stroke::new(1.0, palette.selected)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(500.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            Frame::new().inner_margin(Margin { left: 24, right: 24, top: 20, bottom: 8 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.set_width(ui.available_width());
                    ui.add_sized(
                        vec2(ui.available_width() - 32.0, 0.0),
                        egui::Label::new(RichText::new(title).font(font(Weight::SemiBold, 20.0)).color(palette.strong)).wrap(),
                    );
                    let (x, r) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
                    paint_icon(ui, Icon::Close, x.center(), 22.0, if r.hovered() { palette.strong } else { palette.icon });
                    if r.clicked() {
                        close = true;
                    }
                });
                if let Some(place) = &place {
                    ui.label(RichText::new(format!("Os destinatários chegarão em {place}")).font(font(Weight::Regular, 15.0)).color(palette.muted));
                }
                ui.add_space(8.0);
                Frame::new()
                    .fill(palette.rail)
                    .corner_radius(CornerRadius::same(8))
                    .stroke(egui::Stroke::new(1.0, palette.selected))
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 10.0;
                            let (icon, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                            paint_icon(ui, Icon::Search, icon.center(), 18.0, palette.muted);
                            ui.add(
                                egui::TextEdit::singleline(&mut search)
                                    .frame(Frame::NONE)
                                    .hint_text(RichText::new("Buscar amigos").color(palette.muted))
                                    .font(font(Weight::Regular, 16.0))
                                    .text_color(palette.text)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                    });
                ui.add_space(6.0);
                ui.label(RichText::new("Amigos").font(font(Weight::Medium, 14.0)).color(palette.muted));
                egui::ScrollArea::vertical()
                    .id_salt("invite-friends")
                    .max_height(380.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if friends.is_empty() {
                            ui.label(RichText::new("Nenhum amigo encontrado.").color(palette.muted));
                        }
                        for (id, name, username) in &friends {
                            let (row, hover) = ui.allocate_exact_size(vec2(ui.available_width(), 53.0), Sense::hover());
                            if hover.hovered() {
                                ui.painter().rect_filled(row, CornerRadius::same(8), palette.hover);
                            }
                            let center = pos2(row.left() + 26.0, row.center().y);
                            if let Some(user) = app.model.user(*id) {
                                let behind = if hover.hovered() { palette.hover } else { palette.chat };
                                super::widgets::avatar(ui, app, user, center, 17.0, Some(behind));
                            }
                            ui.painter().text(pos2(row.left() + 54.0, row.center().y - 8.0), Align2::LEFT_CENTER, name, font(Weight::SemiBold, 16.0), palette.strong);
                            ui.painter().text(pos2(row.left() + 54.0, row.center().y + 10.0), Align2::LEFT_CENTER, username, font(Weight::Regular, 12.0), palette.muted);
                            let done = sent.contains(id);
                            let button = Rect::from_min_size(pos2(row.right() - 96.0, row.center().y - 17.0), vec2(86.0, 34.0));
                            let response = ui.interact(button, ui.id().with(("invite-to", *id)), Sense::click());
                            let fill = match (done, response.hovered()) {
                                (true, _) => Color32::TRANSPARENT,
                                (false, true) => palette.selected,
                                (false, false) => palette.hover,
                            };
                            ui.painter().rect(button, CornerRadius::same(8), fill, egui::Stroke::new(1.0, palette.selected), egui::StrokeKind::Inside);
                            ui.painter().text(button.center(), Align2::CENTER_CENTER, if done { "Enviado" } else { "Convidar" }, font(Weight::Medium, 14.0), if done { palette.muted } else { palette.strong });
                            if response.clicked() && !done && let Some(link) = &link {
                                sent.push(*id);
                                out.push(Action::Chat(crate::chat::ChatAction::SendQuietly { user: *id, text: link.clone() }));
                            }
                        }
                    });
            });
            Frame::new()
                .fill(palette.sidebar)
                .corner_radius(CornerRadius { nw: 0, ne: 0, sw: 12, se: 12 })
                .inner_margin(Margin { left: 24, right: 24, top: 16, bottom: 16 })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Ou, envie um convite do servidor a um amigo").font(font(Weight::Medium, 16.0)).color(palette.strong));
                    Frame::new()
                        .fill(palette.rail)
                        .corner_radius(CornerRadius::same(8))
                        .stroke(egui::Stroke::new(1.0, palette.selected))
                        .inner_margin(Margin::symmetric(10, 4))
                        .show(ui, |ui| {
                            ui.allocate_ui_with_layout(vec2(ui.available_width(), 32.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let text = link.clone().unwrap_or_else(|| "Criando convite…".into());
                                ui.label(RichText::new(text).font(font(Weight::Regular, 16.0)).color(palette.text));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let copied = ui.data(|d| d.get_temp::<bool>(state.with("copied")).unwrap_or(false));
                                    let fill = if copied { palette.online } else { palette.accent };
                                    let copy = egui::Button::new(RichText::new(if copied { "Copiado" } else { "Copiar" }).font(font(Weight::Medium, 14.0)).color(Color32::WHITE))
                                        .fill(fill)
                                        .corner_radius(CornerRadius::same(6))
                                        .min_size(vec2(90.0, 32.0));
                                    if ui.add_enabled(link.is_some(), copy).clicked() && let Some(link) = &link {
                                        ui.ctx().copy_text(link.clone());
                                        ui.data_mut(|d| d.insert_temp(state.with("copied"), true));
                                    }
                                });
                            });
                        });
                    ui.label(RichText::new("Seu link de convite expira em 7 dias.").font(font(Weight::Regular, 12.0)).color(palette.muted));
                });
        });
    if response.should_close() {
        close = true;
    }
    if close {
        ui.data_mut(|d| {
            d.remove::<(String, Vec<crate::model::Id>)>(state);
            d.remove::<bool>(state.with("copied"));
        });
        act(app, AdminAction::CloseModal);
    } else {
        ui.data_mut(|d| d.insert_temp(state, (search, sent)));
    }
    app.actions.extend(out);
}

/// "Criar seu servidor" and "Entrar em um servidor", as Discord's dialog:
/// a centred title, the steps, and Voltar with the main button at the foot.
fn create_guild(app: &mut App, ui: &mut Ui, modal: Modal) {
    use crate::admin::{CreateStep, TEMPLATES};
    let palette = app.palette;
    let busy = app.admin.busy;
    let error = app.admin.error.clone();
    let mut next: Option<Modal> = None;
    let mut submit = false;
    let mut close = false;
    let mut pick = false;
    let me_name = app
        .model
        .user(app.model.me)
        .map(|u| u.name.clone())
        .unwrap_or_default();
    let tile = palette.sidebar;
    // A row with an emoji, a label and a chevron, as Discord's choices.
    let choice = |ui: &mut Ui, emoji: &str, label: &str| -> bool {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
        let fill = if response.hovered() { palette.hover } else { tile };
        ui.painter().rect(rect, CornerRadius::same(8), fill, egui::Stroke::new(1.0, palette.selected), egui::StrokeKind::Inside);
        ui.painter().text(pos2(rect.left() + 26.0, rect.center().y), Align2::CENTER_CENTER, emoji, font(Weight::Regular, 22.0), palette.strong);
        ui.painter().text(pos2(rect.left() + 56.0, rect.center().y), Align2::LEFT_CENTER, label, font(Weight::Medium, 16.0), palette.strong);
        paint_icon(ui, Icon::ChevronRight, pos2(rect.right() - 24.0, rect.center().y), 20.0, palette.icon);
        response.clicked()
    };
    let centered = |ui: &mut Ui, text: &str, size: f32, weight: Weight, color: Color32| {
        ui.vertical_centered(|ui| {
            ui.add(egui::Label::new(RichText::new(text).font(font(weight, size)).color(color)).wrap());
        });
    };
    let response = egui::Modal::new(egui::Id::new("create-guild-modal"))
        .backdrop_color(Color32::from_black_alpha(180))
        .frame(
            Frame::new()
                .fill(palette.chat)
                .corner_radius(CornerRadius::same(12))
                .stroke(egui::Stroke::new(1.0, palette.selected)),
        )
        .show(ui.ctx(), |ui| {
            ui.set_width(460.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            // The close button at the top right.
            let corner = Rect::from_center_size(ui.max_rect().right_top() + vec2(-24.0, 24.0), Vec2::splat(28.0));
            let x = ui.interact(corner, ui.id().with("close"), Sense::click());
            Frame::new().inner_margin(Margin { left: 16, right: 16, top: 24, bottom: 16 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                match modal.clone() {
                    Modal::CreateGuild { step: CreateStep::Start, .. } => {
                        centered(ui, "Criar seu servidor", 24.0, Weight::Bold, palette.strong);
                        centered(ui, "Seu servidor é onde você e seus amigos se reúnem. Crie o seu e comece a interagir.", 15.0, Weight::Regular, palette.text);
                        ui.add_space(12.0);
                        egui::ScrollArea::vertical().max_height(330.0).auto_shrink([false, true]).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 8.0;
                            for (i, template) in TEMPLATES.iter().enumerate() {
                                if i == 1 {
                                    ui.add_space(4.0);
                                    ui.label(RichText::new("Começar de um molde").font(font(Weight::SemiBold, 12.0)).color(palette.text));
                                }
                                if choice(ui, template.emoji, template.label) {
                                    next = Some(Modal::CreateGuild {
                                        step: CreateStep::Audience,
                                        template: i,
                                        name: format!("Servidor de {me_name}"),
                                        icon: None,
                                    });
                                }
                            }
                        });
                        ui.add_space(8.0);
                        centered(ui, "Já tem um convite?", 20.0, Weight::SemiBold, palette.strong);
                        let join = egui::Button::new(RichText::new("Entrar em um servidor").font(font(Weight::Medium, 14.0)).color(palette.strong))
                            .fill(palette.hover)
                            .corner_radius(CornerRadius::same(8))
                            .min_size(vec2(ui.available_width(), 40.0));
                        if ui.add(join).clicked() {
                            next = Some(Modal::JoinGuild { code: String::new() });
                        }
                    }
                    Modal::CreateGuild { step: CreateStep::Audience, template, name, icon } => {
                        centered(ui, "Conte-nos mais sobre seu servidor", 24.0, Weight::Bold, palette.strong);
                        centered(ui, "Para ajudar com a configuração, diga se o seu novo servidor é apenas para alguns amigos ou para uma comunidade maior.", 15.0, Weight::Regular, palette.text);
                        ui.add_space(12.0);
                        let customize = Modal::CreateGuild { step: CreateStep::Customize, template, name, icon };
                        if choice(ui, "🏛️", "Para um clube ou comunidade") || choice(ui, "🧑‍🤝‍🧑", "Para mim e meus amigos") {
                            next = Some(customize.clone());
                        }
                        ui.add_space(4.0);
                        ui.vertical_centered(|ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                ui.label(RichText::new("Não tem certeza? Você pode").font(font(Weight::Regular, 14.0)).color(palette.text));
                                let skip = ui.add(egui::Label::new(RichText::new("pular esta pergunta").font(font(Weight::Medium, 14.0)).color(Color32::from_rgb(0x00, 0xa8, 0xfc))).sense(Sense::click()));
                                ui.label(RichText::new("por enquanto.").font(font(Weight::Regular, 14.0)).color(palette.text));
                                if skip.clicked() {
                                    next = Some(customize.clone());
                                }
                            });
                        });
                    }
                    Modal::CreateGuild { step: CreateStep::Customize, template, mut name, icon } => {
                        centered(ui, "Personalize seu servidor", 24.0, Weight::Bold, palette.strong);
                        centered(ui, "Deixe seu novo servidor com a sua cara dando um nome e um ícone a ele. Você pode alterá-los a qualquer momento.", 15.0, Weight::Regular, palette.text);
                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            let (spot, response) = ui.allocate_exact_size(Vec2::splat(80.0), Sense::click());
                            let picture = icon.as_ref().map(|uri| format!("bytes://new-guild-icon-{}", uri.len()));
                            match (&icon, &picture) {
                                (Some(uri), Some(key)) => {
                                    use base64::Engine;
                                    let data = uri.split_once(',').map(|(_, d)| d).unwrap_or_default();
                                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                                        ui.ctx().include_bytes(key.clone(), bytes);
                                    }
                                    egui::Image::new(key.clone()).corner_radius(CornerRadius::same(40)).paint_at(ui, spot);
                                }
                                _ => {
                                    // Discord's dashed circle with a camera and "ENVIAR".
                                    let steps = 28;
                                    for i in 0..steps {
                                        if i % 2 == 0 {
                                            let a0 = std::f32::consts::TAU * i as f32 / steps as f32;
                                            let a1 = std::f32::consts::TAU * (i + 1) as f32 / steps as f32;
                                            let c = spot.center();
                                            ui.painter().line_segment(
                                                [c + 38.0 * vec2(a0.cos(), a0.sin()), c + 38.0 * vec2(a1.cos(), a1.sin())],
                                                egui::Stroke::new(2.0, if response.hovered() { palette.strong } else { palette.icon }),
                                            );
                                        }
                                    }
                                    paint_icon(ui, Icon::Image, spot.center() - vec2(0.0, 8.0), 22.0, palette.icon);
                                    ui.painter().text(spot.center() + vec2(0.0, 14.0), Align2::CENTER_CENTER, "ENVIAR", font(Weight::Bold, 11.0), palette.icon);
                                    ui.painter().circle_filled(spot.right_top() + vec2(-8.0, 10.0), 11.0, palette.accent);
                                    paint_icon(ui, Icon::Plus, spot.right_top() + vec2(-8.0, 10.0), 14.0, Color32::WHITE);
                                }
                            }
                            if response.on_hover_text("Enviar um ícone").clicked() {
                                pick = true;
                            }
                        });
                        ui.add_space(8.0);
                        ui.label(RichText::new("Nome do servidor").font(font(Weight::SemiBold, 14.0)).color(palette.strong));
                        let edit = Frame::new()
                            .fill(palette.rail)
                            .corner_radius(CornerRadius::same(8))
                            .stroke(egui::Stroke::new(1.0, palette.selected))
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut name)
                                        .frame(Frame::NONE)
                                        .font(font(Weight::Regular, 16.0))
                                        .text_color(palette.text)
                                        .desired_width(f32::INFINITY),
                                )
                            });
                        if edit.inner.changed() {
                            next = Some(Modal::CreateGuild { step: CreateStep::Customize, template, name: name.clone(), icon: icon.clone() });
                        }
                        ui.label(RichText::new("Ao criar um servidor, você concorda com as Diretrizes da Comunidade do Discord.").font(font(Weight::Regular, 12.0)).color(palette.muted));
                    }
                    Modal::JoinGuild { mut code } => {
                        centered(ui, "Entrar em um servidor", 24.0, Weight::Bold, palette.strong);
                        centered(ui, "Insira um convite abaixo para entrar em um servidor existente", 15.0, Weight::Regular, palette.text);
                        ui.add_space(12.0);
                        ui.label(RichText::new("Link de convite *").font(font(Weight::SemiBold, 14.0)).color(palette.strong));
                        let edit = Frame::new()
                            .fill(palette.rail)
                            .corner_radius(CornerRadius::same(8))
                            .stroke(egui::Stroke::new(1.0, palette.selected))
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut code)
                                        .frame(Frame::NONE)
                                        .hint_text(RichText::new("https://discord.gg/hTKzmak").color(palette.muted))
                                        .font(font(Weight::Regular, 16.0))
                                        .text_color(palette.text)
                                        .desired_width(f32::INFINITY),
                                );
                                if ui.memory(|m| m.focused().is_none()) {
                                    r.request_focus();
                                }
                                r
                            });
                        if edit.inner.changed() {
                            next = Some(Modal::JoinGuild { code: code.clone() });
                        }
                        ui.add_space(4.0);
                        ui.label(RichText::new("Os convites são parecidos com estes:").font(font(Weight::SemiBold, 14.0)).color(palette.strong));
                        for example in ["hTKzmak", "https://discord.gg/hTKzmak", "https://discord.gg/cool-people"] {
                            ui.label(RichText::new(example).font(font(Weight::Regular, 14.0)).color(palette.text));
                        }
                    }
                    _ => {}
                }
                if let Some(error) = &error {
                    ui.label(RichText::new(error).color(palette.danger));
                }
            });
            paint_icon(ui, Icon::Close, corner.center(), 22.0, if x.hovered() { palette.strong } else { palette.icon });
            if x.clicked() {
                close = true;
            }
            // The foot: Voltar and the main button, except on the first step.
            let main = match &modal {
                Modal::CreateGuild { step: CreateStep::Customize, .. } => Some("Criar"),
                Modal::JoinGuild { .. } => Some("Entrar no servidor"),
                _ => None,
            };
            let back = match &modal {
                Modal::CreateGuild { step: CreateStep::Audience, .. } | Modal::JoinGuild { .. } => Some(Modal::CreateGuild {
                    step: CreateStep::Start,
                    template: 0,
                    name: String::new(),
                    icon: None,
                }),
                Modal::CreateGuild { step: CreateStep::Customize, template, name, icon } => Some(Modal::CreateGuild {
                    step: CreateStep::Audience,
                    template: *template,
                    name: name.clone(),
                    icon: icon.clone(),
                }),
                _ => None,
            };
            if back.is_some() || main.is_some() {
                Frame::new()
                    .fill(palette.sidebar)
                    .corner_radius(CornerRadius { nw: 0, ne: 0, sw: 12, se: 12 })
                    .inner_margin(Margin::same(16))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if let Some(back) = back {
                                let voltar = egui::Button::new(RichText::new("Voltar").font(font(Weight::Medium, 14.0)).color(palette.strong)).frame(false).min_size(vec2(60.0, 38.0));
                                if ui.add(voltar).clicked() {
                                    next = Some(back);
                                }
                            }
                            if let Some(main) = main {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let label = if busy { "Aguarde…" } else { main };
                                    let button = egui::Button::new(RichText::new(label).font(font(Weight::Medium, 14.0)).color(Color32::WHITE))
                                        .fill(palette.accent)
                                        .corner_radius(CornerRadius::same(8))
                                        .min_size(vec2(96.0, 38.0));
                                    if ui.add_enabled(!busy, button).clicked() {
                                        submit = true;
                                    }
                                });
                            }
                        });
                    });
            }
        });
    if response.should_close() {
        close = true;
    }
    if pick {
        act(app, AdminAction::PickIcon);
    }
    if close {
        act(app, AdminAction::CloseModal);
        return;
    }
    if let Some(next) = next {
        app.admin.error = None;
        app.admin.modal = Some(next);
    }
    if submit && !busy {
        act(app, AdminAction::Submit);
    }
}
