use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::keymap::{self, Keymap};
use crate::preferences::config::{Preferences, ThemeVariant};
use crate::preferences::profiles::ProfilesPage;
use crate::profile::{ColorSchemeStore, ProfileStore};

/// Builds the Preferences window, mirroring Tilix's sidebar of categories
/// (`AdwPreferencesWindow` gives us that navigation for free — one page per
/// category). "General" and "Profiles" have real, wired-up settings; the
/// rest are placeholders for features Rutile doesn't have yet (bookmarks,
/// encoding, ...) — see `docs/ROADMAP.md`.
pub fn build(
    parent: &adw::ApplicationWindow,
    prefs: Rc<RefCell<Preferences>>,
    profiles: Rc<RefCell<ProfileStore>>,
    schemes: &Rc<ColorSchemeStore>,
    keymap: Rc<RefCell<Keymap>>,
) -> adw::PreferencesWindow {
    let window = adw::PreferencesWindow::builder()
        .title("Preferences")
        .transient_for(parent)
        .modal(true)
        .build();

    let prefs_for_appearance = prefs.clone();
    window.add(&general_page(prefs.clone()));
    let profiles_page = ProfilesPage::new(&window, prefs, profiles, schemes.clone());
    window.add(profiles_page.page());
    // `ProfilesPage`'s own callbacks only hold weak refs to it (it holds
    // the widgets they're connected to, strong refs would be a cycle), so
    // something else has to own it for as long as the window lives.
    window.connect_destroy(move |_| {
        let _ = &profiles_page;
    });
    window.add(&appearance_page(prefs_for_appearance));
    window.add(&placeholder_page(
        "Bookmarks",
        "user-bookmarks-symbolic",
        "Planned (roadmap phase 4): command bookmarks organized in folders, inserted into the focused terminal.",
    ));
    window.add(ShortcutsPage::new(keymap).page());
    window.add(&placeholder_page(
        "Encoding",
        "text-x-generic-symbolic",
        "Rutile currently only supports UTF-8. Planned (roadmap phase 2): per-profile encoding and CJK ambiguous width.",
    ));
    window.add(&placeholder_page(
        "Advanced",
        "applications-system-symbolic",
        "Planned (roadmap phase 5): regex triggers and unsafe-paste warnings.",
    ));

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

    let copy_on_select_row = adw::SwitchRow::builder()
        .title("Copy on select")
        .subtitle("Automatically copy the selected text to the clipboard")
        .active(prefs.borrow().copy_on_select)
        .build();
    {
        let prefs = prefs.clone();
        copy_on_select_row.connect_active_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            prefs.copy_on_select = row.is_active();
            prefs.save();
        });
    }
    behavior_group.add(&copy_on_select_row);

    let hyperlinks_row = adw::SwitchRow::builder()
        .title("Enable hyperlinks")
        .subtitle("Ctrl+Click opens OSC 8 links and filesystem paths in terminal output")
        .active(prefs.borrow().enable_hyperlinks)
        .build();
    {
        let prefs = prefs.clone();
        hyperlinks_row.connect_active_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            prefs.enable_hyperlinks = row.is_active();
            prefs.save();
        });
    }
    behavior_group.add(&hyperlinks_row);

    let notifications_row = adw::SwitchRow::builder()
        .title("Enable notifications")
        .subtitle("Desktop notification on bell, or when a pane goes quiet after activity")
        .active(prefs.borrow().enable_notifications)
        .build();
    {
        let prefs = prefs.clone();
        notifications_row.connect_active_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            prefs.enable_notifications = row.is_active();
            prefs.save();
        });
    }
    behavior_group.add(&notifications_row);

    let prompt_on_close_row = adw::SwitchRow::builder()
        .title("Prompt before closing a running process")
        .subtitle("Confirm before closing a pane, session, or window with an active process")
        .active(prefs.borrow().prompt_on_close_with_process)
        .build();
    {
        let prefs = prefs.clone();
        prompt_on_close_row.connect_active_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            prefs.prompt_on_close_with_process = row.is_active();
            prefs.save();
        });
    }
    behavior_group.add(&prompt_on_close_row);

    page.add(&behavior_group);
    page
}

