use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::i18n::{n_, tr};
use crate::keymap::{self, Keymap};
use crate::preferences::bookmarks::BookmarksPage;
use crate::preferences::config::{
    BackgroundImageMode, NewInstanceMode, Preferences, SidebarPosition, ThemeVariant, WindowStyle,
};
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
        .title(tr("Preferences"))
        .transient_for(parent)
        .modal(true)
        .build();

    let prefs_for_appearance = prefs.clone();
    let prefs_for_bookmarks = prefs.clone();
    let prefs_for_advanced = prefs.clone();
    window.add(&general_page(prefs.clone()));
    let profiles_page = ProfilesPage::new(&window, prefs, profiles, schemes.clone());
    window.add(profiles_page.page());
    window.add(&appearance_page(prefs_for_appearance));
    let bookmarks_page = BookmarksPage::new(prefs_for_bookmarks);
    window.add(bookmarks_page.page());
    // `ProfilesPage`'s and `BookmarksPage`'s own callbacks only hold weak
    // refs to them (they hold the widgets those are connected to, strong
    // refs would be a cycle), so something else has to own them for as
    // long as the window lives.
    window.connect_destroy(move |_| {
        let _ = (&profiles_page, &bookmarks_page);
    });
    window.add(ShortcutsPage::new(keymap).page());
    window.add(&placeholder_page(
        tr("Encoding"),
        "text-x-generic-symbolic",
        tr(
            "Choosing which encodings the terminal menu offers is planned. The encoding itself is already set per profile: Profiles → edit a profile → Compatibility.",
        ),
    ));
    window.add(&advanced_page(prefs_for_advanced));

    window
}

fn general_page(prefs: Rc<RefCell<Preferences>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(tr("General"))
        .icon_name("preferences-system-symbolic")
        .build();

    let behavior_group = adw::PreferencesGroup::builder()
        .title(tr("Behavior"))
        .build();

    let focus_row = adw::SwitchRow::builder()
        .title(tr("Focus terminal on mouse hover"))
        .subtitle(tr(
            "Move the pointer over a pane to focus it, without clicking",
        ))
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
        .title(tr("Close window when last session is closed"))
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
        .title(tr("Copy on select"))
        .subtitle(tr("Automatically copy the selected text to the clipboard"))
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
        .title(tr("Enable hyperlinks"))
        .subtitle(tr(
            "Ctrl+Click opens OSC 8 links and filesystem paths in terminal output",
        ))
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
        .title(tr("Enable notifications"))
        .subtitle(tr(
            "Desktop notification on bell, or when a pane goes quiet after activity",
        ))
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
        .title(tr("Prompt before closing a running process"))
        .subtitle(tr(
            "Confirm before closing a pane, session, or window with an active process",
        ))
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

    let current = prefs.borrow().new_instance_mode;
    let new_instance_row = adw::ComboRow::builder()
        .title(tr("On new instance"))
        .subtitle(tr(
            "When rutile is launched again while running (overridden by --action)",
        ))
        .model(&gtk4::StringList::new(
            &NewInstanceMode::ALL.map(NewInstanceMode::label),
        ))
        .build();
    if let Some(index) = NewInstanceMode::ALL.iter().position(|m| *m == current) {
        new_instance_row.set_selected(index as u32);
    }
    {
        let prefs = prefs.clone();
        new_instance_row.connect_selected_notify(move |row| {
            if let Some(&mode) = NewInstanceMode::ALL.get(row.selected() as usize) {
                let mut prefs = prefs.borrow_mut();
                prefs.new_instance_mode = mode;
                prefs.save();
            }
        });
    }
    behavior_group.add(&new_instance_row);

    page.add(&behavior_group);
    page.add(&paste_group(prefs));
    page
}

