use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Persisted user preferences. Defaults match Tilix's own defaults for the
/// equivalent settings (both boxes ship checked in Tilix's General page),
/// since this is meant to be a drop-in replacement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Give focus to a pane when the mouse hovers over it, without needing
    /// a click. When `false`, focus only follows explicit clicks (see
    /// `context_menu::attach`'s `EventControllerFocus`).
    pub focus_follows_mouse: bool,
    /// Close the whole window once the last session is closed, instead of
    /// leaving an empty window open.
    pub close_window_on_last_session_closed: bool,
    /// Automatically copy the terminal selection to the clipboard as soon as
    /// it's made, instead of requiring an explicit "Copy".
    pub copy_on_select: bool,
    /// Ctrl+Click-to-open for OSC 8 hyperlinks and plain filesystem paths
    /// in terminal output (see `terminal::hyperlinks`). No Tilix
    /// equivalent to mirror here (Tilix predates OSC 8 support), so
    /// defaults on since it's a strictly additive, opt-out affordance.
    pub enable_hyperlinks: bool,
    /// Background pane notifications (bell + "command finished after
    /// silence", see `terminal::monitor`). A single global on/off switch —
    /// the *timing* of the silence trigger is per-profile
    /// (`Profile::silence_seconds`), but whether it's active at all isn't
    /// worth exposing per-profile too.
    pub enable_notifications: bool,
    /// Which profile (`crate::profile::ProfileStore`) new sessions are
    /// created with. Kept here rather than on `ProfileStore` itself since
    /// it's a cross-cutting *setting* (which profile is active), not part
    /// of any one profile's own data.
    pub default_profile_id: String,
    /// Show a confirmation dialog (`dialogs::confirm_close`) before closing
    /// a pane/session/window that has a foreground child process running
    /// (e.g. `vim`, a long `ssh`). Mirrors Tilix's own
    /// `prompt-on-close-process` setting, including its default of `true`.
    pub prompt_on_close_with_process: bool,
    /// Light or dark chrome (Catppuccin Latte or Mocha) — Tilix's "Theme
    /// variant". Defaults to dark rather than following the system like
    /// Tilix does: Rutile's look is Catppuccin Mocha, Latte is opt-in.
    pub theme_variant: ThemeVariant,
    /// An image drawn behind every terminal — visible through profiles
    /// with background transparency, like Tilix's. `None`: no image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_image: Option<PathBuf>,
    pub background_image_mode: BackgroundImageMode,
    /// Tilix's "Window style", minus its server-side-decoration variants:
    /// GTK4 can't switch a libadwaita window to server-side decorations.
    pub window_style: WindowStyle,
    /// Which side of the window the session sidebar sits on — Tilix's
    /// "Place the sidebar on the right".
    pub sidebar_position: SidebarPosition,
    /// What launching `rutile` again does while it's already running —
    /// Tilix's "On new instance". Overridden per launch by `--action`,
    /// `--new-window` and `--new-tab` (see `cli`).
    pub new_instance_mode: NewInstanceMode,
    /// Press Enter after inserting a bookmark's command, so it runs right
    /// away — Tilix's "Include return character with bookmark", on by
    /// default like there.
    pub bookmark_include_return: bool,
    /// Tilix's "Use a wide handle for splitters" — off by default like
    /// there (GTK's thin separator).
    pub wide_handles: bool,
    /// Tilix's "Show the terminal title even if it's the only terminal" —
    /// when off, a session with a single pane hides its pane header. On by
    /// default like there.
    pub show_title_when_single: bool,
    /// Tilix's "Warn when attempting unsafe paste": ask before pasting
    /// text that mentions `sudo` over several lines (`terminal::paste`).
    pub unsafe_paste_alert: bool,
    /// Tilix's "Strip first character of paste if comment or variable
    /// declaration": drop a leading `#` or `$`.
    pub paste_strip_first_char: bool,
    /// Tilix's "Strip trailing whitespaces and linebreak characters on
    /// paste".
    pub paste_strip_trailing_whitespace: bool,
    /// Tilix's "Advanced paste is default": the plain paste opens the
    /// advanced paste dialog too.
    pub paste_advanced_default: bool,
    /// The advanced paste dialog's last choices, remembered like Tilix:
    /// tabs to spaces (and every how many columns), CRLF/CR to LF.
    pub advanced_paste_tabs_to_spaces: bool,
    pub advanced_paste_tab_width: u32,
    pub advanced_paste_normalize_line_endings: bool,
}

