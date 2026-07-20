use gtk4::gdk;

use crate::layout::{Direction, Orientation};

/// Actions dispatchable from the keyboard. Pane-level actions are handled
/// directly against the focused session's `PaneView`; session-level actions
/// are wired in by `session::session_view` once it exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    Navigate(Direction),
    NewSession,
    CloseSession,
    NextSession,
    PrevSession,
}

const CTRL_SHIFT: gdk::ModifierType =
    gdk::ModifierType::CONTROL_MASK.union(gdk::ModifierType::SHIFT_MASK);

/// Tilix-parity keybindings for v0.1.
const KEYBINDINGS: &[(gdk::Key, gdk::ModifierType, Action)] = &[
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
];

/// Maps a key + active modifiers to an `Action`, ignoring modifier bits not
/// part of the bindings table (e.g. NumLock/CapsLock lock bits).
pub fn lookup(key: gdk::Key, state: gdk::ModifierType) -> Option<Action> {
    let relevant = state.intersection(
        gdk::ModifierType::CONTROL_MASK
            .union(gdk::ModifierType::SHIFT_MASK)
            .union(gdk::ModifierType::ALT_MASK),
    );
    KEYBINDINGS
        .iter()
        .find(|(k, m, _)| *k == key && *m == relevant)
        .map(|(_, _, action)| *action)
}

pub fn orientation_for(action: Action) -> Option<Orientation> {
    match action {
        Action::SplitHorizontal => Some(Orientation::Horizontal),
        Action::SplitVertical => Some(Orientation::Vertical),
        _ => None,
    }
}