/// The "Advanced" page: the global triggers and how much output they read.
fn advanced_page(prefs: Rc<RefCell<Preferences>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(tr("Advanced"))
        .icon_name("applications-system-symbolic")
        .build();

    let initial = prefs.borrow().triggers.clone();
    {
        let prefs = prefs.clone();
        page.add(&super::triggers::group(
            tr("Triggers"),
            tr(
                "Regular expressions matched against every terminal's output, before the profile's own triggers. $0 is the whole match, $1 to $9 its groups.",
            ),
            initial,
            move |triggers| {
                {
                    let mut prefs = prefs.borrow_mut();
                    prefs.triggers = triggers;
                    prefs.save();
                }
                crate::window::reload_triggers_everywhere();
            },
        ));
    }

    let limits = adw::PreferencesGroup::builder()
        .title(tr("Trigger processing"))
        .description(
            "Rows read each time a terminal's content changes. Output faster than \
             that is partly skipped instead of slowing the terminal down.",
        )
        .build();
    let (lines, unlimited) = {
        let prefs = prefs.borrow();
        (prefs.trigger_lines, prefs.trigger_unlimited_lines)
    };
    let lines_row = adw::SpinRow::builder()
        .title(tr("Rows checked per change"))
        .adjustment(&gtk4::Adjustment::new(
            f64::from(lines),
            1.0,
            100_000.0,
            1.0,
            64.0,
            0.0,
        ))
        .sensitive(!unlimited)
        .build();
    let unlimited_row = adw::SwitchRow::builder()
        .title(tr("No limit"))
        .subtitle(tr(
            "Every row is checked: heavy output gets slower with an unlimited scrollback",
        ))
        .active(unlimited)
        .build();
    {
        let prefs = prefs.clone();
        lines_row.connect_value_notify(move |row| {
            {
                let mut prefs = prefs.borrow_mut();
                prefs.trigger_lines = row.value() as u32;
                prefs.save();
            }
            crate::window::reload_triggers_everywhere();
        });
    }
    {
        let lines_row = lines_row.clone();
        unlimited_row.connect_active_notify(move |row| {
            lines_row.set_sensitive(!row.is_active());
            {
                let mut prefs = prefs.borrow_mut();
                prefs.trigger_unlimited_lines = row.is_active();
                prefs.save();
            }
            crate::window::reload_triggers_everywhere();
        });
    }
    limits.add(&lines_row);
    limits.add(&unlimited_row);
    page.add(&limits);
    page
}

/// Picks one `bool` field out of the preferences, for a switch row.
type BoolField = fn(&mut Preferences) -> &mut bool;

/// Tilix's paste settings (see `dialogs::paste`).
fn paste_group(prefs: Rc<RefCell<Preferences>>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(tr("Paste")).build();
    let rows: [(&'static str, &'static str, BoolField); 4] = [
        (
            n_("Warn before an unsafe paste"),
            n_("Several lines that use sudo"),
            |p| &mut p.unsafe_paste_alert,
        ),
        (
            n_("Strip a leading # or $"),
            n_("A command copied with its prompt, or commented out"),
            |p| &mut p.paste_strip_first_char,
        ),
        (
            n_("Strip trailing whitespace"),
            n_("Including the final line break, so the command doesn't run right away"),
            |p| &mut p.paste_strip_trailing_whitespace,
        ),
        (
            n_("Advanced paste by default"),
            n_("Review and transform multi-line text before pasting it"),
            |p| &mut p.paste_advanced_default,
        ),
    ];
    for (title, subtitle, field) in rows {
        let row = adw::SwitchRow::builder()
            .title(tr(title))
            .subtitle(tr(subtitle))
            .active(*field(&mut prefs.borrow_mut()))
            .build();
        let prefs = prefs.clone();
        row.connect_active_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            *field(&mut prefs) = row.is_active();
            prefs.save();
        });
        group.add(&row);
    }
    group
}

