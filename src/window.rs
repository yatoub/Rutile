use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use crate::context_menu;
use crate::keymap::{self, Action};
use crate::layout::Orientation;
use crate::preferences::{self, Preferences};
use crate::session::session_view::ClosePaneOutcome;
use crate::session::{SessionSidebar, SessionView};
use crate::terminal::broadcast::SessionId;

pub fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    let header_bar = adw::HeaderBar::new();

    let session_view = Rc::new(RefCell::new(SessionView::new()));
    let prefs = Rc::new(RefCell::new(Preferences::load()));

    // Tilix-style session switcher: a left sidebar of session rows instead
    // of a top tab strip. Hidden by default — revealed via the toolbar's
    // sidebar button, which also opens a new session at the same time.
    let sidebar = SessionSidebar::new(session_view.clone());
    sidebar.widget().set_visible(false);

    build_toolbar(&header_bar, &session_view, &prefs, &sidebar);

    let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    body.append(sidebar.widget());
    body.append(&gtk4::Separator::new(gtk4::Orientation::Vertical));
    let tab_view = session_view.borrow().tab_view().clone();
    tab_view.set_hexpand(true);
    tab_view.set_vexpand(true);
    body.append(&tab_view);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&header_bar);
    content.append(&body);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Rutile")
        .default_width(900)
        .default_height(600)
        .content(&content)
        .build();

    // Wire the context menu (right-click: split + broadcast group) for the
    // initial session's initial pane.
    let initial_session_id = session_view.borrow().current_session_id();
    if let Some(session_id) = initial_session_id {
        wire_pane_context_menu(&session_view, &prefs, session_id);
    }

    // Close the whole window once the last session is closed, if the
    // preference is enabled — SessionView otherwise just leaves an empty
    // tab_view behind (no session left to switch to).
    {
        let session_view_for_listener = session_view.clone();
        let prefs = prefs.clone();
        let window_weak = window.downgrade();
        session_view
            .borrow_mut()
            .register_session_listener(move || {
                let is_empty = session_view_for_listener.borrow().session_ids().is_empty();
                if is_empty
                    && prefs.borrow().close_window_on_last_session_closed
                    && let Some(window) = window_weak.upgrade()
                {
                    window.close();
                }
            });
    }

    let new_session_action = gio::SimpleAction::new("new-session", None);
    {
        let session_view = session_view.clone();
        let prefs = prefs.clone();
        new_session_action
            .connect_activate(move |_, _| new_session_and_wire(&session_view, &prefs));
    }
    window.add_action(&new_session_action);

    let close_session_action = gio::SimpleAction::new("close-session", None);
    {
        let session_view = session_view.clone();
        close_session_action.connect_activate(move |_, _| close_current_session(&session_view));
    }
    window.add_action(&close_session_action);

    let preferences_action = gio::SimpleAction::new("preferences", None);
    {
        let prefs = prefs.clone();
        let window_weak = window.downgrade();
        preferences_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                preferences::window::build(&window, prefs.clone()).present();
            }
        });
    }
    window.add_action(&preferences_action);

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let session_view = session_view.clone();
        let prefs = prefs.clone();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, state| {
            let Some(action) = keymap::lookup(key, state) else {
                return glib::Propagation::Proceed;
            };

            match action {
                Action::SplitHorizontal | Action::SplitVertical => {
                    split_focused_and_wire(
                        &session_view,
                        &prefs,
                        keymap::orientation_for(action).unwrap(),
                    );
                }
                Action::ClosePane => {
                    let outcome = session_view.borrow_mut().close_focused_pane();
                    match outcome {
                        ClosePaneOutcome::PaneClosed | ClosePaneOutcome::SessionClosed(_) => {}
                        ClosePaneOutcome::Nothing => return glib::Propagation::Stop,
                    }
                }
                Action::Navigate(direction) => {
                    session_view.borrow_mut().navigate_focused(direction);
                }
                Action::NewSession => new_session_and_wire(&session_view, &prefs),
                Action::CloseSession => close_current_session(&session_view),
                Action::NextSession => session_view.borrow_mut().next_session(),
                Action::PrevSession => session_view.borrow_mut().prev_session(),
            }

            glib::Propagation::Stop
        });
    }
    window.add_controller(key_controller);

    // Keep the SessionView (and every session's terminal widgets) alive
    // for the lifetime of the window.
    unsafe {
        window.set_data("session-view", session_view);
        window.set_data("preferences", prefs);
    }

    window
}

