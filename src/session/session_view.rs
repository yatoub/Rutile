use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use vte4::TerminalExt;

use crate::layout::pane_view::PaneView;
use crate::layout::{Direction, Orientation, PaneId};
use crate::terminal::broadcast::{BroadcastGroup, BroadcastManager, SessionId};

/// Result of attempting to close the focused pane of the current session.
pub enum ClosePaneOutcome {
    PaneClosed,
    /// The closed pane was the last one in its session: the whole session
    /// (tab) was closed too. Carries the id of a still-open session to
    /// switch focus to, or `None` if that was the very last session.
    SessionClosed(Option<SessionId>),
    /// Nothing to close (no sessions left at all).
    Nothing,
}

/// Owns every session (`AdwTabPage` <-> `PaneView`, one independent split
/// tree per tab) plus the broadcast group shared across all of them.
pub struct SessionView {
    tab_view: adw::TabView,
    sessions: HashMap<SessionId, PaneView>,
    /// A single-child wrapper per session: `AdwTabPage`'s child can't be
    /// swapped after the page is created, so each page's real (and stable)
    /// child is this wrapper, whose single child we replace on rebuild.
    containers: HashMap<SessionId, gtk4::Box>,
    pages: HashMap<SessionId, adw::TabPage>,
    session_of_page: HashMap<adw::TabPage, SessionId>,
    broadcast: BroadcastManager,
    /// Guards against re-entrant broadcasting: `feed_child()` on a target
    /// terminal simulates keyboard input, which re-fires that terminal's
    /// own "commit" signal — without this guard, broadcasting would cascade
    /// into every other pane broadcasting right back.
    broadcasting: Cell<bool>,
    /// Per-pane callbacks (`visible`, `excluded`) that keep each pane
    /// header's sync button in sync with the *global* broadcast group —
    /// registered by `pane_header::attach`, invoked whenever the group
    /// changes anywhere (context menu, another pane's button, etc.) or
    /// when this pane's own exclusion is toggled.
    sync_listeners: HashMap<PaneId, Box<dyn Fn(bool, bool)>>,
    /// Invoked whenever a session is created or closed — the session
    /// sidebar (`session::sidebar`) uses this to know when to rebuild its
    /// row list, instead of every session-mutating call site having to
    /// remember to notify it directly. `Rc` (not `Box`) so
    /// `notify_session_listeners` can cheaply clone them out before
    /// deferring their invocation (see its doc comment for why deferral is
    /// needed).
    session_listeners: Vec<Rc<dyn Fn()>>,
    next_session_id: SessionId,
    next_pane_id: PaneId,
}

impl SessionView {
    pub fn new() -> Self {
        let mut this = Self {
            tab_view: adw::TabView::new(),
            sessions: HashMap::new(),
            containers: HashMap::new(),
            pages: HashMap::new(),
            session_of_page: HashMap::new(),
            broadcast: BroadcastManager::new(),
            broadcasting: Cell::new(false),
            sync_listeners: HashMap::new(),
            session_listeners: Vec::new(),
            next_session_id: 0,
            next_pane_id: 0,
        };
        this.new_session();
        this
    }

    pub fn tab_view(&self) -> &adw::TabView {
        &self.tab_view
    }

    pub fn broadcast_group(&self) -> BroadcastGroup {
        self.broadcast.group()
    }

    pub fn set_broadcast_group(&mut self, group: BroadcastGroup) {
        self.broadcast.set_group(group);
        let visible = !matches!(group, BroadcastGroup::None);
        for (pane_id, listener) in &self.sync_listeners {
            listener(visible, self.broadcast.is_excluded(*pane_id));
        }
    }

    /// Registers a callback invoked with `(visible, excluded)` whenever the
    /// broadcast group or this pane's own exclusion changes — the sync
    /// button is only shown at all while a group is active. Also invokes
    /// it immediately with the current state.
    pub fn register_sync_listener(
        &mut self,
        pane_id: PaneId,
        listener: impl Fn(bool, bool) + 'static,
    ) {
        let visible = !matches!(self.broadcast.group(), BroadcastGroup::None);
        listener(visible, self.broadcast.is_excluded(pane_id));
        self.sync_listeners.insert(pane_id, Box::new(listener));
    }

    pub fn current_session_id(&self) -> Option<SessionId> {
        let page = self.tab_view.selected_page()?;
        self.session_of_page.get(&page).copied()
    }

    /// Session ids in tab order (matches `AdwTabView`'s own page order),
    /// for the sidebar to render its rows in a stable, sensible sequence.
    pub fn session_ids(&self) -> Vec<SessionId> {
        let pages = self.tab_view.pages();
        (0..pages.n_items())
            .filter_map(|i| {
                let page: adw::TabPage = pages.item(i)?.downcast().ok()?;
                self.session_of_page.get(&page).copied()
            })
            .collect()
    }