/// The "Appearance" page: theme variant, background image, window style
/// and pane layout.
fn appearance_page(prefs: Rc<RefCell<Preferences>>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(tr("Appearance"))
        .icon_name("view-grid-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder().title(tr("Theme")).build();

    let current = prefs.borrow().theme_variant;
    let row = adw::ComboRow::builder()
        .title(tr("Theme variant"))
        .subtitle(tr("Terminal colors come from each profile's color scheme"))
        .model(&gtk4::StringList::new(
            &ThemeVariant::ALL.map(ThemeVariant::label),
        ))
        .build();
    if let Some(index) = ThemeVariant::ALL.iter().position(|v| *v == current) {
        row.set_selected(index as u32);
    }
    {
        let prefs = prefs.clone();
        row.connect_selected_notify(move |row| {
            let Some(&variant) = ThemeVariant::ALL.get(row.selected() as usize) else {
                return;
            };
            update_appearance(&prefs, |p| p.theme_variant = variant);
        });
    }
    group.add(&row);
    page.add(&group);

    page.add(&background_image_group(prefs.clone()));
    page.add(&window_group(prefs.clone()));
    page.add(&panes_group(prefs));
    page
}

/// Splitter handles and lone-pane headers.
fn panes_group(prefs: Rc<RefCell<Preferences>>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(tr("Panes")).build();

    let wide_row = adw::SwitchRow::builder()
        .title(tr("Wide splitter handles"))
        .subtitle(tr("Easier to grab with the mouse"))
        .active(prefs.borrow().wide_handles)
        .build();
    {
        let prefs = prefs.clone();
        wide_row.connect_active_notify(move |row| {
            update_appearance(&prefs, |p| p.wide_handles = row.is_active());
        });
    }
    group.add(&wide_row);

    let title_row = adw::SwitchRow::builder()
        .title(tr("Show the title of a lone pane"))
        .subtitle(tr(
            "Otherwise a session with a single pane hides its title bar",
        ))
        .active(prefs.borrow().show_title_when_single)
        .build();
    title_row.connect_active_notify(move |row| {
        update_appearance(&prefs, |p| p.show_title_when_single = row.is_active());
    });
    group.add(&title_row);

    group
}

/// Window style and sidebar position.
fn window_group(prefs: Rc<RefCell<Preferences>>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder().title(tr("Window")).build();

    let current = prefs.borrow().window_style;
    let style_row = adw::ComboRow::builder()
        .title(tr("Window style"))
        .subtitle(tr("Without the toolbar, Preferences stay reachable with Ctrl+Comma and the terminal's right-click menu"))
        .model(&gtk4::StringList::new(&WindowStyle::ALL.map(WindowStyle::label)))
        .build();
    if let Some(index) = WindowStyle::ALL.iter().position(|s| *s == current) {
        style_row.set_selected(index as u32);
    }
    {
        let prefs = prefs.clone();
        style_row.connect_selected_notify(move |row| {
            if let Some(&style) = WindowStyle::ALL.get(row.selected() as usize) {
                update_appearance(&prefs, |p| p.window_style = style);
            }
        });
    }
    group.add(&style_row);

    let current = prefs.borrow().sidebar_position;
    let sidebar_row = adw::ComboRow::builder()
        .title(tr("Sidebar position"))
        .model(&gtk4::StringList::new(
            &SidebarPosition::ALL.map(SidebarPosition::label),
        ))
        .build();
    if let Some(index) = SidebarPosition::ALL.iter().position(|s| *s == current) {
        sidebar_row.set_selected(index as u32);
    }
    sidebar_row.connect_selected_notify(move |row| {
        if let Some(&position) = SidebarPosition::ALL.get(row.selected() as usize) {
            update_appearance(&prefs, |p| p.sidebar_position = position);
        }
    });
    group.add(&sidebar_row);

    group
}

/// Saves an Appearance preference and applies it right away.
fn update_appearance(prefs: &Rc<RefCell<Preferences>>, edit: impl FnOnce(&mut Preferences)) {
    {
        let mut prefs = prefs.borrow_mut();
        edit(&mut prefs);
        prefs.save();
    }
    let prefs = prefs.borrow();
    crate::app::load_theme(&prefs);
    crate::window::apply_window_preferences_to_all(&prefs);
}

/// Tilix's background image: a file picker, a clear button and the fill
/// mode. Global, not per profile — like Tilix — and only visible through
/// terminals whose profile has some background transparency.
fn background_image_group(prefs: Rc<RefCell<Preferences>>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(tr("Background image"))
        .description(tr(
            "Shown through terminals whose profile has some background transparency.",
        ))
        .build();

    let image_row = adw::ActionRow::builder().title(tr("Image")).build();
    let describe = |path: Option<&std::path::Path>| match path {
        Some(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        None => tr("None").to_string(),
    };
    image_row.set_subtitle(&describe(prefs.borrow().background_image.as_deref()));

    let choose = gtk4::Button::builder()
        .label(tr("Choose…"))
        .valign(gtk4::Align::Center)
        .build();
    let clear = gtk4::Button::builder()
        .icon_name("edit-clear-symbolic")
        .tooltip_text(tr("Remove the background image"))
        .valign(gtk4::Align::Center)
        .sensitive(prefs.borrow().background_image.is_some())
        .build();
    clear.add_css_class("flat");
    image_row.add_suffix(&choose);
    image_row.add_suffix(&clear);
    {
        let prefs = prefs.clone();
        let image_row = image_row.downgrade();
        let clear = clear.downgrade();
        choose.connect_clicked(move |button| {
            let filter = gtk4::FileFilter::new();
            filter.set_name(Some(tr("Images")));
            filter.add_pixbuf_formats();
            let filters = gtk4::gio::ListStore::new::<gtk4::FileFilter>();
            filters.append(&filter);
            let dialog = gtk4::FileDialog::builder()
                .title(tr("Background image"))
                .filters(&filters)
                .default_filter(&filter)
                .build();
            let parent = button.root().and_downcast::<gtk4::Window>();
            let prefs = prefs.clone();
            let (image_row, clear) = (image_row.clone(), clear.clone());
            dialog.open(
                parent.as_ref(),
                gtk4::gio::Cancellable::NONE,
                move |result| {
                    let Some(path) = result.ok().and_then(|file| file.path()) else {
                        return;
                    };
                    if let Some(row) = image_row.upgrade() {
                        row.set_subtitle(&describe(Some(&path)));
                    }
                    if let Some(clear) = clear.upgrade() {
                        clear.set_sensitive(true);
                    }
                    update_appearance(&prefs, |p| p.background_image = Some(path));
                },
            );
        });
    }
    {
        let prefs = prefs.clone();
        let image_row = image_row.downgrade();
        clear.connect_clicked(move |button| {
            button.set_sensitive(false);
            if let Some(row) = image_row.upgrade() {
                row.set_subtitle(&describe(None));
            }
            update_appearance(&prefs, |p| p.background_image = None);
        });
    }
    group.add(&image_row);

    let current = prefs.borrow().background_image_mode;
    let mode_row = adw::ComboRow::builder()
        .title(tr("Mode"))
        .model(&gtk4::StringList::new(
            &BackgroundImageMode::ALL.map(BackgroundImageMode::label),
        ))
        .build();
    if let Some(index) = BackgroundImageMode::ALL.iter().position(|m| *m == current) {
        mode_row.set_selected(index as u32);
    }
    mode_row.connect_selected_notify(move |row| {
        if let Some(&mode) = BackgroundImageMode::ALL.get(row.selected() as usize) {
            update_appearance(&prefs, |p| p.background_image_mode = mode);
        }
    });
    group.add(&mode_row);

    group
}

fn placeholder_page(title: &str, icon_name: &str, message: &str) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(title)
        .icon_name(icon_name)
        .build();

    let status = adw::StatusPage::builder()
        .icon_name(icon_name)
        .title(tr("Coming later"))
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
            .title(tr("Shortcuts"))
            .icon_name("input-keyboard-symbolic")
            .build();
        let group = adw::PreferencesGroup::builder()
            .title(tr("Keybindings"))
            .description(tr(
                "Click a shortcut, then press the new key combination. Escape cancels.",
            ))
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
                    button.set_label(tr("Press a key…"));
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
