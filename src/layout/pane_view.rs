use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::layout::{Direction, Orientation, PaneId, SplitId, SplitTree};
use crate::profile::ColorScheme;
use crate::terminal::{MarginGuide, SearchBar, TerminalWidget};

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
    /// `Rc<RefCell<_>>` (not a plain field) so that a `GtkPaned`'s
    /// `notify::position` callback — 'static, fired long after this
    /// `PaneView` value was created — can write the dragged ratio straight
    /// back into the tree without going through a full `rebuild()`.
    tree: Rc<RefCell<SplitTree>>,
    widgets: HashMap<PaneId, TerminalWidget>,
    headers: HashMap<PaneId, gtk4::Box>,
    searches: HashMap<PaneId, SearchBar>,
    margins: HashMap<PaneId, MarginGuide>,
    /// The `[header, terminal]` wrapper actually placed into the `GtkPaned`
    /// tree — this is what gets reparented on split/close, not the bare
    /// terminal, so the header travels together with it. A `gtk4::Overlay`
    /// (not a bare `Box`) so the search bar (`terminal/search.rs`) can float
    /// over the terminal instead of pushing it down.
    wrappers: HashMap<PaneId, gtk4::Overlay>,
    focused: PaneId,
    /// When set, only this pane is rendered (Tilix's per-pane "maximize").
    maximized: Option<PaneId>,
    root: gtk4::Widget,
    /// The scheme every new terminal in this session is created with.
    /// Doesn't retroactively re-theme existing panes if the profile's
    /// scheme changes later — matches Tilix, which also only applies a
    /// profile change to newly spawned terminals.
    scheme: ColorScheme,
}

impl PaneView {
    pub fn new(id: PaneId, scheme: ColorScheme) -> Self {
        let mut this = Self {
            tree: Rc::new(RefCell::new(SplitTree::new_leaf(id))),
            widgets: HashMap::new(),
            headers: HashMap::new(),
            searches: HashMap::new(),
            margins: HashMap::new(),
            wrappers: HashMap::new(),
            focused: id,
            maximized: None,
            root: gtk4::Box::new(gtk4::Orientation::Vertical, 0).upcast(),
            scheme,
        };
        this.create_pane(id);
        this.root = this.wrappers[&id].clone().upcast();
        this
    }

    /// Rebuilds a `PaneView` from a split tree restored from a saved
    /// session (`session::persist`) — `tree`'s ids are assumed to already
    /// be fresh/unique in this process (the caller remaps them via
    /// `SplitTree::remap_ids` first). `cwd_for` supplies each pane's
    /// working directory by its (already-remapped) id.
    pub fn from_tree(
        tree: SplitTree,
        scheme: ColorScheme,
        cwd_for: impl Fn(PaneId) -> Option<String>,
    ) -> Self {
        let leaves = tree.leaves();
        let focused = *leaves
            .first()
            .expect("a restored tree always has at least one leaf");
        let mut this = Self {
            tree: Rc::new(RefCell::new(tree)),
            widgets: HashMap::new(),
            headers: HashMap::new(),
            searches: HashMap::new(),
            margins: HashMap::new(),
            wrappers: HashMap::new(),
            focused,
            maximized: None,
            root: gtk4::Box::new(gtk4::Orientation::Vertical, 0).upcast(),
            scheme,
        };
        for id in leaves {
            this.create_pane_with_cwd(id, cwd_for(id).as_deref());
        }
        this.rebuild();
        this
    }

    fn create_pane(&mut self, id: PaneId) {
        self.create_pane_with_cwd(id, None);
    }

    fn create_pane_with_cwd(&mut self, id: PaneId, cwd: Option<&str>) {
        let widget = TerminalWidget::new(&self.scheme, cwd);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        header.add_css_class("pane-header");

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_vexpand(true);
        content.set_hexpand(true);
        content.append(&header);
        content.append(widget.widget());

        let search = SearchBar::new(widget.widget());
        let margin = MarginGuide::new(widget.widget());

        let wrapper = gtk4::Overlay::new();
        wrapper.set_vexpand(true);
        wrapper.set_hexpand(true);
        wrapper.set_child(Some(&content));
        wrapper.add_overlay(search.widget());
        wrapper.add_overlay(margin.widget());

        self.widgets.insert(id, widget);
        self.headers.insert(id, header);
        self.searches.insert(id, search);
        self.margins.insert(id, margin);
        self.wrappers.insert(id, wrapper);
    }

