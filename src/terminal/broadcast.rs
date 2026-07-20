use std::collections::{HashMap, HashSet};

use crate::layout::PaneId;

pub type SessionId = u64;

/// Broadcasting is capped at "current session" — there is no "all
/// sessions" mode, since each session is meant to be an independent
/// working context and cross-session sync would be surprising more often
/// than useful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastGroup {
    None,
    Session,
}

/// Computes which panes should receive a broadcast keystroke. This module
/// never touches vte/GTK: it only decides *which* `PaneId`s are targets —
/// the actual `feed_child()` call lives in `terminal::widget`. The focused
/// pane always receives its own keystrokes natively through vte, so
/// `targets()` never includes the origin pane, regardless of group.
pub struct BroadcastManager {
    group: BroadcastGroup,
    session_members: HashMap<SessionId, Vec<PaneId>>,
    /// Panes temporarily opted out of broadcasting via their header's sync
    /// button, independent of the (session-wide) group setting — an
    /// excluded pane neither sends nor receives broadcast input.
    excluded: HashSet<PaneId>,
}

impl BroadcastManager {
    pub fn new() -> Self {
        Self {
            group: BroadcastGroup::None,
            session_members: HashMap::new(),
            excluded: HashSet::new(),
        }
    }

    pub fn set_group(&mut self, group: BroadcastGroup) {
        self.group = group;
    }

    pub fn group(&self) -> BroadcastGroup {
        self.group
    }

    pub fn register_pane(&mut self, session: SessionId, pane: PaneId) {
        self.session_members.entry(session).or_default().push(pane);
    }

    pub fn unregister_pane(&mut self, session: SessionId, pane: PaneId) {
        if let Some(members) = self.session_members.get_mut(&session) {
            members.retain(|id| *id != pane);
        }
        self.excluded.remove(&pane);
    }

    pub fn is_excluded(&self, pane: PaneId) -> bool {
        self.excluded.contains(&pane)
    }

    /// Toggles whether `pane` is opted out of broadcasting. Returns the new
    /// excluded state.
    pub fn toggle_excluded(&mut self, pane: PaneId) -> bool {
        if !self.excluded.remove(&pane) {
            self.excluded.insert(pane);
        }
        self.excluded.contains(&pane)
    }

    pub fn targets(&self, origin: PaneId, origin_session: SessionId) -> Vec<PaneId> {
        if self.excluded.contains(&origin) {
            return Vec::new();
        }

        let members = match self.group {
            BroadcastGroup::None => Vec::new(),
            BroadcastGroup::Session => self
                .session_members
                .get(&origin_session)
                .cloned()
                .unwrap_or_default(),
        };

        members
            .into_iter()
            .filter(|id| *id != origin && !self.excluded.contains(id))
            .collect()
    }
}

impl Default for BroadcastManager {
    fn default() -> Self {
        Self::new()
    }
}
