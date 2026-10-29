//! The server settings' newer pages: the icon and banner on the
//! overview, a role's permissions, emoji and sticker uploads, AutoMod's
//! rules, and a channel's settings with its permission overwrites.

use egui::{Color32, CornerRadius, Frame, Margin, RichText, Sense, Ui, Vec2, vec2};
use fastframe_fonts::Weight;

use super::widgets::{font, hyphens};
use crate::app::{Action, App};
use crate::manage::{ManageAction, Picking};
use crate::model::{ChannelKind, Id, permission};

fn act(app: &mut App, action: ManageAction) {
    app.actions.push(Action::Manage(action));
}

fn heading(ui: &mut Ui, app: &App, text: &str) {
    ui.add_space(12.0);
    ui.label(
        RichText::new(text)
            .font(font(Weight::Bold, 12.0))
            .color(app.palette.muted),
    );
}

fn field(ui: &mut Ui, app: &App, text: &mut String, hint: &str, multiline: bool) {
    let palette = app.palette;
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
            );
        });
}

fn button(ui: &mut Ui, app: &App, label: &str, fill: Option<Color32>) -> bool {
    let fill = fill.unwrap_or(app.palette.selected);
    let text = if fill == app.palette.selected { app.palette.text } else { Color32::WHITE };
    ui.add(
        egui::Button::new(RichText::new(label).font(font(Weight::Medium, 14.0)).color(text))
            .fill(fill)
            .corner_radius(CornerRadius::same(4))
            .min_size(vec2(0.0, 32.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
    .clicked()
}

/// Under the overview's name: changing or removing the icon and banner.
pub fn pictures(app: &mut App, ui: &mut Ui, guild: Id) {
    let palette = app.palette;
    let (icon, banner) = app
        .model
        .guild(guild)
        .map(|g| (g.icon.is_some(), g.banner.clone()))
        .unwrap_or_default();
    heading(ui, app, "ÍCONE");
    ui.label(RichText::new("Recomendamos uma imagem de pelo menos 512x512.").color(palette.muted));
    ui.horizontal(|ui| {
        if button(ui, app, "Enviar imagem", Some(palette.accent)) {
            act(app, ManageAction::PickImage(Picking::GuildIcon));
        }
        if icon && button(ui, app, "Remover", None) {
            act(app, ManageAction::RemoveGuildImage { banner: false });
        }
    });
    heading(ui, app, "BANNER DO SERVIDOR");
    if let Some(uri) = banner.as_deref().and_then(|b| app.picture(Some(b))) {
        let width = ui.available_width().min(480.0);
        ui.add(
            egui::Image::new(uri)
                .fit_to_exact_size(vec2(width, width * 0.4))
                .corner_radius(CornerRadius::same(6)),
        );
    }
    ui.label(
        RichText::new("Aparece no topo da lista de canais. Precisa de nível 2 de impulso.")
            .color(palette.muted),
    );
    ui.horizontal(|ui| {
        if button(ui, app, "Enviar banner", Some(palette.accent)) {
            act(app, ManageAction::PickImage(Picking::GuildBanner));
        }
        if banner.is_some() && button(ui, app, "Remover", None) {
            act(app, ManageAction::RemoveGuildImage { banner: true });
        }
    });
}

/// A role's permissions as Discord lists them: grouped, each with a
/// switch and what it allows.
pub fn role_permissions(app: &mut App, ui: &mut Ui, role: Id) {
    let palette = app.palette;
    ui.horizontal(|ui| {
        heading(ui, app, "PERMISSÕES");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if button(ui, app, "Excluir cargo", Some(palette.danger)) {
                act(app, ManageAction::DeleteRole(role));
            }
        });
    });
    let bits = app.manage.role_permissions;
    for (group, list) in permission::ALL {
        heading(ui, app, group);
        for (bit, name, about) in *list {
            let mut on = bits & bit != 0;
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 60.0);
                    ui.label(
                        RichText::new(*name)
                            .font(font(Weight::Medium, 15.0))
                            .color(palette.strong),
                    );
                    ui.label(
                        RichText::new(*about)
                            .font(font(Weight::Regular, 13.0))
                            .color(palette.muted),
                    );
                });
                if ui.checkbox(&mut on, "").changed() {
                    act(app, ManageAction::ToggleRolePermission(*bit));
                }
            });
            ui.separator();
        }
    }
}

