mod app;
mod clipboard;
mod config;
mod hotkey;
mod icon;
mod macos;
mod store;
mod tray;

use eframe::egui::{Vec2, ViewportBuilder, WindowLevel};
use eframe::{NativeOptions, Renderer};

use crate::app::ClipiApp;
use crate::config::Settings;

fn main() -> eframe::Result<()> {
    let settings = Settings::load();
    let viewport = ViewportBuilder::default()
        .with_title("clipi")
        .with_inner_size(Vec2::new(440.0, 560.0))
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(false)
        .with_always_on_top()
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_taskbar(false)
        .with_icon(crate::icon::window_icon());

    let options = NativeOptions {
        viewport,
        persist_window: false,
        renderer: Renderer::Glow,
        centered: true,
        ..NativeOptions::default()
    };

    eframe::run_native(
        "clipi",
        options,
        Box::new(|cc| {
            match ClipiApp::new(cc, settings) {
                Ok(app) => Ok(Box::new(app)),
                Err(err) => {
                    eprintln!("clipi: {err}");
                    Err(err.into())
                }
            }
        }),
    )
}
