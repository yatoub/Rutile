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