/// Under the emoji list: naming and uploading one, and removing each.
pub fn emoji_upload(app: &mut App, ui: &mut Ui, guild: Id) {
    let palette = app.palette;
    heading(ui, app, "ENVIAR EMOJI");
    ui.label(
        RichText::new("Nomes com letras, números e _, de 2 a 32 caracteres. Até 256 KB.")
            .color(palette.muted),
    );
    let mut name = app.manage.emoji_name.clone();
    ui.horizontal(|ui| {
        ui.set_max_width(420.0);
        field(ui, app, &mut name, "nome_do_emoji", false);
    });
    app.manage.emoji_name = name;
    if button(ui, app, "Enviar emoji", Some(palette.accent)) {
        act(app, ManageAction::PickImage(Picking::Emoji));
    }
    let list = app
        .model
        .guild(guild)
        .map(|g| g.emojis.clone())
        .unwrap_or_default();
    if !list.is_empty() {
        heading(ui, app, "REMOVER");
    }
    for e in list {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
            if let Some(uri) = app.picture(Some(&crate::model::CustomEmoji::url(e.id, e.animated))) {
                egui::Image::new(uri).paint_at(ui, rect);
            }
            ui.label(RichText::new(format!(":{}:", e.name)).color(palette.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("Excluir").color(palette.danger)).clicked() {
                    act(app, ManageAction::DeleteEmoji(e.id));
                }
            });
        });
    }
}

/// "Figurinhas": the server's stickers and uploading one.
pub fn stickers(app: &mut App, ui: &mut Ui, guild: Id) {
    let palette = app.palette;
    ui.label(
        RichText::new("Figurinhas")
            .font(font(Weight::Bold, 20.0))
            .color(palette.strong),
    );
    let list = app
        .model
        .guild(guild)
        .map(|g| g.stickers.clone())
        .unwrap_or_default();
    ui.label(RichText::new(format!("{} figurinhas neste servidor.", list.len())).color(palette.muted));
    heading(ui, app, "ENVIAR FIGURINHA");
    ui.label(
        RichText::new("PNG, APNG ou GIF de 320x320, até 512 KB; ou Lottie (JSON).")
            .color(palette.muted),
    );
    let (mut name, mut tag) = (app.manage.sticker_name.clone(), app.manage.sticker_tag.clone());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(300.0);
            field(ui, app, &mut name, "Nome da figurinha", false);
        });
        ui.vertical(|ui| {
            ui.set_width(120.0);
            field(ui, app, &mut tag, "Emoji: 🙂", false);
        });
    });
    app.manage.sticker_name = name;
    app.manage.sticker_tag = tag;
    if button(ui, app, "Enviar figurinha", Some(palette.accent)) {
        act(app, ManageAction::PickImage(Picking::Sticker));
    }
    ui.add_space(12.0);
    ui.horizontal_wrapped(|ui| {
        for sticker in list {
            ui.vertical(|ui| {
                ui.set_width(110.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(96.0), Sense::hover());
                if let Some(uri) = sticker.url().as_deref().and_then(|u| app.picture(Some(u))) {
                    egui::Image::new(uri).paint_at(ui, rect);
                }
                ui.label(RichText::new(&sticker.name).color(palette.text));
                if ui.button(RichText::new("Excluir").color(palette.danger)).clicked() {
                    act(app, ManageAction::DeleteSticker(sticker.id));
                }
            });
        }
    });
}

