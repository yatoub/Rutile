use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::preferences::config::Preferences;
use crate::profile::{
    BadgePosition, BellMode, CjkWidth, ColorPair, ColorSchemeStore, CursorBlink, CursorShape,
    ExitAction, Profile, ProfileColors, ProfileStore, TerminalOptions, TextBlink,
};

/// The "Profiles" page: one row per profile (name, scheme, a "Default"
/// badge) plus an "Add" button, each row opening that profile's editor as
/// a subpage of the Preferences window — Tilix's profile list + tabbed
/// profile editor. `Rc<Self>` because the editor's callbacks need to
/// refresh the list afterwards (a rename, a new default, a clone or a
/// delete all change what the list shows); the list is rebuilt from
/// scratch each time rather than patched in place (same "just rebuild"
/// tradeoff `PaneView`/`SessionSidebar` make).
pub struct ProfilesPage {
    page: adw::PreferencesPage,
    list: adw::PreferencesGroup,
    rows: RefCell<Vec<adw::ActionRow>>,
    /// Weak: the window owns this page, so a strong ref would be a cycle.
    window: glib::WeakRef<adw::PreferencesWindow>,
    prefs: Rc<RefCell<Preferences>>,
    profiles: Rc<RefCell<ProfileStore>>,
    schemes: Rc<ColorSchemeStore>,
}

impl ProfilesPage {
    pub fn new(
        window: &adw::PreferencesWindow,
        prefs: Rc<RefCell<Preferences>>,
        profiles: Rc<RefCell<ProfileStore>>,
        schemes: Rc<ColorSchemeStore>,
    ) -> Rc<Self> {
        let page = adw::PreferencesPage::builder()
            .title("Profiles")
            .icon_name("avatar-default-symbolic")
            .build();
        let list = adw::PreferencesGroup::builder()
            .title("Profiles")
            .description("New sessions use the default profile.")
            .build();
        page.add(&list);

        let this = Rc::new(Self {
            page,
            list,
            rows: RefCell::new(Vec::new()),
            window: window.downgrade(),
            prefs,
            profiles,
            schemes,
        });

        let add_button = gtk4::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text("Add Profile")
            .valign(gtk4::Align::Center)
            .build();
        add_button.add_css_class("flat");
        {
            let this = Rc::downgrade(&this);
            add_button.connect_clicked(move |_| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                let scheme_id = this
                    .schemes
                    .iter()
                    .next()
                    .map(|s| s.id.clone())
                    .unwrap_or_else(|| "catppuccin-mocha".to_string());
                let id = this.profiles.borrow_mut().create("New Profile", &scheme_id);
                this.rebuild();
                this.open_editor(&id);
            });
        }
        this.list.set_header_suffix(Some(&add_button));

