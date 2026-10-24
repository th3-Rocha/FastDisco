//! The performance overlay at the top right, toggled with F12.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, RichText, Ui, vec2};

use crate::app::App;
use crate::stats::Stats;

const GOOD: Color32 = Color32::from_rgb(0x23, 0xa5, 0x5a);
const WARN: Color32 = Color32::from_rgb(0xf0, 0xb2, 0x32);
const BAD: Color32 = Color32::from_rgb(0xf2, 0x3f, 0x43);

pub fn show(app: &App, stats: &Stats, ui: &Ui) {
    let ctx = ui.ctx();
    // Keep the numbers moving without drawing faster than they change.
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
    let process = stats.process();
    let (frame_avg, frame_max) = stats.frame_ms();
    let textures = ctx.tex_manager().read().num_allocated();
    let uptime = stats.uptime().as_secs();
    let messages: usize = app.model.channels.iter().map(|c| c.messages.len()).sum();

    egui::Area::new(egui::Id::new("stats-overlay"))
        .order(egui::Order::Foreground)
        .anchor(Align2::RIGHT_TOP, vec2(-12.0, 12.0))
        .interactable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_black_alpha(200))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                    egui::Grid::new("stats-grid")
                        .num_columns(2)
                        .spacing(vec2(16.0, 2.0))
                        .show(ui, |ui| {
                            let row = |ui: &mut Ui, label: &str, value: String, color: Color32| {
                                ui.label(
                                    RichText::new(label)
                                        .monospace()
                                        .size(12.0)
                                        .color(Color32::from_gray(160)),
                                );
                                ui.label(RichText::new(value).monospace().size(12.0).color(color));
                                ui.end_row();
                            };
                            let white = Color32::from_gray(235);
                            row(ui, "FPS", stats.fps().to_string(), white);
                            let frame_color = if frame_avg < 4.0 {
                                GOOD
                            } else if frame_avg < 12.0 {
                                WARN
                            } else {
                                BAD
                            };
                            row(
                                ui,
                                "Frame (CPU)",
                                format!("{frame_avg:.2} ms (máx {frame_max:.1})"),
                                frame_color,
                            );
                            let mb = process.memory as f64 / 1_048_576.0;
                            let ram_color = if mb < 200.0 {
                                GOOD
                            } else if mb < 400.0 {
                                WARN
                            } else {
                                BAD
                            };
                            let share =
                                100.0 * process.memory as f64 / process.system_memory.max(1) as f64;
                            row(ui, "RAM", format!("{mb:.1} MB ({share:.1}%)"), ram_color);
                            row(
                                ui,
                                "Virtual",
                                format!("{:.0} MB", process.virtual_memory as f64 / 1_048_576.0),
                                white,
                            );
                            let cpu_color = if process.cpu < 2.0 {
                                GOOD
                            } else if process.cpu < 10.0 {
                                WARN
                            } else {
                                BAD
                            };
                            row(
                                ui,
                                "CPU",
                                format!(
                                    "{:.1}% ({:.1}% de 1 núcleo)",
                                    process.cpu, process.cpu_core
                                ),
                                cpu_color,
                            );
                            row(ui, "Threads", process.threads.to_string(), white);
                            row(ui, "Núcleos", process.cores.to_string(), white);
                            row(ui, "Texturas", textures.to_string(), white);
                            row(ui, "Servidores", app.model.servers.len().to_string(), white);
                            row(ui, "Canais", app.model.channels.len().to_string(), white);
                            row(ui, "Mensagens", messages.to_string(), white);
                            row(
                                ui,
                                "Aberto há",
                                format!(
                                    "{}:{:02}:{:02}",
                                    uptime / 3600,
                                    uptime / 60 % 60,
                                    uptime % 60
                                ),
                                white,
                            );
                        });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("F12 para fechar")
                            .size(11.0)
                            .color(Color32::from_gray(120)),
                    );
                });
        });
}
