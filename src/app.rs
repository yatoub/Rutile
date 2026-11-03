use std::cell::OnceCell;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::preferences::{Preferences, ThemeVariant};
use crate::window;

pub fn build(app: &adw::Application) {
    load_theme(Preferences::load().theme_variant);

    let win = window::build_window(app);
    win.present();
}

thread_local! {
    /// The chrome stylesheet, installed on the display once and reloaded
    /// in place when the light/dark variant changes (`build` runs on every
    /// activation, so it mustn't stack a new provider each time).
    static CHROME: OnceCell<gtk4::CssProvider> = const { OnceCell::new() };
}

/// Installs Rutile's chrome stylesheet (first call) and applies `variant`
/// — also called by Preferences when the user changes it, which takes
/// effect immediately in every window.
pub fn load_theme(variant: ThemeVariant) {
    let style_manager = adw::StyleManager::default();
    CHROME.with(|chrome| {
        chrome.get_or_init(|| {
            let provider = gtk4::CssProvider::new();
            gtk4::style_context_add_provider_for_display(
                &gtk4::gdk::Display::default().expect("default display"),
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            // Follows libadwaita's own light/dark state, which also
            // changes on its own under `ThemeVariant::System` when the
            // desktop switches.
            style_manager.connect_dark_notify(|manager| reload_chrome(manager.is_dark()));
            provider
        });
    });

    style_manager.set_color_scheme(match variant {
        ThemeVariant::System => adw::ColorScheme::Default,
        ThemeVariant::Light => adw::ColorScheme::ForceLight,
        ThemeVariant::Dark => adw::ColorScheme::ForceDark,
    });
    // `set_color_scheme` only notifies when `dark` actually changes, so
    // the first load (or re-picking the current variant) is done here.
    reload_chrome(style_manager.is_dark());
}

/// The palette's `@define-color` tokens, then the layout that uses them,
/// in a single stylesheet.
fn reload_chrome(dark: bool) {
    let palette = if dark {
        include_str!("../resources/catppuccin-mocha.css")
    } else {
        include_str!("../resources/catppuccin-latte.css")
    };
    let css = format!("{palette}\n{}", include_str!("../resources/chrome.css"));
    CHROME.with(|chrome| {
        if let Some(provider) = chrome.get() {
            provider.load_from_string(&css);
        }
    });
}
