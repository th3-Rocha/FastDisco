//! FastDisco: Discord, native and fast.

mod app;
mod icons;
mod model;
mod theme;
mod ui;

use app::App;

struct Shell {
    app: App,
}

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui::show(&mut self.app, ui);
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
        style.visuals = egui::Visuals::dark();
        rendering.apply_to_visuals(&mut style.visuals);
        style.visuals.selection.bg_fill = theme::Palette::dark().accent;
        style.visuals.text_cursor.stroke.color = theme::Palette::dark().text;
    });
}

fn main() -> anyhow::Result<()> {
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
            Ok(Box::new(Shell {
                app: App::new(model::Model::sample()),
            }))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