        this.rebuild();
        this
    }

    pub fn page(&self) -> &adw::PreferencesPage {
        &self.page
    }

    fn rebuild(self: &Rc<Self>) {
        for row in self.rows.borrow_mut().drain(..) {
            self.list.remove(&row);
        }

        let default_id = self.prefs.borrow().default_profile_id.clone();
        let profiles: Vec<Profile> = self.profiles.borrow().iter().cloned().collect();
        for profile in profiles {
            let scheme_name = self.schemes.get_or_default(&profile.scheme_id).name.clone();
            let row = adw::ActionRow::builder()
                .title(profile.name.clone())
                .subtitle(scheme_name)
                .activatable(true)
                .build();
            if profile.id == default_id {
                let badge = gtk4::Label::builder()
                    .label("Default")
                    .valign(gtk4::Align::Center)
                    .build();
                badge.add_css_class("dim-label");
                row.add_suffix(&badge);
            }
            row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
            {
                let this = Rc::downgrade(self);
                let profile_id = profile.id.clone();
                row.connect_activated(move |_| {
                    if let Some(this) = this.upgrade() {
                        this.open_editor(&profile_id);
                    }
                });
            }
            self.list.add(&row);
            self.rows.borrow_mut().push(row);
        }
    }

    fn open_editor(self: &Rc<Self>, profile_id: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let Some(profile) = self.profiles.borrow().get(profile_id).cloned() else {
            return;
        };
        window.push_subpage(&self.editor(&profile));
    }

    /// One profile's editor: a header bar with a tab switcher over
    /// General / Command / Color / Scrolling pages, like Tilix's profile
    /// editor notebook. Every control saves immediately, like everywhere
    /// else in Preferences.
    fn editor(self: &Rc<Self>, profile: &Profile) -> adw::NavigationPage {
        let stack = adw::ViewStack::new();
        let navigation_page = adw::NavigationPage::builder()
            .title(profile.name.clone())
            .build();

        stack.add_titled_with_icon(
            &self.general_tab(profile, &navigation_page),
            Some("general"),
            "General",
            "preferences-system-symbolic",
        );
        stack.add_titled_with_icon(
            &self.command_tab(profile),
            Some("command"),
            "Command",
            "utilities-terminal-symbolic",
        );
        stack.add_titled_with_icon(
            &self.color_tab(profile),
            Some("color"),
            "Color",
            "applications-graphics-symbolic",
        );
        stack.add_titled_with_icon(
            &self.scrolling_tab(profile),
            Some("scrolling"),
            "Scrolling",
            "view-continuous-symbolic",
        );
        stack.add_titled_with_icon(
            &self.compatibility_tab(profile),
            Some("compatibility"),
            "Compatibility",
            "text-x-generic-symbolic",
        );
        stack.add_titled_with_icon(
            &self.badge_tab(profile),
            Some("badge"),
            "Badge",
            "text-x-generic-symbolic",
        );
        stack.add_titled_with_icon(
            &self.advanced_tab(profile),
            Some("advanced"),
            "Advanced",
            "applications-system-symbolic",
        );

        let header = adw::HeaderBar::builder()
            .title_widget(
                &adw::ViewSwitcher::builder()
                    .stack(&stack)
                    .policy(adw::ViewSwitcherPolicy::Wide)
                    .build(),
            )
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&stack));
        navigation_page.set_child(Some(&toolbar));
        navigation_page
    }

    fn general_tab(
        self: &Rc<Self>,
        profile: &Profile,
        navigation_page: &adw::NavigationPage,
    ) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::new();
        let options = &profile.options;

        let identity = adw::PreferencesGroup::new();
        let name_row = adw::EntryRow::builder()
            .title("Name")
            .text(profile.name.clone())
            .show_apply_button(true)
            .build();
        {
            let this = Rc::downgrade(self);
            let profile_id = profile.id.clone();
            let navigation_page = navigation_page.downgrade();
            name_row.connect_apply(move |row| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                let new_name = row.text().trim().to_string();
                if new_name.is_empty() {
                    return;
                }
                this.profiles.borrow_mut().rename(&profile_id, &new_name);
                if let Some(navigation_page) = navigation_page.upgrade() {
                    navigation_page.set_title(&new_name);
                }
                this.rebuild();
            });
        }
        identity.add(&name_row);

        let is_default = self.prefs.borrow().default_profile_id == profile.id;
        let default_row = adw::SwitchRow::builder()
            .title("Default profile")
            .subtitle("New sessions are created with this profile")
            .active(is_default)
            .build();
        {
            let this = Rc::downgrade(self);
            let profile_id = profile.id.clone();
            default_row.connect_active_notify(move |row| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                if row.is_active() {
                    let mut prefs = this.prefs.borrow_mut();
                    prefs.default_profile_id = profile_id.clone();
                    prefs.save();
                } else if this.prefs.borrow().default_profile_id == profile_id {
                    // There must always be exactly one default profile:
                    // turning the current one off is undone rather than
                    // leaving none. Another profile becomes the default by
                    // switching *it* on instead.
                    row.set_active(true);
                    return;
                }
                this.rebuild();
            });
        }
        identity.add(&default_row);
        page.add(&identity);

        let text = adw::PreferencesGroup::builder().title("Text").build();
        let font_dialog = gtk4::FontDialog::builder().title("Terminal font").build();
        let font_button = gtk4::FontDialogButton::builder()
            .dialog(&font_dialog)
            .font_desc(&gtk4::pango::FontDescription::from_string(&options.font))
            .valign(gtk4::Align::Center)
            .build();
        let font_row = adw::ActionRow::builder()
            .title("Custom font")
            .sensitive(!options.use_system_font)
            .build();
        font_row.add_suffix(&font_button);
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            font_button.connect_font_desc_notify(move |button| {
                if let Some(desc) = button.font_desc() {
                    let font = desc.to_string();
                    profiles
                        .borrow_mut()
                        .update(&profile_id, |p| p.options.font = font);
                }
            });
        }
        let system_font_row = options_switch_row(
            &self.profiles,
            &profile.id,
            "Use the system monospace font",
            options.use_system_font,
            |o, v| o.use_system_font = v,
        );
        {
            let font_row = font_row.clone();
            system_font_row
                .connect_active_notify(move |row| font_row.set_sensitive(!row.is_active()));
        }
        text.add(&system_font_row);
        text.add(&font_row);
        text.add(&options_spin_row(
            &self.profiles,
            &profile.id,
            "Cell width spacing",
            options.cell_width_scale,
            |o, v| o.cell_width_scale = v,
        ));
        text.add(&options_spin_row(
            &self.profiles,
            &profile.id,
            "Cell height spacing",
            options.cell_height_scale,
            |o, v| o.cell_height_scale = v,
        ));
        text.add(&options_combo_row(
            &self.profiles,
            &profile.id,
            "Text blink",
            &TextBlink::ALL,
            TextBlink::label,
            options.text_blink,
            |o, v| o.text_blink = v,
        ));
        page.add(&text);

        let cursor = adw::PreferencesGroup::builder()
            .title("Cursor and selection")
            .build();
        cursor.add(&options_combo_row(
            &self.profiles,
            &profile.id,
            "Cursor shape",
            &CursorShape::ALL,
            CursorShape::label,
            options.cursor_shape,
            |o, v| o.cursor_shape = v,
        ));
        cursor.add(&options_combo_row(
            &self.profiles,
            &profile.id,
            "Cursor blink",
            &CursorBlink::ALL,
            CursorBlink::label,
            options.cursor_blink,
            |o, v| o.cursor_blink = v,
        ));
        let word_chars_row = adw::EntryRow::builder()
            .title("Word-wise select characters")
            .text(options.word_chars.clone())
            .show_apply_button(true)
            .build();
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            word_chars_row.connect_apply(move |row| {
                let chars = row.text().to_string();
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.options.word_chars = chars);
            });
        }
        cursor.add(&word_chars_row);
        page.add(&cursor);

        let notifications = adw::PreferencesGroup::builder()
            .title("Bell and notifications")
            .build();
        notifications.add(&options_combo_row(
            &self.profiles,
            &profile.id,
            "Terminal bell",
            &BellMode::ALL,
            BellMode::label,
            options.bell,
            |o, v| o.bell = v,
        ));
        let silence_adjustment =
            gtk4::Adjustment::new(profile.silence_seconds as f64, 1.0, 300.0, 1.0, 5.0, 0.0);
        let silence_row = adw::SpinRow::builder()
            .title("Notify after silence")
            .subtitle(
                "Seconds of no output, after activity, before a \"command finished\" notification",
            )
            .adjustment(&silence_adjustment)
            .build();
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            silence_row.connect_value_notify(move |row| {
                profiles
                    .borrow_mut()
                    .set_silence_seconds(&profile_id, row.value() as u32);
            });
        }
        notifications.add(&silence_row);
        page.add(&notifications);

        page.add(&self.manage_group(&profile.id));
        page
    }

    /// Clone / Delete, at the bottom of the General tab. Both leave the
    /// editor (the profile it shows is no longer the one being edited, or
    /// no longer exists) and refresh the list.
    fn manage_group(self: &Rc<Self>, profile_id: &str) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        let button_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        button_box.set_halign(gtk4::Align::End);

        let clone_button = gtk4::Button::builder().label("Clone").build();
        {
            let this = Rc::downgrade(self);
            let profile_id = profile_id.to_string();
            clone_button.connect_clicked(move |_| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                let new_id = this.profiles.borrow_mut().clone_profile(&profile_id);
                this.rebuild();
                if let (Some(window), Some(new_id)) = (this.window.upgrade(), new_id) {
                    window.pop_subpage();
                    this.open_editor(&new_id);
                }
            });
        }
        button_box.append(&clone_button);

        if self.profiles.borrow().iter().count() > 1 {
            let delete_button = gtk4::Button::builder().label("Delete").build();
            delete_button.add_css_class("destructive-action");
            {
                let this = Rc::downgrade(self);
                let profile_id = profile_id.to_string();
                delete_button.connect_clicked(move |_| {
                    let Some(this) = this.upgrade() else {
                        return;
                    };
                    let was_default = this.prefs.borrow().default_profile_id == profile_id;
                    if !this.profiles.borrow_mut().delete(&profile_id) {
                        return;
                    }
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
                    if let Some(window) = this.window.upgrade() {
                        window.pop_subpage();
                    }
                });
            }
            button_box.append(&delete_button);
        }

        group.add(&button_box);
        group
    }

    fn command_tab(self: &Rc<Self>, profile: &Profile) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::new();

        let login_shell_row = adw::SwitchRow::builder()
            .title("Run command as a login shell")
            .active(profile.login_shell)
            .build();
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            login_shell_row.connect_active_notify(move |row| {
                let active = row.is_active();
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.login_shell = active);
            });
        }
        group.add(&login_shell_row);

        let use_custom_command_row = adw::SwitchRow::builder()
            .title("Run a custom command instead of my shell")
            .active(profile.use_custom_command)
            .build();
        let custom_command_row = adw::EntryRow::builder()
            .title("Custom command")
            .text(profile.custom_command.clone())
            .show_apply_button(true)
            .sensitive(profile.use_custom_command)
            .build();
        // A login shell only applies to the shell itself, so the switch is
        // meaningless while a custom command replaces it.
        login_shell_row.set_sensitive(!profile.use_custom_command);
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            let custom_command_row = custom_command_row.clone();
            let login_shell_row = login_shell_row.clone();
            use_custom_command_row.connect_active_notify(move |row| {
                let active = row.is_active();
                custom_command_row.set_sensitive(active);
                login_shell_row.set_sensitive(!active);
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.use_custom_command = active);
            });
        }
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            custom_command_row.connect_apply(move |row| {
                let command = row.text().trim().to_string();
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.custom_command = command);
            });
        }
        group.add(&use_custom_command_row);
        group.add(&custom_command_row);

        let working_directory_row = adw::EntryRow::builder()
            .title("Working directory (empty: where Rutile was launched)")
            .text(profile.working_directory.clone().unwrap_or_default())
            .show_apply_button(true)
            .build();
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            working_directory_row.connect_apply(move |row| {
                let directory = row.text().trim().to_string();
                let directory = (!directory.is_empty()).then_some(directory);
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.working_directory = directory);
            });
        }
        group.add(&working_directory_row);

        group.add(&enum_combo_row(
            "When command exits",
            &ExitAction::ALL,
            ExitAction::label,
            profile.exit_action,
            {
                let profiles = self.profiles.clone();
                let profile_id = profile.id.clone();
                move |action| {
                    profiles
                        .borrow_mut()
                        .update(&profile_id, |p| p.exit_action = action)
                }
            },
        ));

        page.add(&group);
        page
    }

    fn color_tab(self: &Rc<Self>, profile: &Profile) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::new();

        let scheme_ids: Vec<String> = self.schemes.iter().map(|s| s.id.clone()).collect();
        let scheme_names: Vec<&str> = self.schemes.iter().map(|s| s.name.as_str()).collect();
        let scheme_row = adw::ComboRow::builder()
            .title("Color scheme")
            .model(&gtk4::StringList::new(&scheme_names))
            .build();
        if let Some(index) = scheme_ids.iter().position(|id| *id == profile.scheme_id) {
            scheme_row.set_selected(index as u32);
        }
        {
            let this = Rc::downgrade(self);
            let profile_id = profile.id.clone();
            scheme_row.connect_selected_notify(move |row| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                if let Some(scheme_id) = scheme_ids.get(row.selected() as usize) {
                    this.profiles
                        .borrow_mut()
                        .set_scheme(&profile_id, scheme_id);
                    // The list shows each profile's scheme as its subtitle.
                    this.rebuild();
                }
            });
        }
        group.add(&scheme_row);
        page.add(&group);

        let colors = &profile.colors;
        let overrides = adw::PreferencesGroup::builder()
            .title("Colors")
            .description("Set on top of the color scheme")
            .build();
        overrides.add(&{
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            let row = adw::SwitchRow::builder()
                .title("Show bold text in bright colors")
                .active(colors.bold_is_bright)
                .build();
            row.connect_active_notify(move |row| {
                let active = row.is_active();
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.colors.bold_is_bright = active);
            });
            row
        });
        overrides.add(&{
            let adjustment = gtk4::Adjustment::new(
                f64::from(colors.background_transparency),
                0.0,
                100.0,
                5.0,
                10.0,
                0.0,
            );
            let row = adw::SpinRow::builder()
                .title("Background transparency")
                .subtitle("Percent — 0 is opaque")
                .adjustment(&adjustment)
                .build();
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            row.connect_value_notify(move |row| {
                let percent = row.value().clamp(0.0, 100.0) as u8;
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.colors.background_transparency = percent);
            });
            row
        });
        overrides.add(&color_pair_expander(
            &self.profiles,
            &profile.id,
            "Custom cursor colors",
            colors.cursor_colors.clone(),
            |c| &mut c.cursor_colors,
        ));
        overrides.add(&color_pair_expander(
            &self.profiles,
            &profile.id,
            "Custom highlight colors",
            colors.highlight_colors.clone(),
            |c| &mut c.highlight_colors,
        ));
        overrides.add(&color_expander(
            &self.profiles,
            &profile.id,
            "Custom bold color",
            colors.bold_color.clone(),
            |c| &mut c.bold_color,
        ));
        page.add(&overrides);

        page
    }

    /// Tilix's "Automatic Profile Switching" list: one row per rule with a
    /// remove button, and an entry to add one.
    fn advanced_tab(self: &Rc<Self>, profile: &Profile) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::builder()
            .title("Automatic profile switching")
            .description(
                "Switch a terminal to this profile while it is in a matching place: \
                 user@host:directory, every part optional (e.g. prod-db, \
                 :~/projets/work, root@server:/etc). A directory matches its \
                 subdirectories too. Rules are checked when the shell reports \
                 its directory (OSC 7).",
            )
            .build();
        let rows: Rc<RefCell<Vec<adw::ActionRow>>> = Rc::default();

        let add_row = adw::EntryRow::builder()
            .title("Add a rule")
            .show_apply_button(true)
            .build();
        group.add(&add_row);

        let refresh: Rc<dyn Fn()> = {
            let group = group.downgrade();
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            let rows = rows.clone();
            Rc::new(move || {
                let Some(group) = group.upgrade() else {
                    return;
                };
                rebuild_rule_rows(&group, &rows, &profiles, &profile_id);
            })
        };
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            let refresh = refresh.clone();
            add_row.connect_apply(move |row| {
                let rule = row.text().trim().to_string();
                if rule.is_empty() {
                    return;
                }
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.automatic_switch.push(rule));
                row.set_text("");
                refresh();
            });
        }
        refresh();

        page.add(&group);
        page
    }

    /// Encoding and ambiguous-width characters — Tilix's Compatibility tab.
    fn compatibility_tab(self: &Rc<Self>, profile: &Profile) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::new();

        let encodings = crate::terminal::encoding::supported_encodings();
        let names: Vec<&str> = encodings.iter().map(String::as_str).collect();
        let encoding_row = adw::ComboRow::builder()
            .title("Encoding")
            .subtitle("For legacy systems — the shell and programs must use the same one")
            .model(&gtk4::StringList::new(&names))
            .enable_search(true)
            .expression(gtk4::PropertyExpression::new(
                gtk4::StringObject::static_type(),
                None::<gtk4::Expression>,
                "string",
            ))
            .build();
        let current = encodings
            .iter()
            .position(|e| e.eq_ignore_ascii_case(&profile.options.encoding))
            .unwrap_or(0);
        encoding_row.set_selected(current as u32);
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            encoding_row.connect_selected_notify(move |row| {
                if let Some(encoding) = encodings.get(row.selected() as usize).cloned() {
                    profiles
                        .borrow_mut()
                        .update(&profile_id, |p| p.options.encoding = encoding);
                }
            });
        }
        group.add(&encoding_row);

        group.add(&options_combo_row(
            &self.profiles,
            &profile.id,
            "Ambiguous-width characters",
            &CjkWidth::ALL,
            CjkWidth::label,
            profile.options.cjk_ambiguous_width,
            |o, v| o.cjk_ambiguous_width = v,
        ));

        page.add(&group);
        page
    }

    fn badge_tab(self: &Rc<Self>, profile: &Profile) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::new();
        let badge = &profile.badge;
        let group = adw::PreferencesGroup::builder()
            .description(
                "Large text in a corner of the terminal. Same variables as titles: \
                 ${title}, ${directory}, ${host}, ${user}, ${id}.",
            )
            .build();

        let text_row = adw::EntryRow::builder()
            .title("Badge text (empty: no badge)")
            .text(badge.badge_text.clone())
            .show_apply_button(true)
            .build();
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            text_row.connect_apply(move |row| {
                let text = row.text().to_string();
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.badge.badge_text = text);
            });
        }
        group.add(&text_row);

        group.add(&enum_combo_row(
            "Position",
            &BadgePosition::ALL,
            BadgePosition::label,
            badge.badge_position,
            {
                let profiles = self.profiles.clone();
                let profile_id = profile.id.clone();
                move |position| {
                    profiles
                        .borrow_mut()
                        .update(&profile_id, |p| p.badge.badge_position = position)
                }
            },
        ));

        let font_dialog = gtk4::FontDialog::builder().title("Badge font").build();
        let font_button = gtk4::FontDialogButton::builder()
            .dialog(&font_dialog)
            .font_desc(&gtk4::pango::FontDescription::from_string(
                &badge.badge_font,
            ))
            .valign(gtk4::Align::Center)
            .build();
        let font_row = adw::ActionRow::builder()
            .title("Custom font")
            .sensitive(!badge.badge_use_system_font)
            .build();
        font_row.add_suffix(&font_button);
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            font_button.connect_font_desc_notify(move |button| {
                if let Some(desc) = button.font_desc() {
                    let font = desc.to_string();
                    profiles
                        .borrow_mut()
                        .update(&profile_id, |p| p.badge.badge_font = font);
                }
            });
        }
        let system_font_row = adw::SwitchRow::builder()
            .title("Twice the terminal font size")
            .active(badge.badge_use_system_font)
            .build();
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            let font_row = font_row.clone();
            system_font_row.connect_active_notify(move |row| {
                let active = row.is_active();
                font_row.set_sensitive(!active);
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.badge.badge_use_system_font = active);
            });
        }
        group.add(&system_font_row);
        group.add(&font_row);

        group.add(&color_expander(
            &self.profiles,
            &profile.id,
            "Custom badge color",
            profile.colors.badge_color.clone(),
            |c| &mut c.badge_color,
        ));

        page.add(&group);
        page
    }

    fn scrolling_tab(self: &Rc<Self>, profile: &Profile) -> adw::PreferencesPage {
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::new();
        let options = &profile.options;

        group.add(&options_switch_row(
            &self.profiles,
            &profile.id,
            "Show scrollbar",
            options.show_scrollbar,
            |o, v| o.show_scrollbar = v,
        ));
        group.add(&options_switch_row(
            &self.profiles,
            &profile.id,
            "Scroll on output",
            options.scroll_on_output,
            |o, v| o.scroll_on_output = v,
        ));
        group.add(&options_switch_row(
            &self.profiles,
            &profile.id,
            "Scroll on keystroke",
            options.scroll_on_keystroke,
            |o, v| o.scroll_on_keystroke = v,
        ));

        let lines_adjustment = gtk4::Adjustment::new(
            options.scrollback_lines as f64,
            0.0,
            1_000_000.0,
            100.0,
            1000.0,
            0.0,
        );
        let lines_row = adw::SpinRow::builder()
            .title("Scrollback lines")
            .adjustment(&lines_adjustment)
            .sensitive(!options.scrollback_unlimited)
            .build();
        {
            let profiles = self.profiles.clone();
            let profile_id = profile.id.clone();
            lines_row.connect_value_notify(move |row| {
                let lines = row.value() as u32;
                profiles
                    .borrow_mut()
                    .update(&profile_id, |p| p.options.scrollback_lines = lines);
            });
        }
        let unlimited_row = options_switch_row(
            &self.profiles,
            &profile.id,
            "Unlimited scrollback",
            options.scrollback_unlimited,
            |o, v| o.scrollback_unlimited = v,
        );
        {
            let lines_row = lines_row.clone();
            unlimited_row
                .connect_active_notify(move |row| lines_row.set_sensitive(!row.is_active()));
        }
        group.add(&unlimited_row);
        group.add(&lines_row);

        page.add(&group);
        page
    }
}

