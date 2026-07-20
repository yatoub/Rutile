use std::collections::HashMap;

use gtk4::prelude::*;

use crate::layout::{Direction, Orientation, PaneId, SplitTree};
use crate::terminal::TerminalWidget;

/// Owns one session's split tree together with the live terminal widgets,
/// and renders the tree into nested `GtkPaned`s. On every mutation the
/// whole widget tree is thrown away and rebuilt from scratch: the pane
/// count per session is always small, so simplicity wins over incremental
/// patching of the existing `GtkPaned` tree.
///
/// `PaneId`s are allocated by the caller (`SessionView` keeps one counter
/// shared across all sessions) so that ids stay globally unique.
///
/// Each pane renders as `[header, terminal]` stacked vertically. `header`
/// is an empty `gtk4::Box` that `PaneView` never populates itself — the
/// caller (`SessionView`/`pane_header.rs`) fills it with the Tilix-style
/// per-pane bar (sync toggle, maximize, close), since those actions need
/// session-level context `PaneView` deliberately doesn't have.
pub struct PaneView {
    tree: SplitTree,
    widgets: HashMap<PaneId, TerminalWidget>,
    headers: HashMap<PaneId, gtk4::Box>,
    /// The `[header, terminal]` wrapper actually placed into the `GtkPaned`
    /// tree — this is what gets reparented on split/close, not the bare
    /// terminal, so the header travels together with it.
    wrappers: HashMap<PaneId, gtk4::Box>,
    focused: PaneId,
    /// When set, only this pane is rendered (Tilix's per-pane "maximize").
    maximized: Option<PaneId>,
    root: gtk4::Widget,
}

impl PaneView {
    pub fn new(id: PaneId) -> Self {
        let mut this = Self {
            tree: SplitTree::new_leaf(id),
            widgets: HashMap::new(),
            headers: HashMap::new(),
            wrappers: HashMap::new(),
            focused: id,
            maximized: None,
            root: gtk4::Box::new(gtk4::Orientation::Vertical, 0).upcast(),
        };
        this.create_pane(id);
        this.root = this.wrappers[&id].clone().upcast();
        this
    }

    fn create_pane(&mut self, id: PaneId) {
        let widget = TerminalWidget::new();

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        header.add_css_class("pane-header");

        let wrapper = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        wrapper.set_vexpand(true);
        wrapper.set_hexpand(true);
        wrapper.append(&header);
        wrapper.append(widget.widget());

        self.widgets.insert(id, widget);
        self.headers.insert(id, header);
        self.wrappers.insert(id, wrapper);
    }

    fn destroy_pane(&mut self, id: PaneId) {
        self.widgets.remove(&id);
        self.headers.remove(&id);
        self.wrappers.remove(&id);
    }

    pub fn root(&self) -> &gtk4::Widget {
        &self.root
    }

    pub fn focused(&self) -> PaneId {
        self.focused
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.tree.leaves()
    }

    pub fn widget_for(&self, id: PaneId) -> Option<&vte4::Terminal> {
        self.widgets.get(&id).map(|w| w.widget())
    }

    /// The empty per-pane header box for `id`, for the caller to fill with
    /// the sync/maximize/close bar.
    pub fn header_for(&self, id: PaneId) -> Option<&gtk4::Box> {
        self.headers.get(&id)
    }

    /// Moves focus to a specific pane (e.g. the one the user right-clicked
    /// on), so that a subsequent `split`/`close` targets it instead of
    /// whatever was last focused via keyboard. Returns false if `id` isn't
    /// a leaf of this tree.
    pub fn set_focused(&mut self, id: PaneId) -> bool {
        if self.tree.find(id).is_none() {
            return false;
        }
        self.focused = id;
        self.focus_current();
        true
    }

    pub fn split(&mut self, orientation: Orientation, new_id: PaneId) {
        self.create_pane(new_id);
        self.tree.split(self.focused, orientation, new_id);

        self.focused = new_id;
        self.maximized = None;
        self.rebuild();
        self.focus_current();
    }

    /// Closes the focused pane. Returns `None` (no-op) if it's the last
    /// pane in this session — the caller should treat that as "close the
    /// session" instead. Otherwise returns the closed pane's id, so the
    /// caller can drop its broadcast-group registration.
    pub fn close_focused(&mut self) -> Option<PaneId> {
        if self.tree.is_leaf_only() {
            return None;
        }

        let closed = self.focused;
        if !self.tree.close(closed) {
            return None;
        }
        self.destroy_pane(closed);
        if self.maximized == Some(closed) {
            self.maximized = None;
        }

        self.focused = *self
            .tree
            .leaves()
            .first()
            .expect("tree has at least one leaf");
        self.rebuild();
        self.focus_current();
        Some(closed)
    }