/// Builds the global toolbar (packed into the headerbar): split buttons, a
/// sidebar visibility toggle, and a hamburger menu with session actions and
/// preferences.
fn build_toolbar(
    header_bar: &adw::HeaderBar,
    session_view: &Rc<RefCell<SessionView>>,
    prefs: &Rc<RefCell<Preferences>>,
    sidebar: &Rc<SessionSidebar>,
) {
    let toggle_sidebar = gtk4::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Afficher/masquer les sessions")
        .active(sidebar.widget().is_visible())
        .build();
    {
        let sidebar = sidebar.clone();
        toggle_sidebar.connect_toggled(move |button| {
            sidebar.widget().set_visible(button.is_active());
        });
    }
    header_bar.pack_start(&toggle_sidebar);

    let split_h = gtk4::Button::builder()
        .label("Split ↔")
        .tooltip_text("Diviser horizontalement (Ctrl+Shift+O)")
        .build();
    {
        let session_view = session_view.clone();
        let prefs = prefs.clone();
        split_h.connect_clicked(move |_| {
            split_focused_and_wire(&session_view, &prefs, Orientation::Horizontal)
        });
    }
    header_bar.pack_start(&split_h);

    let split_v = gtk4::Button::builder()
        .label("Split ↕")
        .tooltip_text("Diviser verticalement (Ctrl+Shift+E)")
        .build();
    {
        let session_view = session_view.clone();
        let prefs = prefs.clone();
        split_v.connect_clicked(move |_| {
            split_focused_and_wire(&session_view, &prefs, Orientation::Vertical)
        });
    }
    header_bar.pack_start(&split_v);

    let menu = gio::Menu::new();
    let session_section = gio::Menu::new();
    session_section.append(Some("Nouvelle session"), Some("win.new-session"));
    session_section.append(Some("Fermer la session"), Some("win.close-session"));
    menu.append_section(None, &session_section);

    let preferences_section = gio::Menu::new();
    preferences_section.append(Some("Préférences"), Some("win.preferences"));
    menu.append_section(None, &preferences_section);

    let menu_button = gtk4::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Menu")
        .menu_model(&menu)
        .build();
    header_bar.pack_end(&menu_button);
}

fn split_focused_and_wire(
    session_view: &Rc<RefCell<SessionView>>,
    prefs: &Rc<RefCell<Preferences>>,
    orientation: Orientation,
) {
    let split = session_view.borrow_mut().split_focused(orientation);
    if let Some((session_id, new_id)) = split {
        // `let` first, not `if let Some(x) = rc.borrow()....` — the latter
        // extends the `Ref` guard's lifetime across the whole if-let body
        // (temporary lifetime extension in scrutinee position), so the
        // `borrow_mut()` inside `context_menu::attach` would panic.
        let terminal = session_view.borrow().widget_for(session_id, new_id);
        if let Some(terminal) = terminal {
            context_menu::attach(
                session_view.clone(),
                prefs.clone(),
                session_id,
                new_id,
                &terminal,
            );
        }
    }
}

fn new_session_and_wire(session_view: &Rc<RefCell<SessionView>>, prefs: &Rc<RefCell<Preferences>>) {
    let session_id = session_view.borrow_mut().new_session();
    wire_pane_context_menu(session_view, prefs, session_id);
}

fn close_current_session(session_view: &Rc<RefCell<SessionView>>) {
    let current = session_view.borrow().current_session_id();
    if let Some(id) = current {
        session_view.borrow_mut().close_session(id);
    }
}

/// Attaches the right-click context menu to a session's (single, initial)
/// focused pane. Used right after a new session/tab is created.
fn wire_pane_context_menu(
    session_view: &Rc<RefCell<SessionView>>,
    prefs: &Rc<RefCell<Preferences>>,
    session_id: SessionId,
) {
    let pane_id = session_view.borrow().focused_pane_id(session_id);
    let Some(pane_id) = pane_id else {
        return;
    };
    let terminal = session_view.borrow().widget_for(session_id, pane_id);
    if let Some(terminal) = terminal {
        context_menu::attach(
            session_view.clone(),
            prefs.clone(),
            session_id,
            pane_id,
            &terminal,
        );
    }
}
