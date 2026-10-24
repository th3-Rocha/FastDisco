//! FastDisco: Discord, native and fast.

mod animation;
mod app;
mod backend;
mod chat;
mod discord;
mod icons;
mod login;
mod markdown;
mod media;
mod model;
mod notify;
mod session;
mod settings;
mod stats;
mod text;
mod theme;
mod ui;

use app::App;

struct Shell {
    app: App,
    stats: stats::Stats,
}

impl eframe::App for Shell {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.app.save_settings();
        self.app.flush_files();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.stats.frame(frame.info().cpu_usage);
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F12)) {
            self.app.show_stats = !self.app.show_stats;
        }
        self.app.poll(ui.ctx());
        ui::show(&mut self.app, ui);
        if self.app.show_stats {
            ui::overlay::show(&self.app, &self.stats, ui);
        }
        ui::pointing_hand(ui.ctx());
        self.app.apply();
    }
}

fn setup(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    fastframe_icons::install::<icons::Icon>(ctx);
    fastframe_emoji::EmojiSetup::default().install();
    std::thread::spawn(fastframe_emoji::warm_up);
    ctx.add_plugin(fastframe_emoji::EmojiPlugin::default());
    let mut fonts = fastframe_fonts::FontSetup::default().definitions();
    let rendering = fastframe_text::detect();
    rendering.apply_to(&mut fonts);
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|style| {
        rendering.apply_to_visuals(&mut style.visuals);
        style.spacing.menu_margin = egui::Margin::same(8);
        style.spacing.button_padding = egui::vec2(8.0, 6.0);
        style.interaction.tooltip_delay = 0.0;
        style.interaction.tooltip_grace_time = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
        style.override_font_id = None;
        style.text_styles.insert(
            egui::TextStyle::Body,
            fastframe_fonts::Weight::Medium.font_id(14.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            fastframe_fonts::Weight::Medium.font_id(14.0),
        );
    });
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("fastdisco=info"))
        .init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FastDisco")
            .with_app_id("fastdisco")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([940.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "FastDisco",
        options,
        Box::new(|cc| {
            setup(&cc.egui_ctx);
            fastframe_file::mark_ui_thread();
            Ok(Box::new(Shell {
                app: {
                    let mut app = App::new(&cc.egui_ctx);
                    app.start_themes();
                    app
                },
                stats: stats::Stats::start(),
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