/// A switch bound to one boolean of a profile's `TerminalOptions`, saved on
/// every toggle like every other preference.
fn options_switch_row(
    profiles: &Rc<RefCell<ProfileStore>>,
    profile_id: &str,
    title: &str,
    active: bool,
    set: fn(&mut TerminalOptions, bool),
) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(title)
        .active(active)
        .build();
    let profiles = profiles.clone();
    let profile_id = profile_id.to_string();
    row.connect_active_notify(move |row| {
        let active = row.is_active();
        profiles
            .borrow_mut()
            .update(&profile_id, |p| set(&mut p.options, active));
    });
    row
}

/// A drop-down over an enum's values (`all`, its `ALL` constant, gives the
/// order), calling `on_change` with the newly selected one.
fn enum_combo_row<T: Copy + PartialEq + 'static>(
    title: &str,
    all: &'static [T],
    label: fn(T) -> &'static str,
    current: T,
    on_change: impl Fn(T) + 'static,
) -> adw::ComboRow {
    let labels: Vec<&str> = all.iter().map(|value| label(*value)).collect();
    let row = adw::ComboRow::builder()
        .title(title)
        .model(&gtk4::StringList::new(&labels))
        .build();
    if let Some(index) = all.iter().position(|value| *value == current) {
        row.set_selected(index as u32);
    }
    row.connect_selected_notify(move |row| {
        if let Some(&value) = all.get(row.selected() as usize) {
            on_change(value);
        }
    });
    row
}