/// "AutoMod": the rules, each on or off, and a new rule that blocks
/// words.
pub fn automod(app: &mut App, ui: &mut Ui) {
    let palette = app.palette;
    ui.label(
        RichText::new("AutoMod")
            .font(font(Weight::Bold, 20.0))
            .color(palette.strong),
    );
    ui.label(
        RichText::new("O AutoMod bloqueia mensagens antes que alguém as veja, segundo as regras abaixo.")
            .color(palette.muted),
    );
    let Some(rules) = app.manage.automod.clone() else {
        ui.spinner();
        return;
    };
    if rules.is_empty() {
        ui.label(RichText::new("Nenhuma regra ainda.").color(palette.muted));
    }
    for rule in rules {
        let Some(id) = crate::discord::parse::id(&rule["id"]) else {
            continue;
        };
        let name = rule["name"].as_str().unwrap_or_default().to_owned();
        let mut enabled = rule["enabled"].as_bool().unwrap_or(false);
        let kind = match rule["trigger_type"].as_u64() {
            Some(1) => "Palavras personalizadas",
            Some(3) => "Spam suspeito",
            Some(4) => "Palavras de listas do Discord",
            Some(5) => "Excesso de menções",
            Some(6) => "Perfil de membro",
            _ => "Regra",
        };
        let words = rule["trigger_metadata"]["keyword_filter"]
            .as_array()
            .map(|l| l.iter().filter_map(|w| w.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        Frame::new()
            .fill(palette.sidebar)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(hyphens(&name)).font(font(Weight::SemiBold, 15.0)).color(palette.strong));
                        ui.label(RichText::new(kind).color(palette.muted));
                        if !words.is_empty() {
                            ui.label(RichText::new(&words).font(font(Weight::Regular, 13.0)).color(palette.text));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Excluir").color(palette.danger)).clicked() {
                            act(app, ManageAction::DeleteRule(id));
                        }
                        if ui.checkbox(&mut enabled, "Ativa").changed() {
                            act(app, ManageAction::ToggleRule(id, enabled));
                        }
                    });
                });
            });
    }
    heading(ui, app, "NOVA REGRA: BLOQUEAR PALAVRAS");
    let (mut name, mut words) = (app.manage.rule_name.clone(), app.manage.rule_words.clone());
    field(ui, app, &mut name, "Nome da regra", false);
    field(ui, app, &mut words, "Palavras, separadas por vírgula (* no começo ou fim vale como curinga)", true);
    app.manage.rule_name = name;
    app.manage.rule_words = words;
    if button(ui, app, "Criar regra", Some(palette.accent)) {
        act(app, ManageAction::CreateRule);
    }
}

