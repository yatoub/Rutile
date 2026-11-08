use std::cell::{OnceCell, RefCell};

use gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use crate::cli::LaunchRequest;
use crate::i18n::tr_args;
use crate::preferences::{NewInstanceMode, Preferences, ThemeVariant};
use crate::profile::ProfileStore;
use crate::window;

/// `activate` without a command line (e.g. `gapplication launch`): a plain
/// launch.
pub fn build(app: &adw::Application) {
    launch(app, &LaunchRequest::default());
}

/// Every `rutile …` invocation, the first one and those handed over by
/// later ones while this instance runs (`HANDLES_COMMAND_LINE`). Messages
/// go to the *invoking* terminal through `cmdline`, not this process's
/// stderr.
pub fn command_line(
    app: &adw::Application,
    cmdline: &gio::ApplicationCommandLine,
) -> glib::ExitCode {
    let cwd = cmdline.cwd();
    let launch_request = match LaunchRequest::parse(&cmdline.arguments(), cwd.as_deref()) {
        Ok(request) => request,
        Err(err) => {
            // Also `--help`/`--version`, which exit successfully.
            let text = err.render().to_string();
            if err.exit_code() == 0 {
                cmdline.print_literal(&text);
            } else {
                cmdline.printerr_literal(&text);
            }
            return glib::ExitCode::from(err.exit_code().clamp(0, 255) as u8);
        }
    };
    if let Err(message) = validate(&launch_request) {
        cmdline.printerr_literal(&format!("rutile: {message}\n"));
        return glib::ExitCode::FAILURE;
    }
    for warning in &launch_request.warnings {
        cmdline.printerr_literal(&format!("{warning}\n"));
    }
    launch(app, &launch_request);
    glib::ExitCode::SUCCESS
}

/// Rejects what can't work before anything opens, so the mistake is
/// reported where the command was typed instead of silently falling back.
fn validate(request: &LaunchRequest) -> Result<(), String> {
    if let Some(dir) = &request.working_directory
        && !dir.is_dir()
    {
        return Err(tr_args(
            "{path}: not a directory",
            &[("path", &dir.display())],
        ));
    }
    if let Some(file) = &request.session_file
        && !file.is_file()
    {
        return Err(tr_args(
            "{path}: no such session file",
            &[("path", &file.display())],
        ));
    }
    if let Some(wanted) = &request.profile {
        let store = ProfileStore::load();
        let known = store.get(wanted).is_some()
            || store.iter().any(|p| p.name.eq_ignore_ascii_case(wanted));
        if !known {
            let names: Vec<&str> = store.iter().map(|p| p.name.as_str()).collect();
            return Err(tr_args(
                "unknown profile \u{201c}{name}\u{201d} (profiles: {profiles})",
                &[("name", wanted), ("profiles", &names.join(", "))],
            ));
        }
    }
    Ok(())
}

/// Opens what `request` asks for: a new window if none is open yet,
/// otherwise whatever `--action` (or the "On new instance" preference)
/// says, in the most recently focused window.
fn launch(app: &adw::Application, request: &LaunchRequest) {
    let prefs = Preferences::load();
    load_theme(&prefs);

    let existing = app
        .active_window()
        .and_downcast::<adw::ApplicationWindow>()
        .filter(|window| window.has_css_class("rutile-window"));
    let target = match existing {
        Some(existing) => {
            let mode = request.action.unwrap_or(prefs.new_instance_mode);
            if mode == NewInstanceMode::NewWindow {
                None
            } else {
                window::launch_into(&existing, request, mode);
                Some(existing)
            }
        }
        None => None,
    };
    let target = target.unwrap_or_else(|| window::build_window(app, request));
    target.present();
    window::apply_launch_window_state(&target, request);
    if request.preferences {
        let _ = WidgetExt::activate_action(&target, "win.preferences", None);
    }
}

thread_local! {
    /// The chrome stylesheet, installed on the display once and reloaded
    /// in place when the light/dark variant or the background image
    /// changes (`launch` runs for every invocation, so it mustn't stack a
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
    let uri = gio::File::for_path(path).uri();
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