    /// Selects a session by id (e.g. from a sidebar row click).
    pub fn select_session(&mut self, session_id: SessionId) {
        if let Some(page) = self.pages.get(&session_id) {
            self.tab_view.set_selected_page(page);
        }
    }

    /// Registers a callback invoked whenever a session is created or
    /// closed. Also invoked immediately isn't needed here (unlike the sync
    /// listener) since the sidebar does its own initial `rebuild()` right
    /// after registering.
    pub fn register_session_listener(&mut self, listener: impl Fn() + 'static) {
        self.session_listeners.push(Rc::new(listener));
    }

    /// Deferred to the next main-loop iteration: the sidebar's listener
    /// needs to `borrow()` this same `SessionView` (wrapped in
    /// `Rc<RefCell<_>>` by the caller) to rebuild itself, but
    /// `new_session`/`close_session` are invoked through a `borrow_mut()`
    /// that's still on the stack when this runs — calling listeners
    /// synchronously here would panic ("already mutably borrowed").
    fn notify_session_listeners(&self) {
        let listeners = self.session_listeners.clone();
        glib::idle_add_local_once(move || {
            for listener in &listeners {
                listener();
            }
        });
    }

    pub fn new_session(&mut self) -> SessionId {
        let session_id = self.next_session_id;
        self.next_session_id += 1;

        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;

        let pane_view = PaneView::new(pane_id);
        self.broadcast.register_pane(session_id, pane_id);

        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(pane_view.root());

        let page = self.tab_view.append(&container);
        self.tab_view.set_selected_page(&page);

        self.containers.insert(session_id, container);
        self.pages.insert(session_id, page.clone());
        self.session_of_page.insert(page, session_id);
        self.sessions.insert(session_id, pane_view);

        self.notify_session_listeners();
        session_id
    }

    pub fn close_session(&mut self, id: SessionId) {
        if let Some(pane_view) = self.sessions.remove(&id) {
            for pane_id in pane_view.pane_ids() {
                self.broadcast.unregister_pane(id, pane_id);
                self.sync_listeners.remove(&pane_id);
            }
        }
        self.containers.remove(&id);
        if let Some(page) = self.pages.remove(&id) {
            self.session_of_page.remove(&page);
            self.tab_view.close_page(&page);
        }
        self.notify_session_listeners();
    }

    pub fn next_session(&mut self) {
        self.tab_view.select_next_page();
    }

    pub fn prev_session(&mut self) {
        self.tab_view.select_previous_page();
    }

    /// Splits the currently focused pane of the currently selected session.
    /// Returns the new pane's `(SessionId, PaneId)`, e.g. so the caller can
    /// wire up its right-click context menu.
    pub fn split_focused(&mut self, orientation: Orientation) -> Option<(SessionId, PaneId)> {
        let session_id = self.current_session_id()?;
        let new_id = self.split_in_session(session_id, orientation)?;
        Some((session_id, new_id))
    }

    /// Splits a specific pane (e.g. the one under a right-click), focusing
    /// it first so `close`/further splits target it as expected.
    pub fn split_pane(
        &mut self,
        session_id: SessionId,
        pane_id: PaneId,
        orientation: Orientation,
    ) -> Option<(SessionId, PaneId)> {
        if !self.sessions.get_mut(&session_id)?.set_focused(pane_id) {
            return None;
        }
        let new_id = self.split_in_session(session_id, orientation)?;
        Some((session_id, new_id))
    }

    fn split_in_session(
        &mut self,
        session_id: SessionId,
        orientation: Orientation,
    ) -> Option<PaneId> {
        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;

        let pane_view = self.sessions.get_mut(&session_id)?;
        pane_view.split(orientation, pane_id);
        self.broadcast.register_pane(session_id, pane_id);
        self.resync_page_child(session_id);
        Some(pane_id)
    }

    pub fn focused_pane_id(&self, session_id: SessionId) -> Option<PaneId> {
        self.sessions
            .get(&session_id)
            .map(|pane_view| pane_view.focused())
    }

    pub fn widget_for(&self, session_id: SessionId, pane_id: PaneId) -> Option<vte4::Terminal> {
        self.sessions.get(&session_id)?.widget_for(pane_id).cloned()
    }

    /// The empty per-pane header box for `pane_id`, for the caller
    /// (`pane_header.rs`) to fill with the sync/maximize/close bar.
    pub fn header_for(&self, session_id: SessionId, pane_id: PaneId) -> Option<gtk4::Box> {
        self.sessions.get(&session_id)?.header_for(pane_id).cloned()
    }

    /// The session's stable content wrapper (survives pane split/close
    /// rebuilds — see the `containers` field doc). The sidebar mirrors it
    /// live into a thumbnail via `gtk4::WidgetPaintable`.
    pub fn container_for(&self, session_id: SessionId) -> Option<gtk4::Box> {
        self.containers.get(&session_id).cloned()
    }

