use std::cell::RefCell;
use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

use crate::preferences::config::Preferences;

/// Builds the Preferences window, mirroring Tilix's sidebar of categories
/// (`AdwPreferencesWindow` gives us that navigation for free — one page per
/// category). Only "General" has real, wired-up settings for now; the rest
/// are placeholders for features Rutile doesn't have yet (profiles,
/// bookmarks, encoding, ...) — see GUIDELINE.md's v0.2+ list.
pub fn build(
    parent: &adw::ApplicationWindow,
    prefs: Rc<RefCell<Preferences>>,
) -> adw::PreferencesWindow {
    let window = adw::PreferencesWindow::builder()
        .title("Preferences")
        .transient_for(parent)
        .modal(true)
        .build();

    window.add(&general_page(prefs));
    window.add(&placeholder_page(
        "Appearance",
        "view-grid-symbolic",
        "Theme variants (Catppuccin Latte, custom palettes) are planned for a future version.",
    ));
    window.add(&placeholder_page(
        "Bookmarks",
        "user-bookmarks-symbolic",
        "Rutile has no bookmarks concept yet.",
    ));
    window.add(&placeholder_page("Shortcuts", "input-keyboard-symbolic", "Keybindings are currently a fixed Tilix-parity table (see keymap.rs); making them configurable is planned for a future version."));
    window.add(&placeholder_page(
        "Encoding",
        "text-x-generic-symbolic",
        "Rutile currently only supports UTF-8.",
    ));
    window.add(&placeholder_page(
        "Advanced",
        "applications-system-symbolic",
        "No advanced settings yet.",
    ));
    window.add(&placeholder_page("Profiles", "avatar-default-symbolic", "Rutile has no per-profile configuration yet — all terminals share the same Catppuccin Mocha theme and behavior."));

    window
}

fn general_page(prefs: Rc<RefCell<Preferences>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("General")
        .icon_name("preferences-system-symbolic")
        .build();

    let behavior_group = adw::PreferencesGroup::builder().title("Behavior").build();

    let focus_row = adw::SwitchRow::builder()
        .title("Focus terminal on mouse hover")
        .subtitle("Move the pointer over a pane to focus it, without clicking")
        .active(prefs.borrow().focus_follows_mouse)
        .build();
    {
        let prefs = prefs.clone();
        focus_row.connect_active_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            prefs.focus_follows_mouse = row.is_active();
            prefs.save();
        });
    }
    behavior_group.add(&focus_row);

    let close_window_row = adw::SwitchRow::builder()
        .title("Close window when last session is closed")
        .active(prefs.borrow().close_window_on_last_session_closed)
        .build();
    {
        let prefs = prefs.clone();
        close_window_row.connect_active_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            prefs.close_window_on_last_session_closed = row.is_active();
            prefs.save();
        });
    }
    behavior_group.add(&close_window_row);

    page.add(&behavior_group);
    page
}

fn placeholder_page(title: &str, icon_name: &str, message: &str) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(title)
        .icon_name(icon_name)
        .build();

    let status = adw::StatusPage::builder()
        .icon_name(icon_name)
        .title("Coming later")
        .description(message)
        .vexpand(true)
        .build();

    let group = adw::PreferencesGroup::new();
    group.add(&status);
    page.add(&group);
    page
}
