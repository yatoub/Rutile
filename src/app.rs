use std::cell::{OnceCell, RefCell};

use gtk4::prelude::*;
use libadwaita as adw;

use crate::preferences::{Preferences, ThemeVariant};
use crate::window;

pub fn build(app: &adw::Application) {
    load_theme(&Preferences::load());

    let win = window::build_window(app);
    win.present();
}

thread_local! {
    /// The chrome stylesheet, installed on the display once and reloaded
    /// in place when the light/dark variant or the background image
    /// changes (`build` runs on every activation, so it mustn't stack a
    /// new provider each time).
    static CHROME: OnceCell<gtk4::CssProvider> = const { OnceCell::new() };
    /// The background image rule last built from Preferences, kept so a
    /// light/dark switch coming from the desktop (`connect_dark_notify`)
    /// reloads it along with the palette.
    static BACKGROUND_CSS: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Installs Rutile's chrome stylesheet (first call) and applies the theme
/// variant and background image from `prefs` — also called by Preferences
/// when the user changes either, which takes effect immediately in every
/// window.
pub fn load_theme(prefs: &Preferences) {
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

    BACKGROUND_CSS.with(|css| css.replace(background_css(prefs)));
    style_manager.set_color_scheme(match prefs.theme_variant {
        ThemeVariant::System => adw::ColorScheme::Default,
        ThemeVariant::Light => adw::ColorScheme::ForceLight,
        ThemeVariant::Dark => adw::ColorScheme::ForceDark,
    });
    // `set_color_scheme` only notifies when `dark` actually changes, so
    // the first load (or re-picking the current variant, or a background
    // image change) is done here.
    reload_chrome(style_manager.is_dark());
}

/// The background image rule for the main window, or nothing when no
/// image is set or the file is gone (GTK would only log a CSS error and
/// skip it anyway). The path goes through `gio::File::uri()`, whose
/// percent-encoding also keeps quotes out of the CSS string.
fn background_css(prefs: &Preferences) -> String {
    let Some(path) = prefs.background_image.as_ref().filter(|p| p.is_file()) else {
        return String::new();
    };
    let uri = gtk4::gio::File::for_path(path).uri();
    format!(
        "window.rutile-window.background {{ background-image: url(\"{uri}\"); {} }}\n",
        prefs.background_image_mode.css()
    )
}

/// The palette's `@define-color` tokens, then the layout that uses them,
/// then the background image rule, in a single stylesheet.
fn reload_chrome(dark: bool) {
    let palette = if dark {
        include_str!("../resources/catppuccin-mocha.css")
    } else {
        include_str!("../resources/catppuccin-latte.css")
    };
    let background = BACKGROUND_CSS.with(|css| css.borrow().clone());
    let css = format!(
        "{palette}\n{}\n{background}",
        include_str!("../resources/chrome.css")
    );
    CHROME.with(|chrome| {
        if let Some(provider) = chrome.get() {
            provider.load_from_string(&css);
        }
    });
}