/// `enum_combo_row` bound to one enum of a profile's `TerminalOptions`.
fn options_combo_row<T: Copy + PartialEq + 'static>(
    profiles: &Rc<RefCell<ProfileStore>>,
    profile_id: &str,
    title: &str,
    all: &'static [T],
    label: fn(T) -> &'static str,
    current: T,
    set: fn(&mut TerminalOptions, T),
) -> adw::ComboRow {
    let profiles = profiles.clone();
    let profile_id = profile_id.to_string();
    enum_combo_row(title, all, label, current, move |value| {
        profiles
            .borrow_mut()
            .update(&profile_id, |p| set(&mut p.options, value))
    })
}

/// A 1.0–2.0 spin button bound to one of a profile's cell scale factors.
fn options_spin_row(
    profiles: &Rc<RefCell<ProfileStore>>,
    profile_id: &str,
    title: &str,
    value: f64,
    set: fn(&mut TerminalOptions, f64),
) -> adw::SpinRow {
    let adjustment = gtk4::Adjustment::new(value, 1.0, 2.0, 0.05, 0.25, 0.0);
    let row = adw::SpinRow::builder()
        .title(title)
        .adjustment(&adjustment)
        .digits(2)
        .build();
    let profiles = profiles.clone();
    let profile_id = profile_id.to_string();
    row.connect_value_notify(move |row| {
        let value = row.value();
        profiles
            .borrow_mut()
            .update(&profile_id, |p| set(&mut p.options, value));
    });
    row
}

