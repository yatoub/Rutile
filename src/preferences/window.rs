use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::keymap::{self, Keymap};
use crate::preferences::config::Preferences;
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

    window.add(&general_page(prefs.clone()));
    window.add(ProfilesPage::new(prefs, profiles, schemes.clone()).page());
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
    window.add(ShortcutsPage::new(keymap).page());
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

/// The "Profiles" page: one `AdwPreferencesGroup` per profile (name, color
/// scheme picker, default toggle, clone/delete), plus an "Add profile" row.
/// `Rc<Self>` (not a free function) because every row's callback needs to
/// trigger a full `rebuild()` of the page afterwards — adding/renaming/
/// deleting a profile changes what every other row should show (e.g. only
/// one row's "default" switch can be on at a time), and rebuilding from
/// scratch is far simpler than patching individual rows in place (same
/// "just rebuild" tradeoff `PaneView`/`SessionSidebar` already make).
struct ProfilesPage {
    page: adw::PreferencesPage,
    groups: RefCell<Vec<adw::PreferencesGroup>>,
    prefs: Rc<RefCell<Preferences>>,
    profiles: Rc<RefCell<ProfileStore>>,
    schemes: Rc<ColorSchemeStore>,
}

impl ProfilesPage {
    fn new(
        prefs: Rc<RefCell<Preferences>>,
        profiles: Rc<RefCell<ProfileStore>>,
        schemes: Rc<ColorSchemeStore>,
    ) -> Rc<Self> {
        let page = adw::PreferencesPage::builder()
            .title("Profiles")
            .icon_name("avatar-default-symbolic")
            .build();

        let this = Rc::new(Self {
            page,
            groups: RefCell::new(Vec::new()),
            prefs,
            profiles,
            schemes,
        });
        this.rebuild();
        this
    }

    fn page(&self) -> &adw::PreferencesPage {
        &self.page
    }

    fn rebuild(self: &Rc<Self>) {
        for group in self.groups.borrow_mut().drain(..) {
            self.page.remove(&group);
        }

        let scheme_ids: Vec<String> = self.schemes.iter().map(|s| s.id.clone()).collect();
        let scheme_names: Vec<&str> = self.schemes.iter().map(|s| s.name.as_str()).collect();

        let profile_ids: Vec<String> = self
            .profiles
            .borrow()
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let can_delete = profile_ids.len() > 1;

        for profile_id in &profile_ids {
            let group =
                self.build_profile_group(profile_id, &scheme_ids, &scheme_names, can_delete);
            self.page.add(&group);
            self.groups.borrow_mut().push(group);
        }

        let add_group = adw::PreferencesGroup::new();
        let add_button = gtk4::Button::builder()
            .label("Add Profile")
            .icon_name("list-add-symbolic")
            .halign(gtk4::Align::Start)
            .build();
        {
            let this = self.clone();
            add_button.connect_clicked(move |_| {
                let scheme_id = this
                    .schemes
                    .iter()
                    .next()
                    .map(|s| s.id.clone())
                    .unwrap_or_else(|| "catppuccin-mocha".to_string());
                this.profiles.borrow_mut().create("New Profile", &scheme_id);
                this.rebuild();
            });
        }
        add_group.add(&add_button);
        self.page.add(&add_group);
        self.groups.borrow_mut().push(add_group);
    }

    fn build_profile_group(
        self: &Rc<Self>,
        profile_id: &str,
        scheme_ids: &[String],
        scheme_names: &[&str],
        can_delete: bool,
    ) -> adw::PreferencesGroup {
        let profile = self
            .profiles
            .borrow()
            .get(profile_id)
            .expect("profile_id was read from this same store a moment ago")
            .clone();
        let is_default = self.prefs.borrow().default_profile_id == profile.id;

        let group = adw::PreferencesGroup::builder()
            .title(profile.name.clone())
            .build();

        let name_row = adw::EntryRow::builder()
            .title("Name")
            .text(profile.name.clone())
            .show_apply_button(true)
            .build();
        {
            let this = self.clone();
            let profile_id = profile.id.clone();
            name_row.connect_apply(move |row| {
                let new_name = row.text();
                let new_name = new_name.trim();
                if !new_name.is_empty() {
                    this.profiles.borrow_mut().rename(&profile_id, new_name);
                }
                this.rebuild();
            });
        }
        group.add(&name_row);

        let scheme_row = adw::ComboRow::builder()
            .title("Color scheme")
            .model(&gtk4::StringList::new(scheme_names))
            .build();
        if let Some(index) = scheme_ids.iter().position(|id| *id == profile.scheme_id) {
            scheme_row.set_selected(index as u32);
        }
        {
            let this = self.clone();
            let profile_id = profile.id.clone();
            let scheme_ids = scheme_ids.to_vec();
            scheme_row.connect_selected_notify(move |row| {
                if let Some(scheme_id) = scheme_ids.get(row.selected() as usize) {
                    this.profiles
                        .borrow_mut()
                        .set_scheme(&profile_id, scheme_id);
                }
            });
        }
        group.add(&scheme_row);

        let default_row = adw::SwitchRow::builder()
            .title("Default profile")
            .subtitle("New sessions are created with this profile")
            .active(is_default)
            .build();
        {
            let this = self.clone();
            let profile_id = profile.id.clone();
            default_row.connect_active_notify(move |row| {
                // Also handles the user turning the *current* default off
                // directly: nothing gets written since `is_active()` is
                // false, and `rebuild()` snaps the switch straight back to
                // on — there must always be exactly one default profile.
                if row.is_active() {
                    let mut prefs = this.prefs.borrow_mut();
                    prefs.default_profile_id = profile_id.clone();
                    prefs.save();
                }
                this.rebuild();
            });
        }
        group.add(&default_row);

        let button_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        button_box.set_margin_top(6);
        button_box.set_margin_bottom(6);
        button_box.set_halign(gtk4::Align::End);

        let clone_button = gtk4::Button::builder().label("Clone").build();
        {
            let this = self.clone();
            let profile_id = profile.id.clone();
            clone_button.connect_clicked(move |_| {
                this.profiles.borrow_mut().clone_profile(&profile_id);
                this.rebuild();
            });
        }
        button_box.append(&clone_button);

        if can_delete {
            let delete_button = gtk4::Button::builder().label("Delete").build();
            delete_button.add_css_class("destructive-action");
            {
                let this = self.clone();
                let profile_id = profile.id.clone();
                delete_button.connect_clicked(move |_| {
                    let was_default = this.prefs.borrow().default_profile_id == profile_id;
                    this.profiles.borrow_mut().delete(&profile_id);
                    if was_default {
                        // The now-deleted profile was the default: fall
                        // back to whatever profile happens to be first
                        // rather than leaving `default_profile_id`
                        // dangling on an id nothing references anymore.
                        let fallback = this.profiles.borrow().iter().next().map(|p| p.id.clone());
                        if let Some(fallback) = fallback {
                            let mut prefs = this.prefs.borrow_mut();
                            prefs.default_profile_id = fallback;
                            prefs.save();
                        }
                    }
                    this.rebuild();
                });
            }
            button_box.append(&delete_button);
        }

        group.add(&button_box);
        group
    }
}