/// The "Appearance" page. Only the theme variant for now — the rest of
/// Tilix's Appearance settings land in their own changes (see the
/// group's description).
fn appearance_page(prefs: Rc<RefCell<Preferences>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Appearance")
        .icon_name("view-grid-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title("Theme")
        .description(
            "Still to come (roadmap phase 2): background transparency and image, \
             window style, sidebar position.",
        )
        .build();

    let current = prefs.borrow().theme_variant;
    let row = adw::ComboRow::builder()
        .title("Theme variant")
        .subtitle("Terminal colors come from each profile's color scheme")
        .model(&gtk4::StringList::new(
            &ThemeVariant::ALL.map(ThemeVariant::label),
        ))
        .build();
    if let Some(index) = ThemeVariant::ALL.iter().position(|v| *v == current) {
        row.set_selected(index as u32);
    }
    row.connect_selected_notify(move |row| {
        let Some(&variant) = ThemeVariant::ALL.get(row.selected() as usize) else {
            return;
        };
        {
            let mut prefs = prefs.borrow_mut();
            prefs.theme_variant = variant;
            prefs.save();
        }
        crate::app::load_theme(variant);
    });
    group.add(&row);

    page.add(&group);
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

/// The "Shortcuts" page: one row per `Action`, showing its current chord and
/// an "Edit" button that captures the next key combo pressed and rebinds it
/// (`Keymap::rebind`, persisted immediately — same "no separate Apply
/// button" convention as every other preference here). `Rc<Self>` for the
/// same reason as `ProfilesPage`: rebinding one row can free up (or steal)
/// another row's chord, so the whole list re-renders from `Keymap::entries`
/// after every change instead of patching a single row in place.
struct ShortcutsPage {
    page: adw::PreferencesPage,
    group: adw::PreferencesGroup,
    rows: RefCell<Vec<adw::ActionRow>>,
    keymap: Rc<RefCell<Keymap>>,
}

impl ShortcutsPage {
    fn new(keymap: Rc<RefCell<Keymap>>) -> Rc<Self> {
        let page = adw::PreferencesPage::builder()
            .title("Shortcuts")
            .icon_name("input-keyboard-symbolic")
            .build();
        let group = adw::PreferencesGroup::builder()
            .title("Keybindings")
            .description("Click Edit, then press the new key combination.")
            .build();
        page.add(&group);

        let this = Rc::new(Self {
            page,
            group,
            rows: RefCell::new(Vec::new()),
            keymap,
        });
        this.rebuild();
        this
    }

    fn page(&self) -> &adw::PreferencesPage {
        &self.page
    }

    fn rebuild(self: &Rc<Self>) {
        for row in self.rows.borrow_mut().drain(..) {
            self.group.remove(&row);
        }

        for (action, chord) in self.keymap.borrow().entries() {
            let row = adw::ActionRow::builder()
                .title(keymap::action_label(action))
                .build();

            let edit_button = gtk4::Button::builder()
                .label(chord)
                .valign(gtk4::Align::Center)
                .build();
            {
                let this = self.clone();
                let edit_button_for_click = edit_button.clone();
                edit_button.connect_clicked(move |button| {
                    button.set_label("Press a key…");
                    let controller = gtk4::EventControllerKey::new();
                    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
                    let this = this.clone();
                    let edit_button_for_key = edit_button_for_click.clone();
                    controller.connect_key_pressed(move |controller, key, _keycode, state| {
                        if key == gtk4::gdk::Key::Escape {
                            edit_button_for_key.remove_controller(controller);
                            this.rebuild();
                            return gtk4::glib::Propagation::Stop;
                        }
                        if is_pure_modifier(key) {
                            return gtk4::glib::Propagation::Proceed;
                        }
                        this.keymap.borrow_mut().rebind(action, key, state);
                        this.keymap.borrow().save();
                        edit_button_for_key.remove_controller(controller);
                        this.rebuild();
                        gtk4::glib::Propagation::Stop
                    });
                    edit_button_for_click.add_controller(controller);
                    edit_button_for_click.grab_focus();
                });
            }
            row.add_suffix(&edit_button);
            self.group.add(&row);
            self.rows.borrow_mut().push(row);
        }
    }
}

fn is_pure_modifier(key: gtk4::gdk::Key) -> bool {
    matches!(
        key,
        gtk4::gdk::Key::Control_L
            | gtk4::gdk::Key::Control_R
            | gtk4::gdk::Key::Shift_L
            | gtk4::gdk::Key::Shift_R
            | gtk4::gdk::Key::Alt_L
            | gtk4::gdk::Key::Alt_R
            | gtk4::gdk::Key::Super_L
            | gtk4::gdk::Key::Super_R
            | gtk4::gdk::Key::Meta_L
            | gtk4::gdk::Key::Meta_R
            | gtk4::gdk::Key::Caps_Lock
    )
}
