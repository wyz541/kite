#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod actions;

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod context_menu;
#[cfg(windows)]
mod explorer_integration;
#[cfg(windows)]
mod preview_window;
#[cfg(windows)]
mod shell_fs;
#[cfg(windows)]
mod table;
#[cfg(windows)]
mod tabs;

#[cfg(windows)]
use app::KiteApp;
#[cfg(windows)]
use gpui_kit::component::{Theme, ThemeMode};
#[cfg(windows)]
use gpui_kit::*;
#[cfg(windows)]
pub use kite_core::{devices, fs, platform, session, updater, vcs};
#[cfg(windows)]
use tabs::KiteWindow;

#[cfg(windows)]
fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            KiteApp::bind_keys(cx);
            KiteApp::install_menus(cx);

            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Kite".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::centered(size(px(1180.), px(760.)), cx)),
                ..Default::default()
            };

            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| KiteWindow::new(window, cx))
            })
            .expect("failed to open Kite window");
            cx.activate(true);
        });
}

#[cfg(not(windows))]
fn main() {}