    fn destroy_pane(&mut self, id: PaneId) {
        self.widgets.remove(&id);
        self.headers.remove(&id);
        self.searches.remove(&id);
        self.margins.remove(&id);
        self.wrappers.remove(&id);
    }

    /// Toggles the Ctrl+Shift+F search overlay for the focused pane.
    /// No-op if the focused pane somehow has no search bar (shouldn't
    /// happen — every pane gets one in `create_pane_with_cwd`).
    pub fn toggle_search(&self) {
        if let (Some(search), Some(widget)) = (
            self.searches.get(&self.focused),
            self.widgets.get(&self.focused),
        ) {
            search.toggle(widget.widget());
        }
    }

    /// Toggles the 80-column margin guide for the focused pane
    /// (`Action::ToggleMargin`). No-op if the focused pane somehow has no
    /// guide (shouldn't happen — every pane gets one in
    /// `create_pane_with_cwd`).
    pub fn toggle_margin(&self) {
        if let Some(margin) = self.margins.get(&self.focused) {
            margin.toggle();
        }
    }

    pub fn root(&self) -> &gtk4::Widget {
        &self.root
    }

    pub fn focused(&self) -> PaneId {
        self.focused
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.tree.borrow().leaves()
    }

    /// A snapshot of the current split tree (ratios included), for
    /// `session::persist` to serialize. Cloned rather than borrowed since
    /// the caller needs an owned value to put into a `SavedSession`.
    pub fn tree_snapshot(&self) -> SplitTree {
        self.tree.borrow().clone()
    }

    /// The working directory a pane's shell is currently in, for
    /// `session::persist` to save alongside the tree — see
    /// `TerminalWidget::current_directory`'s doc comment for when this is
    /// `None`.
    pub fn cwd_for(&self, id: PaneId) -> Option<String> {
        self.widgets.get(&id)?.current_directory()
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
        if self.tree.borrow().find(id).is_none() {
            return false;
        }
        self.focused = id;
        self.focus_current();
        true
    }

    pub fn split(&mut self, orientation: Orientation, new_id: PaneId, split_id: SplitId) {
        self.create_pane(new_id);
        self.tree
            .borrow_mut()
            .split(self.focused, orientation, new_id, split_id);

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
        if self.tree.borrow().is_leaf_only() {
            return None;
        }

        let closed = self.focused;
        if !self.tree.borrow_mut().close(closed) {
            return None;
        }
        self.destroy_pane(closed);
        if self.maximized == Some(closed) {
            self.maximized = None;
        }

        self.focused = *self
            .tree
            .borrow()
            .leaves()
            .first()
            .expect("tree has at least one leaf");
        self.rebuild();
        self.focus_current();
        Some(closed)
    }

    pub fn navigate(&mut self, direction: Direction) {
        if let Some(next) = self.tree.borrow().neighbor(self.focused, direction) {
            self.focused = next;
            self.focus_current();
        }
    }

    /// Grows/shrinks the focused pane towards `direction` by nudging the
    /// nearest matching-orientation split's ratio (see
    /// `SplitTree::find_resizable_ancestor`). No-op (returns `false`) if
    /// there's no split on that axis at all (e.g. a single pane, or the
    /// axis only splits somewhere unrelated to the focused pane).
    pub fn resize_focused(&mut self, direction: Direction) -> bool {
        const STEP: f32 = 0.05;
        let axis = match direction {
            Direction::Left | Direction::Right => Orientation::Horizontal,
            Direction::Up | Direction::Down => Orientation::Vertical,
        };
        let Some((split_id, is_left, ratio)) = self
            .tree
            .borrow()
            .find_resizable_ancestor(self.focused, axis)
        else {
            return false;
        };

        // Growing the pane on the "start" (left/top) side means giving it a
        // bigger share, i.e. increasing the ratio, when the requested
        // direction points away from the divider (Right/Down); growing the
        // "end" (right/bottom) side means the opposite (Left/Up growing it
        // by shrinking the other side's ratio).
        let grows_start_side = matches!(direction, Direction::Right | Direction::Down);
        let delta = if grows_start_side == is_left {
            STEP
        } else {
            -STEP
        };

        self.tree.borrow_mut().set_ratio(split_id, ratio + delta);
        self.rebuild();
        true
    }

