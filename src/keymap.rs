use std::path::PathBuf;

use gtk4::gdk;
use serde::{Deserialize, Serialize};

use crate::layout::{Direction, Orientation};

/// Actions dispatchable from the keyboard. Pane-level actions are handled
/// directly against the focused session's `PaneView`; session-level actions
/// are wired in by `session::session_view` once it exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    Navigate(Direction),
    NewSession,
    CloseSession,
    NextSession,
    PrevSession,
    ToggleSearch,
    Copy,
    Paste,
    /// Jumps directly to the Nth session in tab order (1-indexed, matching
    /// what's shown to the user — `SwitchToSession(1)` is the first tab).
    SwitchToSession(u8),
    ResizePane(Direction),
    ToggleSyncCurrentPane,
    RenameSession,
    RenamePane,
    DetachSession,
    CopyAsHtml,
    PasteAdvanced,
    ToggleMargin,
}

const CTRL_SHIFT: gdk::ModifierType =
    gdk::ModifierType::CONTROL_MASK.union(gdk::ModifierType::SHIFT_MASK);
const CTRL_SHIFT_ALT: gdk::ModifierType = CTRL_SHIFT.union(gdk::ModifierType::ALT_MASK);

/// Tilix-parity keybindings, extended with Phase 4's new actions. Serves as
/// both the runtime lookup table when no user config exists yet, and the
/// baseline `Keymap::default()` written out (and diffed against) once one
/// does.
const DEFAULT_KEYBINDINGS: &[(gdk::Key, gdk::ModifierType, Action)] = &[
    (gdk::Key::O, CTRL_SHIFT, Action::SplitHorizontal),
    (gdk::Key::E, CTRL_SHIFT, Action::SplitVertical),
    (gdk::Key::W, CTRL_SHIFT, Action::ClosePane),
    (gdk::Key::Up, CTRL_SHIFT, Action::Navigate(Direction::Up)),
    (
        gdk::Key::Down,
        CTRL_SHIFT,
        Action::Navigate(Direction::Down),
    ),
    (
        gdk::Key::Left,
        CTRL_SHIFT,
        Action::Navigate(Direction::Left),
    ),
    (
        gdk::Key::Right,
        CTRL_SHIFT,
        Action::Navigate(Direction::Right),
    ),
    (gdk::Key::T, CTRL_SHIFT, Action::NewSession),
    (gdk::Key::Q, CTRL_SHIFT, Action::CloseSession),
    (gdk::Key::Page_Down, CTRL_SHIFT, Action::NextSession),
    (gdk::Key::Page_Up, CTRL_SHIFT, Action::PrevSession),
    (gdk::Key::F, CTRL_SHIFT, Action::ToggleSearch),
    (gdk::Key::C, CTRL_SHIFT, Action::Copy),
    (gdk::Key::V, CTRL_SHIFT, Action::Paste),
    (gdk::Key::_1, CTRL_SHIFT, Action::SwitchToSession(1)),
    (gdk::Key::_2, CTRL_SHIFT, Action::SwitchToSession(2)),
    (gdk::Key::_3, CTRL_SHIFT, Action::SwitchToSession(3)),
    (gdk::Key::_4, CTRL_SHIFT, Action::SwitchToSession(4)),
    (gdk::Key::_5, CTRL_SHIFT, Action::SwitchToSession(5)),
    (gdk::Key::_6, CTRL_SHIFT, Action::SwitchToSession(6)),
    (gdk::Key::_7, CTRL_SHIFT, Action::SwitchToSession(7)),
    (gdk::Key::_8, CTRL_SHIFT, Action::SwitchToSession(8)),
    (gdk::Key::_9, CTRL_SHIFT, Action::SwitchToSession(9)),
    (
        gdk::Key::Up,
        CTRL_SHIFT_ALT,
        Action::ResizePane(Direction::Up),
    ),
    (
        gdk::Key::Down,
        CTRL_SHIFT_ALT,
        Action::ResizePane(Direction::Down),
    ),
    (
        gdk::Key::Left,
        CTRL_SHIFT_ALT,
        Action::ResizePane(Direction::Left),
    ),
    (
        gdk::Key::Right,
        CTRL_SHIFT_ALT,
        Action::ResizePane(Direction::Right),
    ),
    (gdk::Key::S, CTRL_SHIFT, Action::ToggleSyncCurrentPane),
    (gdk::Key::F2, CTRL_SHIFT, Action::RenameSession),
    (gdk::Key::F6, CTRL_SHIFT, Action::RenamePane),
    (gdk::Key::D, CTRL_SHIFT, Action::DetachSession),
    (gdk::Key::C, CTRL_SHIFT_ALT, Action::CopyAsHtml),
    (gdk::Key::V, CTRL_SHIFT_ALT, Action::PasteAdvanced),
    (gdk::Key::M, CTRL_SHIFT, Action::ToggleMargin),
];

const RELEVANT_MODIFIERS: gdk::ModifierType = gdk::ModifierType::CONTROL_MASK
    .union(gdk::ModifierType::SHIFT_MASK)
    .union(gdk::ModifierType::ALT_MASK);

fn relevant(state: gdk::ModifierType) -> gdk::ModifierType {
    state.intersection(RELEVANT_MODIFIERS)
}

/// One user-editable entry: which key+modifier chord triggers which action.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Binding {
    /// e.g. `"Ctrl+Shift+O"` — `gdk::Key::name()`/`gdk::Key::from_name()`
    /// round-trip through this, so anything GDK can name is representable.
    chord: String,
    action: Action,
}

