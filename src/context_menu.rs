use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;
use vte4::TerminalExt;

use crate::layout::{Orientation, PaneId};
use crate::pane_header;
use crate::session::SessionView;
use crate::terminal::broadcast::{BroadcastGroup, SessionId};

/// Wires up interaction for a pane: its Tilix-style header bar (sync
/// toggle, maximize, close) and its terminal's mouse/keyboard behavior:
/// - right-click opens a context menu offering mouse-driven split
///   (horizontal/vertical) and broadcast-group selection — the same
///   actions available from the keyboard (`keymap.rs`);
/// - gaining GTK focus (e.g. a left click) marks this pane as focused, so
///   split/close/broadcast actions target whatever the user last clicked
///   into instead of a stale keyboard-driven focus;
/// - every "commit" (text this pane is about to send its child process,
///   i.e. typed/pasted input) is forwarded to the current broadcast group.
pub fn attach(
    session_view: Rc<RefCell<SessionView>>,
    session_id: SessionId,
    pane_id: PaneId,
    terminal: &vte4::Terminal,
) {
    // NB: bind the borrow to its own `let` first — `if let Some(x) =
    // rc.borrow().method() { ... }` would extend the `Ref` guard's
    // lifetime to the whole if-let body (temporary lifetime extension in
    // the scrutinee position), so the `borrow_mut()` inside
    // `pane_header::attach` below would panic ("already borrowed").
    let header = session_view.borrow().header_for(session_id, pane_id);
    if let Some(header) = header {
        pane_header::attach(session_view.clone(), session_id, pane_id, &header);
    }

    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);

    let terminal_for_menu = terminal.clone();
    let session_view_for_menu = session_view.clone();
    gesture.connect_pressed(move |_gesture, _n_press, x, y| {
        show_menu(
            session_view_for_menu.clone(),
            session_id,
            pane_id,
            &terminal_for_menu,
            x,
            y,
        );
    });
    terminal.add_controller(gesture);

    let session_view_for_focus = session_view.clone();
    let focus_controller = gtk4::EventControllerFocus::new();
    focus_controller.connect_enter(move |_controller| {
        // Reparenting during a split/close rebuild can make GTK reassign
        // focus to another pane synchronously, re-entering this handler
        // while `window.rs` still holds a `borrow_mut()` on the same
        // `RefCell` further up the call stack. That's not a real,
        // user-driven focus change, so it's safe to just skip it here —
        // the split/close logic already sets the correct focused pane
        // itself once it's done.
        if let Ok(mut session_view) = session_view_for_focus.try_borrow_mut() {
            session_view.set_focused_pane(session_id, pane_id);
        }
    });
    terminal.add_controller(focus_controller);

    let session_view_for_commit = session_view.clone();
    terminal.connect_commit(move |_terminal, text, _size| {
        session_view_for_commit
            .borrow()
            .broadcast_from(session_id, pane_id, text.as_bytes());
    });

    // Standard terminal behavior: Ctrl+D (EOF) is handled natively by vte
    // and simply ends up exiting the shell like any other exit path
    // (`exit`, `logout`, the process crashing, ...) — so instead of
    // special-casing a keystroke, react to the child actually exiting and
    // close this pane, exactly like Tilix/GNOME Terminal do. Deferred to
    // the next main-loop iteration since this signal fires *during* the
    // terminal's own teardown — tearing its pane down synchronously here
    // (unparenting/rebuilding the tree) would mutate the very widget still
    // emitting the signal.
    terminal.connect_child_exited(move |_terminal, _status| {
        let session_view = session_view.clone();
        gtk4::glib::idle_add_local_once(move || {
            session_view.borrow_mut().close_pane(session_id, pane_id);
        });
    });
}

fn show_menu(
    session_view: Rc<RefCell<SessionView>>,
    session_id: SessionId,
    pane_id: PaneId,
    terminal: &vte4::Terminal,
    x: f64,
    y: f64,
) {
    let popover = gtk4::Popover::new();
    popover.set_parent(terminal);
    popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover.set_autohide(true);
    popover.connect_closed(|popover| popover.unparent());

    let menu_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);

    {
        let session_view = session_view.clone();
        add_action(&menu_box, &popover, "Diviser horizontalement", move || {
            split_and_wire(&session_view, session_id, pane_id, Orientation::Horizontal);
        });
    }
    {
        let session_view = session_view.clone();
        add_action(&menu_box, &popover, "Diviser verticalement", move || {
            split_and_wire(&session_view, session_id, pane_id, Orientation::Vertical);
        });
    }

    menu_box.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));

    let current_group = session_view.borrow().broadcast_group();
    for (label, group) in [
        ("Saisie synchro : aucune", BroadcastGroup::None),
        ("Saisie synchro : session", BroadcastGroup::Session),
    ] {
        let checked = if group == current_group { "✓ " } else { "" };
        let label = format!("{checked}{label}");
        let session_view = session_view.clone();
        add_action(&menu_box, &popover, &label, move || {
            session_view.borrow_mut().set_broadcast_group(group);
        });
    }

    popover.set_child(Some(&menu_box));
    popover.popup();
}

fn add_action(
    container: &gtk4::Box,
    popover: &gtk4::Popover,
    label: &str,
    on_click: impl Fn() + 'static,
) {
    let button = gtk4::Button::builder()
        .label(label)
        .has_frame(false)
        .build();
    button.child().unwrap().set_halign(gtk4::Align::Start);

    let popover = popover.clone();
    button.connect_clicked(move |_| {
        on_click();
        popover.popdown();
    });

    container.append(&button);
}

/// Performs a mouse-triggered split, then wires up the newly created pane's
/// own context menu so it can be split/broadcast-toggled the same way.
fn split_and_wire(
    session_view: &Rc<RefCell<SessionView>>,
    session_id: SessionId,
    pane_id: PaneId,
    orientation: Orientation,
) {
    let Some((session_id, new_id)) =
        session_view
            .borrow_mut()
            .split_pane(session_id, pane_id, orientation)
    else {
        return;
    };
    let terminal = session_view.borrow().widget_for(session_id, new_id);
    if let Some(terminal) = terminal {
        attach(session_view.clone(), session_id, new_id, &terminal);
    }
}