/// `#rrggbb` for a color picked in a `ColorDialogButton` (alpha dropped —
/// vte draws these colors opaque anyway).
fn rgba_to_hex(rgba: &gtk4::gdk::RGBA) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgba.red()),
        channel(rgba.green()),
        channel(rgba.blue())
    )
}

fn color_button(hex: &str) -> gtk4::ColorDialogButton {
    gtk4::ColorDialogButton::builder()
        .dialog(&gtk4::ColorDialog::builder().with_alpha(false).build())
        .rgba(&crate::profile::scheme::parse_or_fallback(hex))
        .valign(gtk4::Align::Center)
        .build()
}

/// An expander whose built-in switch turns an optional foreground/
/// background color pair of `ProfileColors` on (`Some`, with the two
/// buttons' colors) or off (`None`, vte's default). The buttons keep their
/// colors while it's off, so turning it back on restores them.
fn color_pair_expander(
    profiles: &Rc<RefCell<ProfileStore>>,
    profile_id: &str,
    title: &str,
    current: Option<ColorPair>,
    field: fn(&mut ProfileColors) -> &mut Option<ColorPair>,
) -> adw::ExpanderRow {
    let enabled = current.is_some();
    let initial = current.unwrap_or_else(ColorPair::tilix_default);
    let foreground = color_button(&initial.foreground);
    let background = color_button(&initial.background);

    let expander = adw::ExpanderRow::builder()
        .title(title)
        .show_enable_switch(true)
        .enable_expansion(enabled)
        .build();
    for (label, button) in [("Text", &foreground), ("Background", &background)] {
        let row = adw::ActionRow::builder().title(label).build();
        row.add_suffix(button);
        expander.add_row(&row);
    }

    let save: Rc<dyn Fn()> = {
        let profiles = profiles.clone();
        let profile_id = profile_id.to_string();
        let expander = expander.downgrade();
        let (foreground, background) = (foreground.downgrade(), background.downgrade());
        Rc::new(move || {
            let (Some(expander), Some(foreground), Some(background)) = (
                expander.upgrade(),
                foreground.upgrade(),
                background.upgrade(),
            ) else {
                return;
            };
            let value = expander.enables_expansion().then(|| ColorPair {
                foreground: rgba_to_hex(&foreground.rgba()),
                background: rgba_to_hex(&background.rgba()),
            });
            profiles
                .borrow_mut()
                .update(&profile_id, |p| *field(&mut p.colors) = value);
        })
    };
    {
        let save = save.clone();
        expander.connect_enable_expansion_notify(move |_| save());
    }
    for button in [&foreground, &background] {
        let save = save.clone();
        button.connect_rgba_notify(move |_| save());
    }
    expander
}

