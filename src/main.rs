//! Discofast: Discord, native and fast.

mod admin;
mod animation;
mod app;
mod backend;
mod call;
mod chat;
mod devices;
mod discovery;
mod events;
mod discord;
mod folders;
mod icons;
mod interact;
mod login;
mod manage;
mod markdown;
mod media;
mod migrate;
mod model;
mod nav;
mod notify;
mod paste;
mod resident;
mod room;
mod session;
mod settings;
mod sounds;
mod spell;
mod stats;
mod text;
mod theme;
mod threads;
mod ui;
mod updates;
mod voice;

use app::App;

/// One window; the app itself outlives it in the shell.
struct Window {
    app: fastframe_shell::Held<App>,
}

impl eframe::App for Window {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.window_logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let app = &mut *self.app;
        app.stats.frame(frame.info().cpu_usage);
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F12)) {
            app.show_stats = !app.show_stats;
        }
        app.poll(ui.ctx());
        ui::show(app, ui);
        if app.show_stats {
            ui::overlay::show(app, &app.stats, ui);
        }
        ui::pointing_hand(ui.ctx());
        app.apply();
    }
}

/// What each window's context needs: image loaders, icons, fonts, emoji
/// and the style.
fn setup(ctx: &egui::Context) {
    static EMOJI: std::sync::Once = std::sync::Once::new();
    EMOJI.call_once(|| {
        fastframe_emoji::EmojiSetup::default().install();
        std::thread::spawn(fastframe_emoji::warm_up);
    });
    egui_extras::install_image_loaders(ctx);
    fastframe_icons::install::<icons::Icon>(ctx);
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

fn options() -> eframe::NativeOptions {
    let icon = egui::IconData {
        rgba: resident::icon(256),
        width: 256,
        height: 256,
    };
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Discofast")
            .with_app_id("discofast")
            .with_icon(icon)
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([940.0, 500.0]),
        ..Default::default()
    }
}

const SLOT: &str = "io.github.th3_rocha.Discofast";

/// The name `--version` answers with. A FastDisco 0.9 updater downloads
/// this app under its old name and expects `fastdisco <version>`; that
/// copy (a `fastdisco` executable, or a `fastdisco-*.AppImage`) answers
/// with the old name, every other copy with the new one.
fn version_name() -> &'static str {
    let legacy = |path: std::path::PathBuf| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.to_ascii_lowercase().starts_with(updates::LEGACY_SLUG))
    };
    let appimage = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from);
    let running = appimage.or_else(|| std::env::current_exe().ok());
    if running.is_some_and(legacy) {
        updates::LEGACY_SLUG
    } else {
        updates::CONFIG.slug
    }
}

fn main() -> anyhow::Result<()> {
    // First of all: the update helper runs here when asked
    // (`--apply-update`) and exits, and the restarted copy learns how the
    // update went.
    let launch = fastframe_update::intercept(&updates::CONFIG);
    // The updater asks a downloaded copy for its version before trusting it.
    if launch.arguments.get(1).and_then(|a| a.to_str()) == Some("--version") {
        println!("{} {}", version_name(), env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("discofast=info,songbird=warn"),
    )
    .init();
    let slot = fastframe_instance::Slot::new(SLOT);
    // `discofast reload-themes`, from Omarchy's theme hook: tell the
    // running copy and leave.
    if std::env::args().nth(1).as_deref() == Some("reload-themes") {
        let _ = slot.send("reload-themes");
        return Ok(());
    }
    // `discofast write-icon <file.png> [size]`: the app icon, for packaging.
    if std::env::args().nth(1).as_deref() == Some("write-icon") {
        let path = std::env::args()
            .nth(2)
            .ok_or_else(|| anyhow::anyhow!("write-icon <file.png> [size]"))?;
        let size: u32 = std::env::args()
            .nth(3)
            .and_then(|s| s.parse().ok())
            .unwrap_or(256);
        let pixels = resident::icon(size as usize);
        image::save_buffer(&path, &pixels, size, size, image::ExtendedColorType::Rgba8)?;
        return Ok(());
    }
    let waker = fastframe_shell::Waker::default();
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::<&'static str>::new()));
    let queue = requests.clone();
    let wake = waker.clone();
    let _guard = match slot.claim("show", move |request| {
        let known = match request {
            "show" => "show",
            "reload-themes" => "reload-themes",
            _ => return None,
        };
        queue.lock().unwrap_or_else(|e| e.into_inner()).push(known);
        wake.wake();
        Some("ok".to_owned())
    }) {
        fastframe_instance::Claim::First(guard) => guard,
        // Another copy runs and has been asked to show its window.
        _ => return Ok(()),
    };
    migrate::run();
    let mut app = App::new(&waker);
    app.requests = requests;
    app.update_receipt = launch.receipt;
    if let Some(error) = launch.error {
        app.update = updates::Status::Failed(error);
    }
    app.start_themes();
    fastframe_shell::Shell::new(app, &waker)
        .idle(fastframe_tray::idle)
        .run(|lease| {
            eframe::run_native(
                "Discofast",
                options(),
                Box::new(move |cc| {
                    setup(&cc.egui_ctx);
                    fastframe_file::mark_ui_thread();
                    let mut app = lease.take(&cc.egui_ctx);
                    app.attach(&cc.egui_ctx);
                    Ok(Box::new(Window { app }))
                }),
            )
        })
        .map_err(|e| anyhow::anyhow!("{e}"))
}