/// The user's keybinding table: `Keymap::default()` mirrors
/// `DEFAULT_KEYBINDINGS` exactly, loaded/saved as TOML so the Preferences
/// "Shortcuts" page can let the user rebind entries persistently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: DEFAULT_KEYBINDINGS
                .iter()
                .map(|(key, modifiers, action)| Binding {
                    chord: format_chord(*key, *modifiers),
                    action: *action,
                })
                .collect(),
        }
    }
}

fn config_path() -> PathBuf {
    let mut path = gtk4::glib::user_config_dir();
    path.push("rutile");
    path.push("keybindings.toml");
    path
}

impl Keymap {
    /// Loads the user's keymap from disk, falling back to
    /// `Keymap::default()` if the file doesn't exist yet or fails to parse.
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

    /// Maps a key + active modifiers to an `Action`, ignoring modifier bits
    /// not part of the bindings table (e.g. NumLock/CapsLock lock bits).
    pub fn lookup(&self, key: gdk::Key, state: gdk::ModifierType) -> Option<Action> {
        let relevant = relevant(state);
        self.bindings
            .iter()
            .find(|b| parse_chord(&b.chord) == Some((key, relevant)))
            .map(|b| b.action)
    }

    /// The chord currently bound to `action`, formatted for display (e.g. in
    /// the Preferences "Shortcuts" page). `None` if nothing is bound to it.
    pub fn chord_for(&self, action: Action) -> Option<String> {
        self.bindings
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.chord.clone())
    }

    /// Every `(action, chord)` pair, in table order — for the Preferences
    /// page to render one row per action.
    pub fn entries(&self) -> Vec<(Action, String)> {
        self.bindings
            .iter()
            .map(|b| (b.action, b.chord.clone()))
            .collect()
    }

    /// Rebinds `action` to `key`+`state`, replacing whatever it was bound to
    /// before. Any *other* action previously bound to the same chord is
    /// unbound (a chord can only ever trigger one action), mirroring how
    /// most keybinding editors handle collisions.
    pub fn rebind(&mut self, action: Action, key: gdk::Key, state: gdk::ModifierType) {
        let chord = format_chord(key, relevant(state));
        self.bindings
            .retain(|b| b.chord != chord || b.action == action);
        match self.bindings.iter_mut().find(|b| b.action == action) {
            Some(binding) => binding.chord = chord,
            None => self.bindings.push(Binding { chord, action }),
        }
    }
}

fn format_chord(key: gdk::Key, modifiers: gdk::ModifierType) -> String {
    let mut parts = Vec::new();
    if modifiers.contains(gdk::ModifierType::CONTROL_MASK) {
        parts.push("Ctrl".to_string());
    }
    if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
        parts.push("Shift".to_string());
    }
    if modifiers.contains(gdk::ModifierType::ALT_MASK) {
        parts.push("Alt".to_string());
    }
    parts.push(key.name().map(|n| n.to_string()).unwrap_or_default());
    parts.join("+")
}

fn parse_chord(chord: &str) -> Option<(gdk::Key, gdk::ModifierType)> {
    let mut modifiers = gdk::ModifierType::empty();
    let mut key = None;
    for part in chord.split('+') {
        match part {
            "Ctrl" => modifiers |= gdk::ModifierType::CONTROL_MASK,
            "Shift" => modifiers |= gdk::ModifierType::SHIFT_MASK,
            "Alt" => modifiers |= gdk::ModifierType::ALT_MASK,
            name => key = gdk::Key::from_name(name),
        }
    }
    Some((key?, modifiers))
}

fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "Up",
        Direction::Down => "Down",
        Direction::Left => "Left",
        Direction::Right => "Right",
    }
}

/// Human-readable label for the Preferences "Shortcuts" page.
pub fn action_label(action: Action) -> String {
    match action {
        Action::SplitHorizontal => "Split horizontally".to_string(),
        Action::SplitVertical => "Split vertically".to_string(),
        Action::ClosePane => "Close pane".to_string(),
        Action::Navigate(d) => format!("Navigate {}", direction_label(d)),
        Action::NewSession => "New session".to_string(),
        Action::CloseSession => "Close session".to_string(),
        Action::NextSession => "Next session".to_string(),
        Action::PrevSession => "Previous session".to_string(),
        Action::ToggleSearch => "Find in terminal".to_string(),
        Action::Copy => "Copy".to_string(),
        Action::Paste => "Paste".to_string(),
        Action::SwitchToSession(n) => format!("Switch to session {n}"),
        Action::ResizePane(d) => format!("Resize pane {}", direction_label(d)),
        Action::ToggleSyncCurrentPane => "Toggle sync for current pane".to_string(),
        Action::RenameSession => "Rename session".to_string(),
        Action::RenamePane => "Rename pane".to_string(),
        Action::DetachSession => "Detach session into new window".to_string(),
        Action::CopyAsHtml => "Copy as HTML".to_string(),
        Action::PasteAdvanced => "Paste (advanced)".to_string(),
        Action::ToggleMargin => "Toggle margin guide".to_string(),
    }
}

pub fn orientation_for(action: Action) -> Option<Orientation> {
    match action {
        Action::SplitHorizontal => Some(Orientation::Horizontal),
        Action::SplitVertical => Some(Orientation::Vertical),
        _ => None,
    }
}