/// `color_pair_expander` for a single optional color (bold, badge).
fn color_expander(
    profiles: &Rc<RefCell<ProfileStore>>,
    profile_id: &str,
    title: &str,
    current: Option<String>,
    field: fn(&mut ProfileColors) -> &mut Option<String>,
) -> adw::ExpanderRow {
    let enabled = current.is_some();
    // Tilix's default for both the bold and badge colors.
    let button = color_button(current.as_deref().unwrap_or("#ffffff"));

    let expander = adw::ExpanderRow::builder()
        .title(title)
        .show_enable_switch(true)
        .enable_expansion(enabled)
        .build();
    let row = adw::ActionRow::builder().title("Color").build();
    row.add_suffix(&button);
    expander.add_row(&row);

    let save: Rc<dyn Fn()> = {
        let profiles = profiles.clone();
        let profile_id = profile_id.to_string();
        let expander = expander.downgrade();
        let button = button.downgrade();
        Rc::new(move || {
            let (Some(expander), Some(button)) = (expander.upgrade(), button.upgrade()) else {
                return;
            };
            let value = expander
                .enables_expansion()
                .then(|| rgba_to_hex(&button.rgba()));
            profiles
                .borrow_mut()
                .update(&profile_id, |p| *field(&mut p.colors) = value);
        })
    };
    {
        let save = save.clone();
        expander.connect_enable_expansion_notify(move |_| save());
    }
    button.connect_rgba_notify(move |_| save());
    expander
}

