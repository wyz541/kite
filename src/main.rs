#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;

mod app;
mod fs;
mod table;
mod vcs;

use app::KiteApp;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

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
                cx.new(|cx| KiteApp::new(window, cx))
            })
            .expect("failed to open Kite window");
            cx.activate(true);
        });
}