    pub fn is_maximized(&self, session_id: SessionId, pane_id: PaneId) -> bool {
        self.sessions
            .get(&session_id)
            .is_some_and(|pv| pv.is_maximized(pane_id))
    }

    /// Toggles Tilix-style per-pane maximize. Returns the new maximized
    /// state (`false` if the session/pane no longer exists).
    pub fn toggle_maximize(&mut self, session_id: SessionId, pane_id: PaneId) -> bool {
        let Some(pane_view) = self.sessions.get_mut(&session_id) else {
            return false;
        };
        let now_maximized = pane_view.toggle_maximize(pane_id);
        self.resync_page_child(session_id);
        now_maximized
    }

    pub fn is_pane_sync_excluded(&self, pane_id: PaneId) -> bool {
        self.broadcast.is_excluded(pane_id)
    }

    /// Toggles whether `pane_id` is temporarily opted out of the broadcast
    /// group (its header's sync button) — it neither sends nor receives
    /// broadcast input while excluded, regardless of the group setting.
    /// Returns the new excluded state.
    pub fn toggle_pane_sync_exclusion(&mut self, pane_id: PaneId) -> bool {
        let now_excluded = self.broadcast.toggle_excluded(pane_id);
        if let Some(listener) = self.sync_listeners.get(&pane_id) {
            let visible = !matches!(self.broadcast.group(), BroadcastGroup::None);
            listener(visible, now_excluded);
        }
        now_excluded
    }

    /// Forwards `bytes` (from `pane_id`'s "commit" signal — i.e. what it's
    /// about to send its child process) to every pane targeted by the
    /// current broadcast group. Only needs `&self`: the mutation is confined
    /// to the `Cell` re-entrancy guard, so this can safely be called from a
    /// GTK signal handler that fires while other code holds a `borrow_mut`
    /// on the `RefCell<SessionView>` elsewhere (e.g. mid-split).
    pub fn broadcast_from(&self, origin_session: SessionId, origin_pane: PaneId, bytes: &[u8]) {
        if self.broadcasting.get() {
            return;
        }
        self.broadcasting.set(true);

        for pane_id in self.broadcast.targets(origin_pane, origin_session) {
            for pane_view in self.sessions.values() {
                if let Some(widget) = pane_view.widget_for(pane_id) {
                    widget.feed_child(bytes);
                    break;
                }
            }
        }

        self.broadcasting.set(false);
    }

    /// Marks `pane_id` as the focused pane of its session — called whenever
    /// GTK focus lands on that pane's terminal (mouse click, Tab, etc.), so
    /// that subsequent split/close/broadcast actions target whatever the
    /// user last clicked into rather than a stale keyboard-driven focus.
    pub fn set_focused_pane(&mut self, session_id: SessionId, pane_id: PaneId) {
        if let Some(pane_view) = self.sessions.get_mut(&session_id) {
            pane_view.set_focused(pane_id);
        }
    }

    pub fn close_focused_pane(&mut self) -> ClosePaneOutcome {
        let Some(session_id) = self.current_session_id() else {
            return ClosePaneOutcome::Nothing;
        };
        self.close_pane_in_session(session_id)
    }

    /// Closes a specific pane (e.g. from its header's close button),
    /// regardless of whether it's currently focused.
    pub fn close_pane(&mut self, session_id: SessionId, pane_id: PaneId) -> ClosePaneOutcome {
        let Some(pane_view) = self.sessions.get_mut(&session_id) else {
            return ClosePaneOutcome::Nothing;
        };
        if !pane_view.set_focused(pane_id) {
            return ClosePaneOutcome::Nothing;
        }
        self.close_pane_in_session(session_id)
    }

    fn close_pane_in_session(&mut self, session_id: SessionId) -> ClosePaneOutcome {
        let closed = self
            .sessions
            .get_mut(&session_id)
            .and_then(|pane_view| pane_view.close_focused());

        match closed {
            Some(pane_id) => {
                self.broadcast.unregister_pane(session_id, pane_id);
                self.sync_listeners.remove(&pane_id);
                self.resync_page_child(session_id);
                ClosePaneOutcome::PaneClosed
            }
            None => {
                self.close_session(session_id);
                ClosePaneOutcome::SessionClosed(self.current_session_id())
            }
        }
    }

    pub fn navigate_focused(&mut self, direction: Direction) {
        if let Some(session_id) = self.current_session_id()
            && let Some(pane_view) = self.sessions.get_mut(&session_id)
        {
            pane_view.navigate(direction);
        }
    }

    /// After a split/close rebuilds a `PaneView`'s widget tree, re-attach
    /// its new root inside that session's stable container.
    fn resync_page_child(&self, session_id: SessionId) {
        if let (Some(pane_view), Some(container)) = (
            self.sessions.get(&session_id),
            self.containers.get(&session_id),
        ) {
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }
            container.append(pane_view.root());
        }
    }
}

impl Default for SessionView {
    fn default() -> Self {
        Self::new()
    }
}
