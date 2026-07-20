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
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            focus_follows_mouse: true,
            close_window_on_last_session_closed: true,
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
