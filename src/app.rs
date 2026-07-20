use gtk4::prelude::*;
use libadwaita as adw;

use crate::window;

pub fn build(app: &adw::Application) {
    load_theme();

    let win = window::build_window(app);
    win.present();
}

fn load_theme() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(include_str!("../resources/catppuccin-mocha.css"));

    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("default display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