    pub fn navigate(&mut self, direction: Direction) {
        if let Some(next) = self.tree.neighbor(self.focused, direction) {
            self.focused = next;
            self.focus_current();
        }
    }

    pub fn is_leaf_only(&self) -> bool {
        self.tree.is_leaf_only()
    }

    pub fn is_maximized(&self, id: PaneId) -> bool {
        self.maximized == Some(id)
    }

    /// Toggles Tilix-style "maximize" for `id`: while active, only that
    /// pane's wrapper is shown in place of the whole split tree. Returns
    /// the new maximized state. No-op (returns `false`) if `id` isn't a
    /// leaf of this tree.
    pub fn toggle_maximize(&mut self, id: PaneId) -> bool {
        if self.tree.find(id).is_none() {
            return false;
        }
        self.maximized = if self.maximized == Some(id) {
            None
        } else {
            Some(id)
        };
        self.rebuild();
        self.maximized == Some(id)
    }

    /// Grabs keyboard focus for the currently focused pane. Deferred to the
    /// next main-loop iteration: right after a split/close, `self.root` is
    /// a freshly built widget tree that hasn't been reattached to the
    /// window yet (the caller — `SessionView` — does that afterwards).
    /// `grab_focus()` on a widget with no realized top-level ancestor fails
    /// silently, so calling it synchronously here would just leave focus
    /// wherever it was before the split.
    fn focus_current(&self) {
        if let Some(widget) = self.widgets.get(&self.focused) {
            let terminal = widget.widget().clone();
            gtk4::glib::idle_add_local_once(move || {
                terminal.grab_focus();
            });
        }
    }

    fn rebuild(&mut self) {
        // Pane wrappers persist across rebuilds (only the surrounding
        // GtkPaned tree is thrown away), so they're still parented inside
        // the *old* Paned tree and must be detached before joining a new
        // Paned's child slot.
        for wrapper in self.wrappers.values() {
            detach_from_parent(wrapper);
        }

        self.root = match self.maximized {
            Some(id) if self.wrappers.contains_key(&id) => self.wrappers[&id].clone().upcast(),
            _ => Self::build_widget(&self.tree, &self.wrappers),
        };
    }

    fn build_widget(node: &SplitTree, wrappers: &HashMap<PaneId, gtk4::Box>) -> gtk4::Widget {
        match node {
            SplitTree::Leaf(id) => wrappers
                .get(id)
                .expect("leaf id must have a corresponding pane wrapper")
                .clone()
                .upcast(),
            SplitTree::Split {
                orientation,
                left,
                right,
                ..
            } => {
                let gtk_orientation = match orientation {
                    Orientation::Horizontal => gtk4::Orientation::Horizontal,
                    Orientation::Vertical => gtk4::Orientation::Vertical,
                };
                let paned = gtk4::Paned::new(gtk_orientation);
                paned.set_wide_handle(true);
                paned.set_start_child(Some(&Self::build_widget(left, wrappers)));
                paned.set_end_child(Some(&Self::build_widget(right, wrappers)));
                paned.set_vexpand(true);
                paned.set_hexpand(true);
                paned.upcast()
            }
        }
    }
}

/// Detaches a widget from its current parent, if any — using the parent's
/// own removal API when the parent is a `GtkPaned`, rather than a bare
/// `Widget::unparent()`.
///
/// `GtkPaned` caches its children in its own `start-child`/`end-child`
/// properties *in addition to* the generic GTK widget parent/child links.
/// Calling `unparent()` directly on the child only clears the generic
/// link — the `Paned`'s own cached pointer stays stale. When that (now
/// empty-looking, but not really) `Paned` is later disposed — e.g. right
/// after being swapped out of the session's container in
/// `SessionView::resync_page_child` — its dispose logic still walks its
/// stale `start-child`/`end-child` and unparents whatever they point to,
/// ripping widgets back out of the *new* tree they'd already been moved
/// into in the meantime. Going through `set_start_child(None)` /
/// `set_end_child(None)` keeps the `Paned`'s own bookkeeping consistent
/// and avoids that.
fn detach_from_parent(widget: &impl IsA<gtk4::Widget>) {
    let widget: &gtk4::Widget = widget.upcast_ref();
    let Some(parent) = widget.parent() else {
        return;
    };

    if let Some(paned) = parent.downcast_ref::<gtk4::Paned>() {
        if paned.start_child().as_ref() == Some(widget) {
            paned.set_start_child(gtk4::Widget::NONE);
        } else if paned.end_child().as_ref() == Some(widget) {
            paned.set_end_child(gtk4::Widget::NONE);
        }
    } else {
        widget.unparent();
    }
}