/// Tilix's "On new instance" choices, shared with the `--action` command
/// line option (hence the `clap::ValueEnum`, whose kebab-case value names
/// match the TOML ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum NewInstanceMode {
    #[default]
    NewWindow,
    NewSession,
    SplitRight,
    SplitDown,
    FocusWindow,
}

impl NewInstanceMode {
    pub const ALL: [NewInstanceMode; 5] = [
        NewInstanceMode::NewWindow,
        NewInstanceMode::NewSession,
        NewInstanceMode::SplitRight,
        NewInstanceMode::SplitDown,
        NewInstanceMode::FocusWindow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NewInstanceMode::NewWindow => "Open a new window",
            NewInstanceMode::NewSession => "Open a new session",
            NewInstanceMode::SplitRight => "Split right",
            NewInstanceMode::SplitDown => "Split down",
            NewInstanceMode::FocusWindow => "Focus the window",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowStyle {
    #[default]
    Normal,
    /// No header bar (its menu stays reachable from the terminal's
    /// right-click menu and Ctrl+Comma), window frame kept.
    HideToolbar,
    /// No header bar and no window frame (shadow, resize borders).
    Borderless,
}

impl WindowStyle {
    pub const ALL: [WindowStyle; 3] = [
        WindowStyle::Normal,
        WindowStyle::HideToolbar,
        WindowStyle::Borderless,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WindowStyle::Normal => "Normal",
            WindowStyle::HideToolbar => "Hide the toolbar",
            WindowStyle::Borderless => "Borderless",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarPosition {
    #[default]
    Left,
    Right,
}

impl SidebarPosition {
    pub const ALL: [SidebarPosition; 2] = [SidebarPosition::Left, SidebarPosition::Right];

    pub fn label(self) -> &'static str {
        match self {
            SidebarPosition::Left => "Left",
            SidebarPosition::Right => "Right",
        }
    }
}

/// How the background image fills the window — Tilix's four modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundImageMode {
    /// Scaled to cover the whole window, keeping its aspect ratio
    /// (cropped if needed), centered.
    #[default]
    Scale,
    /// Repeated at its own size from the top-left corner.
    Tile,
    /// At its own size, centered.
    Center,
    /// Stretched to the window's size, ignoring its aspect ratio.
    Stretch,
}

impl BackgroundImageMode {
    pub const ALL: [BackgroundImageMode; 4] = [
        BackgroundImageMode::Scale,
        BackgroundImageMode::Tile,
        BackgroundImageMode::Center,
        BackgroundImageMode::Stretch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BackgroundImageMode::Scale => "Scale",
            BackgroundImageMode::Tile => "Tile",
            BackgroundImageMode::Center => "Center",
            BackgroundImageMode::Stretch => "Stretch",
        }
    }

    /// The CSS background properties for this mode.
    pub fn css(self) -> &'static str {
        match self {
            BackgroundImageMode::Scale => {
                "background-size: cover; background-position: center; background-repeat: no-repeat;"
            }
            BackgroundImageMode::Tile => {
                "background-size: auto; background-position: left top; background-repeat: repeat;"
            }
            BackgroundImageMode::Center => {
                "background-size: auto; background-position: center; background-repeat: no-repeat;"
            }
            BackgroundImageMode::Stretch => {
                "background-size: 100% 100%; background-position: center; background-repeat: no-repeat;"
            }
        }
    }
}

/// Tilix's "Theme variant" setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeVariant {
    /// Follow the desktop's light/dark preference.
    System,
    Light,
    #[default]
    Dark,
}

