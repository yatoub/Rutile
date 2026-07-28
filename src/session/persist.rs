use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::layout::{PaneId, SplitTree};
use crate::profile::ProfileId;
use crate::terminal::broadcast::BroadcastGroup;

/// Per-pane extras that live outside the tree's own shape (which is just
/// ids + orientation + ratio). Keyed by the `PaneId` the tree had *at save
/// time* — `SessionView::restore_session` maps that back onto the fresh
/// ids `SplitTree::remap_ids` hands out for this process.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaneMeta {
    /// The shell's working directory, if vte reported one via OSC 7. `None`
    /// means "spawn wherever a brand new terminal normally would" — same
    /// as Tilix, only layout/cwd/profile are restored, not shell state.
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub name: String,
    pub profile_id: ProfileId,
    pub tree: SplitTree,
    pub pane_meta: std::collections::HashMap<PaneId, PaneMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedWindow {
    pub sessions: Vec<SavedSession>,
    pub active_session_index: usize,
    pub window_width: i32,
    pub window_height: i32,
    pub broadcast_group: BroadcastGroup,
}

impl SavedWindow {
    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        std::fs::create_dir_all(parent)?;
        let contents = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, contents)
    }

    pub fn load_from_file(path: &std::path::Path) -> Option<Self> {
        let contents = std::fs::read_to_string(path).ok()?;
        toml::from_str(&contents).ok()
    }
}

/// Where a clean shutdown auto-saves to, and where a fresh launch
/// auto-restores from if present (unless a `-s`/`--session` CLI argument
/// says otherwise — Phase 5).
///
/// `glib::user_state_dir()` would normally do this, but it's gated behind
/// glib's `v2_72` feature, which isn't enabled by the gtk4/libadwaita
/// feature set this project deliberately pins to the oldest supported
/// distro (see CLAUDE.md's "Stack" section) — so this follows the XDG Base
/// Directory spec by hand instead, same fallback glib itself uses.
fn user_state_dir() -> PathBuf {
    if let Ok(xdg_state_home) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(xdg_state_home);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home).join(".local").join("state")
}

pub fn last_session_path() -> PathBuf {
    let mut path = user_state_dir();
    path.push("rutile");
    path.push("last-session.toml");
    path
}