/// Re-lists a profile's automatic switch rules below the "Add a rule"
/// entry, each with a button removing it (by index, then re-listing).
fn rebuild_rule_rows(
    group: &adw::PreferencesGroup,
    rows: &Rc<RefCell<Vec<adw::ActionRow>>>,
    profiles: &Rc<RefCell<ProfileStore>>,
    profile_id: &str,
) {
    for row in rows.borrow_mut().drain(..) {
        group.remove(&row);
    }
    let rules = profiles
        .borrow()
        .get(profile_id)
        .map(|p| p.automatic_switch.clone())
        .unwrap_or_default();
    for (index, rule) in rules.iter().enumerate() {
        let row = adw::ActionRow::builder().title(rule.as_str()).build();
        let remove = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Remove rule")
            .valign(gtk4::Align::Center)
            .build();
        remove.add_css_class("flat");
        {
            let group = group.downgrade();
            let rows = rows.clone();
            let profiles = profiles.clone();
            let profile_id = profile_id.to_string();
            remove.connect_clicked(move |_| {
                profiles.borrow_mut().update(&profile_id, |p| {
                    if index < p.automatic_switch.len() {
                        p.automatic_switch.remove(index);
                    }
                });
                if let Some(group) = group.upgrade() {
                    rebuild_rule_rows(&group, &rows, &profiles, &profile_id);
                }
            });
        }
        row.add_suffix(&remove);
        group.add(&row);
        rows.borrow_mut().push(row);
    }
}