/// A channel's own page: name, topic, slow mode and age restriction, then
/// its permissions for @everyone, each role and member that has some.
pub fn channel(app: &mut App, ui: &mut Ui, guild: Id) {
    let palette = app.palette;
    let Some(id) = app.manage.channel else { return };
    let Some(channel) = app.model.channel(id).cloned() else {
        return;
    };
    let voice = channel.kind.is_voice();
    ui.label(
        RichText::new(format!("{} — Configurações do canal", channel.name))
            .font(font(Weight::Bold, 20.0))
            .color(palette.strong),
    );
    let mut name = app.manage.channel_name.clone();
    let mut topic = app.manage.channel_topic.clone();
    heading(ui, app, "NOME DO CANAL");
    field(ui, app, &mut name, "", false);
    if !voice && channel.kind != ChannelKind::Category {
        heading(ui, app, "TÓPICO DO CANAL");
        field(ui, app, &mut topic, "Diga a todos como usar este canal!", true);
        heading(ui, app, "MODO LENTO");
        let choices = [
            (0, "Desativado"),
            (5, "5s"),
            (10, "10s"),
            (30, "30s"),
            (60, "1min"),
            (300, "5min"),
            (900, "15min"),
            (3_600, "1h"),
            (21_600, "6h"),
        ];
        let mut slowmode = app.manage.slowmode;
        egui::ComboBox::from_id_salt("slowmode")
            .selected_text(
                choices
                    .iter()
                    .find(|c| c.0 == slowmode)
                    .map_or_else(|| format!("{slowmode}s"), |c| c.1.to_owned()),
            )
            .show_ui(ui, |ui| {
                for (seconds, label) in choices {
                    ui.selectable_value(&mut slowmode, seconds, label);
                }
            });
        app.manage.slowmode = slowmode;
    }
    let mut nsfw = app.manage.nsfw;
    ui.checkbox(
        &mut nsfw,
        RichText::new("Canal restrito para maiores de 18 anos").color(palette.text),
    );
    app.manage.nsfw = nsfw;
    app.manage.channel_name = name;
    app.manage.channel_topic = topic;
    ui.horizontal(|ui| {
        if button(ui, app, "Salvar alterações", Some(palette.online)) {
            act(app, ManageAction::SaveChannel);
        }
        if button(ui, app, "Excluir canal", Some(palette.danger)) {
            act(app, ManageAction::DeleteChannel);
        }
    });

    // The permissions: who has an overwrite, and adding one.
    heading(ui, app, "PERMISSÕES");
    let roles = app
        .model
        .guild(guild)
        .map(|g| g.roles.clone())
        .unwrap_or_default();
    let label_of = |target: Id, kind: i64| -> String {
        if kind == 1 {
            format!("@{}", app.model.name_in(Some(guild), target))
        } else if target == guild {
            "@everyone".to_owned()
        } else {
            roles
                .iter()
                .find(|r| r.id == target)
                .map_or_else(|| "cargo".to_owned(), |r| r.name.clone())
        }
    };
    let selected = app.manage.overwrite.clone();
    let mut out: Vec<ManageAction> = Vec::new();
    let mut pick = None;
    ui.horizontal_wrapped(|ui| {
        let mut targets: Vec<(Id, i64)> = channel.overwrites.iter().map(|o| (o.id, o.kind)).collect();
        if !targets.iter().any(|(t, _)| *t == guild) {
            targets.insert(0, (guild, 0));
        }
        if let Some(o) = &selected
            && !targets.iter().any(|(t, _)| *t == o.id)
        {
            targets.push((o.id, o.kind));
        }
        for (target, kind) in targets {
            let on = selected.as_ref().is_some_and(|o| o.id == target);
            if ui
                .selectable_label(on, RichText::new(hyphens(&label_of(target, kind))).color(palette.text))
                .clicked()
            {
                pick = Some((target, kind));
            }
        }
        egui::ComboBox::from_id_salt("add-overwrite")
            .selected_text("+ Adicionar cargo")
            .show_ui(ui, |ui| {
                for role in roles.iter().filter(|r| r.id != guild) {
                    if !channel.overwrites.iter().any(|o| o.id == role.id)
                        && ui.selectable_label(false, hyphens(&role.name)).clicked()
                    {
                        pick = Some((role.id, 0));
                    }
                }
            });
    });
    if let Some((target, kind)) = pick {
        out.push(ManageAction::SelectOverwrite { id: target, kind });
    }
    let Some(overwrite) = selected else {
        app.actions.extend(out.into_iter().map(Action::Manage));
        return;
    };
    ui.label(
        RichText::new(format!(
            "Permissões de {} neste canal: ✕ nega, / segue o servidor, ✓ permite.",
            label_of(overwrite.id, overwrite.kind)
        ))
        .color(palette.muted),
    );
    for (group, list) in permission::ALL {
        if *group == "PERMISSÕES AVANÇADAS" {
            continue;
        }
        heading(ui, app, group);
        for (bit, name, _) in *list {
            let state: i8 = if overwrite.allow & bit != 0 {
                1
            } else if overwrite.deny & bit != 0 {
                -1
            } else {
                0
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(*name).font(font(Weight::Medium, 14.0)).color(palette.strong));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (value, mark, color) in [
                        (1i8, "✓", palette.online),
                        (0, "/", palette.muted),
                        (-1, "✕", palette.danger),
                    ] {
                        let text = RichText::new(mark).font(font(Weight::Bold, 14.0)).color(
                            if state == value { Color32::WHITE } else { color },
                        );
                        let fill = if state == value { color } else { palette.rail };
                        if ui
                            .add(egui::Button::new(text).fill(fill).min_size(vec2(28.0, 24.0)))
                            .clicked()
                        {
                            out.push(ManageAction::SetOverwrite { bit: *bit, state: value });
                        }
                    }
                });
            });
        }
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if button(ui, &*app, "Salvar permissões", Some(palette.online)) {
            out.push(ManageAction::SaveOverwrite);
        }
        if overwrite.id != guild && button(ui, &*app, "Remover", Some(palette.danger)) {
            out.push(ManageAction::RemoveOverwrite);
        }
    });
    app.actions.extend(out.into_iter().map(Action::Manage));
}
