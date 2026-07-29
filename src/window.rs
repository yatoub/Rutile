use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use vte4::TerminalExt;

use crate::context_menu;
use crate::keymap::{self, Action};
use crate::layout::{Orientation, PaneId};
use crate::preferences::{self, Preferences};
use crate::profile::{ColorScheme, ColorSchemeStore, ProfileStore};
use crate::session::persist::{self, SavedWindow};
use crate::session::session_view::ClosePaneOutcome;
use crate::session::{SessionSidebar, SessionView};
use crate::terminal::broadcast::SessionId;

pub fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    let header_bar = adw::HeaderBar::new();

    let prefs = Rc::new(RefCell::new(Preferences::load()));
    let profiles = Rc::new(RefCell::new(ProfileStore::load()));
    // Read-only for the lifetime of the window: schemes are only ever
    // added/edited by hand-editing files under
    // `$XDG_CONFIG_HOME/rutile/schemes/`, not through any UI yet, so
    // there's nothing that needs to invalidate this snapshot.
    let schemes = Rc::new(ColorSchemeStore::load());

    let resolve_scheme = {
        let profiles = profiles.clone();
        let schemes = schemes.clone();
        move |profile_id: &crate::profile::ProfileId| -> ColorScheme {
            let scheme_id = profiles
                .borrow()
                .get(profile_id)
                .map(|p| p.scheme_id.clone())
                .unwrap_or_else(|| "catppuccin-mocha".to_string());
            schemes.get_or_default(&scheme_id).clone()
        }
    };

    let default_profile_id = prefs.borrow().default_profile_id.clone();
    let initial_scheme = resolve_scheme(&default_profile_id);
    let session_view = Rc::new(RefCell::new(SessionView::new(
        default_profile_id,
        initial_scheme,
    )));

    // Auto-restore: a session saved on the last clean shutdown (see the
    // close-request handler below) takes over from the single blank
    // session `SessionView::new` just created above. Doesn't yet honor a
    // `-s`/`--session <path>` CLI override — that's Phase 5 (no CLI
    // parsing exists at all yet).
    if let Some(saved) = persist::SavedWindow::load_from_file(&persist::last_session_path()) {
        let mut session_view_mut = session_view.borrow_mut();
        session_view_mut.replace_all_with(&saved.sessions, &resolve_scheme);
        session_view_mut.set_broadcast_group(saved.broadcast_group);
        let session_ids = session_view_mut.session_ids();
        if let Some(&session_id) = session_ids.get(saved.active_session_index) {
            session_view_mut.select_session(session_id);
        }
    }

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

    // Wire the context menu (right-click: split + broadcast group) for
    // every pane of every session that exists so far — just the one blank
    // session on a fresh launch, but a restored session can already have
    // several sessions each with their own split tree.
    {
        let session_ids = session_view.borrow().session_ids();
        for session_id in session_ids {
            let pane_ids = session_view.borrow().pane_ids_for(session_id);
            for pane_id in pane_ids {
                wire_pane_context_menu_for(&session_view, &prefs, session_id, pane_id);
            }
        }
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

    // Auto-save: a clean shutdown (window closed normally, not a crash)
    // snapshots the whole window to `last_session_path()` so the next
    // launch can auto-restore it (see the load above).
    {
        let session_view = session_view.clone();
        window.connect_close_request(move |window| {
            let saved = snapshot_window(&session_view.borrow(), window);
            if let Err(err) = saved.save_to_file(&persist::last_session_path()) {
                eprintln!("[rutile] failed to auto-save session: {err}");
            }
            glib::Propagation::Proceed
        });
    }

    let session_save_as_action = gio::SimpleAction::new("session-save-as", None);
    {
        let session_view = session_view.clone();
        let window_weak = window.downgrade();
        session_save_as_action.connect_activate(move |_, _| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let saved = snapshot_window(&session_view.borrow(), &window);
            save_session_as(&window, saved);
        });
    }
    window.add_action(&session_save_as_action);

    let session_open_action = gio::SimpleAction::new("session-open", None);
    {
        let session_view = session_view.clone();
        let prefs = prefs.clone();
        let window_weak = window.downgrade();
        let resolve_scheme = resolve_scheme.clone();
        session_open_action.connect_activate(move |_, _| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            open_session(
                &window,
                session_view.clone(),
                prefs.clone(),
                resolve_scheme.clone(),
            );
        });
    }
    window.add_action(&session_open_action);

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
        let profiles = profiles.clone();
        let schemes = schemes.clone();
        let window_weak = window.downgrade();
        preferences_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                preferences::window::build(&window, prefs.clone(), profiles.clone(), &schemes)
                    .present();
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
                Action::ToggleSearch => session_view.borrow().toggle_search_focused(),
                Action::Copy => {
                    if let Some(terminal) = session_view.borrow().focused_terminal() {
                        terminal.copy_clipboard_format(vte4::Format::Text);
                    }
                }
                Action::Paste => {
                    if let Some(terminal) = session_view.borrow().focused_terminal() {
                        terminal.paste_clipboard();
                    }
                }
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
        window.set_data("profiles", profiles);
        window.set_data("schemes", schemes);
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

    let persistence_section = gio::Menu::new();
    persistence_section.append(
        Some("Enregistrer la session sous…"),
        Some("win.session-save-as"),
    );
    persistence_section.append(Some("Ouvrir une session…"), Some("win.session-open"));
    menu.append_section(None, &persistence_section);

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