impl ThemeVariant {
    pub const ALL: [ThemeVariant; 3] = [
        ThemeVariant::System,
        ThemeVariant::Light,
        ThemeVariant::Dark,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeVariant::System => "Follow system",
            ThemeVariant::Light => "Light (Catppuccin Latte)",
            ThemeVariant::Dark => "Dark (Catppuccin Mocha)",
        }
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            focus_follows_mouse: true,
            close_window_on_last_session_closed: true,
            copy_on_select: true,
            enable_hyperlinks: true,
            enable_notifications: true,
            default_profile_id: "default".to_string(),
            prompt_on_close_with_process: true,
            theme_variant: ThemeVariant::default(),
            background_image: None,
            background_image_mode: BackgroundImageMode::default(),
            window_style: WindowStyle::default(),
            sidebar_position: SidebarPosition::default(),
            new_instance_mode: NewInstanceMode::default(),
            bookmark_include_return: true,
            wide_handles: false,
            show_title_when_single: true,
            unsafe_paste_alert: true,
            paste_strip_first_char: false,
            paste_strip_trailing_whitespace: false,
            paste_advanced_default: false,
            advanced_paste_tabs_to_spaces: false,
            advanced_paste_tab_width: 4,
            advanced_paste_normalize_line_endings: false,
        }
    }
}

fn config_path() -> PathBuf {
    let mut path = gtk4::glib::user_config_dir();
    path.push("rutile");
    path.push("preferences.toml");
    path
}

impl Preferences {
    /// The settings that rewrite a plain paste's text.
    pub fn paste_cleanup(&self) -> crate::terminal::paste::PasteCleanup {
        crate::terminal::paste::PasteCleanup {
            strip_trailing_whitespace: self.paste_strip_trailing_whitespace,
            strip_first_char: self.paste_strip_first_char,
        }
    }

    /// The advanced paste dialog's current transforms.
    pub fn advanced_paste_transform(&self) -> crate::terminal::paste::AdvancedTransform {
        crate::terminal::paste::AdvancedTransform {
            tabs_to_spaces: self
                .advanced_paste_tabs_to_spaces
                .then_some(self.advanced_paste_tab_width as usize),
            normalize_line_endings: self.advanced_paste_normalize_line_endings,
        }
    }

    /// Loads preferences from disk, falling back to defaults if the file
    /// doesn't exist yet or fails to parse (e.g. a future version wrote a
    /// format this build doesn't understand).
    pub fn load() -> Self {
        match std::fs::read_to_string(config_path()) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = config_path();
        let Some(parent) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        if let Ok(contents) = toml::to_string_pretty(self) {
            let _ = std::fs::write(path, contents);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_image_round_trips_and_defaults_to_none() {
        let prefs = Preferences {
            background_image: Some(PathBuf::from("/home/u/wall.png")),
            background_image_mode: BackgroundImageMode::Tile,
            ..Preferences::default()
        };
        let toml = toml::to_string(&prefs).unwrap();
        assert!(toml.contains(r#"background_image_mode = "tile""#), "{toml}");
        let back: Preferences = toml::from_str(&toml).unwrap();
        assert_eq!(back.background_image, prefs.background_image);
        assert_eq!(back.background_image_mode, BackgroundImageMode::Tile);

        // An unset image isn't written at all, and an older file without
        // these keys gets the defaults.
        let default_toml = toml::to_string(&Preferences::default()).unwrap();
        assert!(
            !default_toml.contains("background_image ="),
            "{default_toml}"
        );
        let old: Preferences = toml::from_str("focus_follows_mouse = false").unwrap();
        assert_eq!(old.background_image, None);
        assert_eq!(old.background_image_mode, BackgroundImageMode::Scale);
        assert_eq!(old.theme_variant, ThemeVariant::Dark);
        assert_eq!(old.window_style, WindowStyle::Normal);
        assert_eq!(old.sidebar_position, SidebarPosition::Left);
        assert!(!old.wide_handles);
        assert!(old.show_title_when_single);
        assert!(old.unsafe_paste_alert);
        assert!(!old.paste_advanced_default);
        assert_eq!(old.advanced_paste_tab_width, 4);
    }

    #[test]
    fn window_settings_use_tilix_like_names() {
        let prefs = Preferences {
            window_style: WindowStyle::HideToolbar,
            sidebar_position: SidebarPosition::Right,
            ..Preferences::default()
        };
        let toml = toml::to_string(&prefs).unwrap();
        assert!(toml.contains(r#"window_style = "hide-toolbar""#), "{toml}");
        assert!(toml.contains(r#"sidebar_position = "right""#), "{toml}");
    }
}
