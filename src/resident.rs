//! FastDisco outliving its window: the tray item, closing to the tray,
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
            id: "fastdisco",
            title: "FastDisco".into(),
            icon: icon,
            template_icon: None,
            themed_icon: false,
            menu: vec![
                MenuItem::action("show", "Abrir o FastDisco"),
                MenuItem::Separator,
                MenuItem::action("notifications", "Silenciar notificações"),
                MenuItem::Separator,
                MenuItem::action("quit", "Sair do FastDisco"),
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
            "FastDisco\n{count} {}",
            if count == 1 { "menção" } else { "menções" }
        ));
    } else {
        tray.set_icon(icon, None);
        tray.set_tooltip("FastDisco");
    }
}

/// The app's mark: a blurple rounded square with two chat bubbles.
pub fn icon(size: usize) -> Vec<u8> {
    draw(size, false)
}

fn icon_unread(size: usize) -> Vec<u8> {
    draw(size, true)
}

fn draw(size: usize, unread: bool) -> Vec<u8> {
    let s = size as f32;
    let mut pixels = vec![0u8; size * size * 4];
    let blurple = [0x58, 0x65, 0xf2];
    let inside_rounded = |x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32| -> f32 {
        // Signed distance to a rounded rectangle, negative inside.
        let cx = x.clamp(x0 + r, x1 - r);
        let cy = y.clamp(y0 + r, y1 - r);
        ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r
    };
    for py in 0..size {
        for px in 0..size {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let mut color = [0u8; 4];
            let body = inside_rounded(x, y, 0.0, 0.0, s, s, s * 0.28);
            if body < 0.5 {
                let alpha = (0.5 - body).clamp(0.0, 1.0);
                color = [blurple[0], blurple[1], blurple[2], (alpha * 255.0) as u8];
                // Two white bubbles, the back one offset.
                let back = inside_rounded(x, y, s * 0.36, s * 0.22, s * 0.80, s * 0.56, s * 0.08);
                let front = inside_rounded(x, y, s * 0.20, s * 0.38, s * 0.64, s * 0.74, s * 0.08);
                let tail = x > s * 0.24
                    && x < s * 0.36
                    && y > s * 0.70
                    && y < s * 0.84
                    && (y - s * 0.70) < (s * 0.36 - x);
                if front < 0.5 || tail || (back < 0.5 && front > 2.5) {
                    color = [0xff, 0xff, 0xff, color[3]];
                }
            }
            if unread {
                let d = ((x - s * 0.80).powi(2) + (y - s * 0.80).powi(2)).sqrt();
                if d < s * 0.22 {
                    color = [0xf2, 0x3f, 0x43, 0xff];
                }
            }
            pixels[(py * size + px) * 4..][..4].copy_from_slice(&color);
        }
    }
    pixels
}

impl App {
    /// Tray clicks and menu entries; `window` says whether one is open.
    pub fn tray_events(&mut self, ctx: Option<&egui::Context>) {
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

    /// Whether closing the window can leave FastDisco in the tray.
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
