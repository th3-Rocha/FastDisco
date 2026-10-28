//! Discofast outliving its window: the tray item, closing to the tray,
//! and the headless ticks that keep the gateway, notifications and the
//! unread count going while no window exists.

use std::time::Duration;

use fastframe_shell::{Closed, Headless, Resident};
use fastframe_tray::{Config, Event, MenuItem, Tray};

use crate::app::App;

/// The tray item, or `None` where the desktop has no tray.
pub fn tray(waker: &fastframe_shell::Waker) -> Option<Tray> {
    let waker = waker.clone();
    Tray::spawn(
        Config {
            id: "discofast",
            title: "Discofast".into(),
            icon: icon,
            template_icon: None,
            themed_icon: false,
            menu: vec![
                MenuItem::action("show", "Abrir o Discofast"),
                MenuItem::Separator,
                MenuItem::action("notifications", "Silenciar notificações"),
                MenuItem::Separator,
                MenuItem::action("quit", "Sair do Discofast"),
            ],
        },
        move || waker.wake(),
    )
}

/// The red dot and the tooltip while messages wait, as Discord's tray.
pub fn show_unread(tray: Option<&mut Tray>, count: u32) {
    let Some(tray) = tray else { return };
    if count > 0 {
        tray.set_icon(icon_unread, None);
        tray.set_tooltip(format!(
            "Discofast\n{count} {}",
            if count == 1 { "menção" } else { "menções" }
        ));
    } else {
        tray.set_icon(icon, None);
        tray.set_tooltip("Discofast");
    }
}

/// The app's mark: a blurple rounded square with two chat bubbles.
pub fn icon(size: usize) -> Vec<u8> {
    draw(size, false)
}

fn icon_unread(size: usize) -> Vec<u8> {
    draw(size, true)
}

/// The pig snout of assets/icon.svg, `size` pixels square, straight RGBA;
/// `unread` adds the red dot of waiting messages.
fn draw(size: usize, unread: bool) -> Vec<u8> {
    use resvg::{tiny_skia, usvg};
    let side = size.max(1) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(side, side).expect("a non-empty icon");
    let tree = usvg::Tree::from_data(
        include_bytes!("../assets/icon.svg"),
        &usvg::Options::default(),
    )
    .expect("assets/icon.svg parses");
    let scale = side as f32 / tree.size().width();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    if unread {
        let s = side as f32;
        let mut red = tiny_skia::Paint::default();
        red.set_color_rgba8(0xf2, 0x3f, 0x43, 0xff);
        red.anti_alias = true;
        if let Some(dot) = tiny_skia::PathBuilder::from_circle(s * 0.80, s * 0.80, s * 0.20) {
            pixmap.fill_path(
                &dot,
                &red,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
    pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let c = pixel.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

impl App {
    /// Tray clicks and menu entries; `window` says whether one is open.
    pub fn tray_events(&mut self, ctx: Option<&egui::Context>) {
        for request in self.take_requests() {
            match (request, ctx) {
                ("show", Some(ctx)) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                ("show", None) => self.wants_show = true,
                ("reload-themes", _) => self.reload_themes(),
                _ => {}
            }
        }
        let events = self.tray.as_ref().map(Tray::events).unwrap_or_default();
        for event in events {
            match event {
                Event::Toggle => match ctx {
                    Some(ctx) if self.can_hide() => {
                        self.hide_intent = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Some(ctx) => ctx.send_viewport_cmd(egui::ViewportCommand::Focus),
                    None => self.wants_show = true,
                },
                Event::Show | Event::Menu("show") => match ctx {
                    Some(ctx) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    None => self.wants_show = true,
                },
                Event::Menu("notifications") => {
                    self.settings.notifications = !self.settings.notifications;
                    let label = if self.settings.notifications {
                        "Silenciar notificações"
                    } else {
                        "Ativar notificações"
                    };
                    if let Some(tray) = &mut self.tray {
                        tray.set_label("notifications", label);
                    }
                    self.save_settings();
                }
                Event::Menu("quit") => {
                    self.quit_requested = true;
                    if let Some(ctx) = ctx {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Event::Menu(_) => {}
            }
        }
    }

    /// Whether closing the window can leave Discofast in the tray.
    fn can_hide(&self) -> bool {
        self.settings.close_to_tray && self.tray.as_ref().is_some_and(Tray::is_shown)
    }

    /// The window's close button: to the tray, or quit. Called from
    /// `eframe::App::logic`, which runs even for a minimized window.
    pub fn window_logic(&mut self, ctx: &egui::Context) {
        self.tray_events(Some(ctx));
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_requested && self.can_hide()
        {
            self.hide_intent = true;
        }
    }
}

impl Resident for App {
    fn closed(&self) -> Closed {
        if !self.quit_requested && self.hide_intent {
            Closed::Hide
        } else {
            Closed::Quit
        }
    }

    fn window_gone(&mut self) {
        self.hide_intent = false;
        self.wants_show = false;
        self.window_open = false;
        self.chat.player = None;
        self.chat.popout = None;
    }

    fn headless_frame(&mut self, ctx: &egui::Context) -> Headless {
        self.tray_events(None);
        self.poll(ctx);
        self.apply();
        if self.quit_requested {
            Headless::Quit
        } else if self.wants_show {
            Headless::Show
        } else {
            // The backend and the tray wake the shell; nothing is due before.
            Headless::WaitFor(Duration::from_secs(30))
        }
    }

    fn shutdown(&mut self) {
        self.save_settings();
        self.flush_files();
    }
}