    pub fn is_leaf_only(&self) -> bool {
        self.tree.borrow().is_leaf_only()
    }

    pub fn is_maximized(&self, id: PaneId) -> bool {
        self.maximized == Some(id)
    }

    /// Toggles Tilix-style "maximize" for `id`: while active, only that
    /// pane's wrapper is shown in place of the whole split tree. Returns
    /// the new maximized state. No-op (returns `false`) if `id` isn't a
    /// leaf of this tree.
    pub fn toggle_maximize(&mut self, id: PaneId) -> bool {
        if self.tree.borrow().find(id).is_none() {
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
            _ => {
                let node = self.tree.borrow();
                Self::build_widget(&self.tree, &node, &self.wrappers)
            }
        };
    }

    /// `tree` is the whole-tree handle cloned into each `GtkPaned`'s
    /// `notify::position` callback (so a drag can write its ratio straight
    /// back via `SplitTree::set_ratio` without a full `rebuild()`); `node`
    /// is the already-borrowed subtree currently being rendered — kept
    /// separate so this recursive read doesn't have to re-borrow `tree` at
    /// every level.
    fn build_widget(
        tree: &Rc<RefCell<SplitTree>>,
        node: &SplitTree,
        wrappers: &HashMap<PaneId, gtk4::Overlay>,
    ) -> gtk4::Widget {
        match node {
            SplitTree::Leaf(id) => wrappers
                .get(id)
                .expect("leaf id must have a corresponding pane wrapper")
                .clone()
                .upcast(),
            SplitTree::Split {
                id: split_id,
                orientation,
                ratio,
                left,
                right,
            } => {
                let gtk_orientation = match orientation {
                    Orientation::Horizontal => gtk4::Orientation::Horizontal,
                    Orientation::Vertical => gtk4::Orientation::Vertical,
                };
                let paned = gtk4::Paned::new(gtk_orientation);
                paned.set_wide_handle(true);
                paned.set_start_child(Some(&Self::build_widget(tree, left, wrappers)));
                paned.set_end_child(Some(&Self::build_widget(tree, right, wrappers)));
                paned.set_vexpand(true);
                paned.set_hexpand(true);

                // Deferred: the paned has no allocated size yet right after
                // construction, so setting `position` (an absolute pixel
                // offset, unlike the tree's normalized `ratio`) now would
                // just be clamped to 0. Also guards against the `notify`
                // handler firing on this programmatic set and immediately
                // writing a bogus ratio back into the tree.
                let split_id = *split_id;
                let initial_ratio = *ratio;
                let self_initiated = Rc::new(Cell::new(false));
                {
                    let paned = paned.clone();
                    let self_initiated = self_initiated.clone();
                    gtk4::glib::idle_add_local_once(move || {
                        let extent = if paned.orientation() == gtk4::Orientation::Horizontal {
                            paned.width()
                        } else {
                            paned.height()
                        };
                        if extent > 0 {
                            self_initiated.set(true);
                            paned.set_position((extent as f32 * initial_ratio) as i32);
                            self_initiated.set(false);
                        }
                    });
                }

                let tree = tree.clone();
                paned.connect_notify_local(Some("position"), move |paned, _pspec| {
                    if self_initiated.get() {
                        return;
                    }
                    let extent = if paned.orientation() == gtk4::Orientation::Horizontal {
                        paned.width()
                    } else {
                        paned.height()
                    };
                    if extent <= 0 {
                        return;
                    }
                    let ratio = paned.position() as f32 / extent as f32;
                    if let Ok(mut tree) = tree.try_borrow_mut() {
                        tree.set_ratio(split_id, ratio);
                    }
                });

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