/// Snapshots every session (tree, profile, per-pane cwd), the active tab,
/// window size, and broadcast group into a `SavedWindow` — the shared core
/// of auto-save and "Save Session As...".
fn snapshot_window(session_view: &SessionView, window: &adw::ApplicationWindow) -> SavedWindow {
    let session_ids = session_view.session_ids();
    let active_session_index = session_view
        .current_session_id()
        .and_then(|id| session_ids.iter().position(|&sid| sid == id))
        .unwrap_or(0);

    let sessions = session_ids
        .iter()
        .filter_map(|&session_id| session_view.session_snapshot(session_id))
        .collect();

    SavedWindow {
        sessions,
        active_session_index,
        window_width: window.width(),
        window_height: window.height(),
        broadcast_group: session_view.broadcast_group(),
    }
}

fn session_file_filter() -> gtk4::FileFilter {
    let filter = gtk4::FileFilter::new();
    filter.set_name(Some("Rutile session (*.rutile-session.toml)"));
    filter.add_pattern("*.rutile-session.toml");
    filter
}

fn save_session_as(window: &adw::ApplicationWindow, saved: SavedWindow) {
    let dialog = gtk4::FileDialog::builder()
        .title("Enregistrer la session sous")
        .initial_name("session.rutile-session.toml")
        .default_filter(&session_file_filter())
        .build();

    dialog.save(Some(window), gtk4::gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else { return };
        let Some(path) = file.path() else { return };
        if let Err(err) = saved.save_to_file(&path) {
            eprintln!("[rutile] failed to save session to {path:?}: {err}");
        }
    });
}

fn open_session(
    window: &adw::ApplicationWindow,
    session_view: Rc<RefCell<SessionView>>,
    prefs: Rc<RefCell<Preferences>>,
    resolve_scheme: impl Fn(&crate::profile::ProfileId) -> ColorScheme + 'static,
) {
    let dialog = gtk4::FileDialog::builder()
        .title("Ouvrir une session")
        .default_filter(&session_file_filter())
        .build();

    dialog.open(Some(window), gtk4::gio::Cancellable::NONE, move |result| {
        let Ok(file) = result else { return };
        let Some(path) = file.path() else { return };
        let Some(saved) = SavedWindow::load_from_file(&path) else {
            eprintln!("[rutile] failed to parse session file {path:?}");
            return;
        };

        let mut view = session_view.borrow_mut();
        view.replace_all_with(&saved.sessions, &resolve_scheme);
        view.set_broadcast_group(saved.broadcast_group);
        let session_ids = view.session_ids();
        if let Some(&session_id) = session_ids.get(saved.active_session_index) {
            view.select_session(session_id);
        }
        drop(view);

        // Every restored pane needs its context menu wired up fresh, same
        // as the ones created at launch-time auto-restore.
        for &session_id in &session_ids {
            let pane_ids = session_view.borrow().pane_ids_for(session_id);
            for pane_id in pane_ids {
                wire_pane_context_menu_for(&session_view, &prefs, session_id, pane_id);
            }
        }
    });
}

/// Attaches the right-click context menu to a session's (single, initial)
/// focused pane. Used right after a new session/tab is created.
fn wire_pane_context_menu(
    session_view: &Rc<RefCell<SessionView>>,
    prefs: &Rc<RefCell<Preferences>>,
    session_id: SessionId,
) {
    let pane_id = session_view.borrow().focused_pane_id(session_id);
    if let Some(pane_id) = pane_id {
        wire_pane_context_menu_for(session_view, prefs, session_id, pane_id);
    }
}

/// Same as `wire_pane_context_menu`, but for a specific pane rather than
/// "whichever one is currently focused" — used to wire up every pane of a
/// session restored from a save file, since a restored tree can already
/// have several.
fn wire_pane_context_menu_for(
    session_view: &Rc<RefCell<SessionView>>,
    prefs: &Rc<RefCell<Preferences>>,
    session_id: SessionId,
    pane_id: PaneId,
) {
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
