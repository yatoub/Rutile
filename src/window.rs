use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use vte4::TerminalExt;

use crate::cli::{LaunchRequest, WindowState};
use crate::context_menu;
use crate::dialogs::{bookmark_editor, bookmark_picker, confirm_close};
use crate::keymap::{self, Action, Keymap};
use crate::layout::pane_view::LayoutStyle;
use crate::layout::{Orientation, PaneId};
use crate::preferences::{self, NewInstanceMode, Preferences, SidebarPosition, WindowStyle};
use crate::profile::{ColorSchemeStore, ProfileId, ProfileResolver, ProfileStore, TerminalConfig};
use crate::session::persist::{self, SavedWindow};
use crate::session::{SessionSidebar, SessionView};
use crate::terminal::broadcast::SessionId;

const TOOLBAR_NAME: &str = "rutile-toolbar";
const BODY_NAME: &str = "rutile-body";
const SIDEBAR_SEPARATOR_NAME: &str = "rutile-sidebar-separator";

fn child_named(parent: &gtk4::Widget, name: &str) -> Option<gtk4::Widget> {
    let mut child = parent.first_child();
    while let Some(widget) = child {
        if widget.widget_name() == name {
            return Some(widget);
        }
        child = widget.next_sibling();
    }
    None
}

/// Applies the window style and sidebar position from `prefs` to one of
/// Rutile's main windows — at creation, and again from Preferences when
/// either changes (see `apply_window_preferences_to_all`).
pub fn apply_window_preferences(window: &adw::ApplicationWindow, prefs: &Preferences) {
    window.set_decorated(prefs.window_style != WindowStyle::Borderless);
    // No handles yet while `build_window` is still putting the window
    // together (it gives the SessionView its layout style itself).
    if let Some(handles) = WindowHandles::of(window)
        && let Ok(mut session_view) = handles.session_view.try_borrow_mut()
    {
        session_view.set_layout_style(layout_style(prefs));
    }
    let Some(content) = adw::prelude::AdwApplicationWindowExt::content(window) else {
        return;
    };
    if let Some(toolbar) = child_named(&content, TOOLBAR_NAME) {
        toolbar.set_visible(prefs.window_style == WindowStyle::Normal);
    }

    let Some(body) = child_named(&content, BODY_NAME).and_downcast::<gtk4::Box>() else {
        return;
    };
    let Some(separator) = child_named(body.upcast_ref(), SIDEBAR_SEPARATOR_NAME) else {
        return;
    };
    let mut sidebar = None;
    let mut tabs = None;
    let mut child = body.first_child();
    while let Some(widget) = child {
        if widget.has_css_class("rutile-sidebar") {
            sidebar = Some(widget.clone());
        } else if widget.is::<adw::TabView>() {
            tabs = Some(widget.clone());
        }
        child = widget.next_sibling();
    }
    let (Some(sidebar), Some(tabs)) = (sidebar, tabs) else {
        return;
    };
    let order = match prefs.sidebar_position {
        SidebarPosition::Left => [&sidebar, &separator, &tabs],
        SidebarPosition::Right => [&tabs, &separator, &sidebar],
    };
    body.reorder_child_after(order[0], None::<&gtk4::Widget>);
    body.reorder_child_after(order[1], Some(order[0]));
    body.reorder_child_after(order[2], Some(order[1]));
}

/// The "About" window. `adw::AboutWindow` rather than `adw::AboutDialog`:
/// the latter needs libadwaita 1.5, above the 1.4 floor (Ubuntu 24.04).
fn about_window(parent: &adw::ApplicationWindow) -> adw::AboutWindow {
    adw::AboutWindow::builder()
        .transient_for(parent)
        .modal(true)
        .application_name("Rutile")
        // Same generic icon as `resources/rutile.desktop`.
        .application_icon("utilities-terminal")
        .version(env!("CARGO_PKG_VERSION"))
        .comments(env!("CARGO_PKG_DESCRIPTION"))
        .website(env!("CARGO_PKG_REPOSITORY"))
        .issue_url(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues"))
        .license_type(gtk4::License::MitX11)
        .developer_name("Yatoub")
        .build()
}

/// The pane layout settings out of the Appearance preferences.
fn layout_style(prefs: &Preferences) -> LayoutStyle {
    LayoutStyle {
        wide_handles: prefs.wide_handles,
        title_when_single: prefs.show_title_when_single,
    }
}

/// `apply_window_preferences` on every open Rutile window.
pub fn apply_window_preferences_to_all(prefs: &Preferences) {
    for window in gtk4::Window::list_toplevels() {
        if let Ok(window) = window.downcast::<adw::ApplicationWindow>()
            && window.has_css_class("rutile-window")
        {
            apply_window_preferences(&window, prefs);
        }
    }
}

/// A window's first session(s): the session file given with `-s`, else —
/// for the first window of a plain launch only — the session saved on the
/// last clean shutdown, else a single session shaped by the command line
/// (`-p`, `-e`, `-w`, `-t`; just the default profile for a plain launch).
fn open_initial_sessions(
    session_view: &Rc<RefCell<SessionView>>,
    resolver: &ProfileResolver,
    prefs: &Rc<RefCell<Preferences>>,
    launch: &LaunchRequest,
    first_window: bool,
) {
    let saved = match &launch.session_file {
        Some(path) => SavedWindow::load_from_file(path),
        None if first_window && !launch.shapes_session() => {
            SavedWindow::load_from_file(&persist::last_session_path())
        }
        None => None,
    };
    if let Some(saved) = saved {
        let mut view = session_view.borrow_mut();
        view.replace_all_with(&saved.sessions, |id| resolver.resolve(id));
        view.set_broadcast_group(saved.broadcast_group);
        let session_ids = view.session_ids();
        if let Some(&session_id) = session_ids.get(saved.active_session_index) {
            view.select_session(session_id);
        }
    }
    // Also when the saved window had no session at all — a clean shutdown
    // after closing every session saves an empty list — rather than
    // opening onto an empty window.
    if session_view.borrow().session_ids().is_empty() {
        let default_profile = prefs.borrow().default_profile_id.clone();
        let (profile_id, config) = launch_config(resolver, launch, &default_profile);
        let cwd = launch_cwd(launch, &config);
        let session_id =
            session_view
                .borrow_mut()
                .new_session_with(profile_id, config, cwd.as_deref());
        if let Some(title) = &launch.title {
            session_view
                .borrow_mut()
                .rename_session(session_id, Some(title.clone()));
        }
    }
}

/// The profile (`-p`, by id or name — else `base_profile`) and config a
/// launch asks for, with `-e`/`-x` replacing the profile's command.
fn launch_config(
    resolver: &ProfileResolver,
    launch: &LaunchRequest,
    base_profile: &ProfileId,
) -> (ProfileId, TerminalConfig) {
    let profile_id = launch
        .profile
        .as_deref()
        .and_then(|wanted| resolver.find_profile_id(wanted))
        .unwrap_or_else(|| base_profile.clone());
    let mut config = resolver.resolve(&profile_id);
    if let Some(command) = &launch.command {
        config.profile.use_custom_command = true;
        config.profile.custom_command = command.clone();
    }
    (profile_id, config)
}

/// Where a launched terminal starts: `-w`, else the profile's own working
/// directory (`None` here, `TerminalWidget` applies it), else wherever
/// `rutile` was launched from.
fn launch_cwd(launch: &LaunchRequest, config: &TerminalConfig) -> Option<String> {
    let dir = match &launch.working_directory {
        Some(dir) => Some(dir),
        None if config.profile.working_directory.is_some() => None,
        None => launch.invocation_dir.as_ref(),
    };
    dir.map(|dir| dir.to_string_lossy().into_owned())
}

/// The per-window state `build_window` attaches to its window, so code
/// that only has the window (a launch handed to the running instance, a
/// detached session) can reach it.
struct WindowHandles {
    session_view: Rc<RefCell<SessionView>>,
    prefs: Rc<RefCell<Preferences>>,
    profiles: Rc<RefCell<ProfileStore>>,
    schemes: Rc<ColorSchemeStore>,
}

impl WindowHandles {
    fn of(window: &adw::ApplicationWindow) -> Option<Self> {
        // SAFETY: these keys are only ever set by `build_window`, with
        // exactly these types, and live as long as the window.
        unsafe {
            Some(Self {
                session_view: window
                    .data::<Rc<RefCell<SessionView>>>("session-view")?
                    .as_ref()
                    .clone(),
                prefs: window
                    .data::<Rc<RefCell<Preferences>>>("preferences")?
                    .as_ref()
                    .clone(),
                profiles: window
                    .data::<Rc<RefCell<ProfileStore>>>("profiles")?
                    .as_ref()
                    .clone(),
                schemes: window
                    .data::<Rc<ColorSchemeStore>>("schemes")?
                    .as_ref()
                    .clone(),
            })
        }
    }

    fn resolver(&self) -> ProfileResolver {
        ProfileResolver::new(self.profiles.clone(), self.schemes.clone())
    }
}

/// A launch handed over to an already open window (`--action
/// new-session`/`split-right`/`split-down`, or the "On new instance"
/// preference): opens the session or split it asks for there.
pub fn launch_into(window: &adw::ApplicationWindow, launch: &LaunchRequest, mode: NewInstanceMode) {
    let Some(handles) = WindowHandles::of(window) else {
        return;
    };
    let Some(app) = window.application().and_downcast::<adw::Application>() else {
        return;
    };
    let resolver = handles.resolver();
    match mode {
        NewInstanceMode::NewSession => {
            let default_profile = handles.prefs.borrow().default_profile_id.clone();
            let (profile_id, config) = launch_config(&resolver, launch, &default_profile);
            let cwd = launch_cwd(launch, &config);
            let session_id = handles.session_view.borrow_mut().new_session_with(
                profile_id,
                config,
                cwd.as_deref(),
            );
            if let Some(title) = &launch.title {
                handles
                    .session_view
                    .borrow_mut()
                    .rename_session(session_id, Some(title.clone()));
            }
            wire_pane_context_menu(
                &handles.session_view,
                &handles.prefs,
                &handles.profiles,
                &app,
                session_id,
            );
        }
        NewInstanceMode::SplitRight | NewInstanceMode::SplitDown => {
            let orientation = if mode == NewInstanceMode::SplitRight {
                Orientation::Horizontal
            } else {
                Orientation::Vertical
            };
            let current = handles.session_view.borrow().current_session_id();
            let session_profile = current
                .and_then(|id| handles.session_view.borrow().profile_id_for(id))
                .unwrap_or_else(|| handles.prefs.borrow().default_profile_id.clone());
            let (_, config) = launch_config(&resolver, launch, &session_profile);
            let cwd = launch_cwd(launch, &config);
            let split = handles.session_view.borrow_mut().split_focused_with(
                orientation,
                Some(&config),
                cwd.as_deref(),
            );
            if let Some((session_id, new_id)) = split {
                wire_pane_context_menu_for(
                    &handles.session_view,
                    &handles.prefs,
                    &handles.profiles,
                    &app,
                    session_id,
                    new_id,
                );
            }
        }
        NewInstanceMode::NewWindow | NewInstanceMode::FocusWindow => {}
    }
}

/// `--maximize`/`--minimize`/`--full-screen` and `--geometry`, once the
/// window is shown.
pub fn apply_launch_window_state(window: &adw::ApplicationWindow, launch: &LaunchRequest) {
    match launch.window_state {
        Some(WindowState::Maximized) => window.maximize(),
        Some(WindowState::Minimized) => window.minimize(),
        Some(WindowState::FullScreen) => window.fullscreen(),
        None => {}
    }
    let Some(geometry) = launch.geometry else {
        return;
    };
    // Cell size and the chrome around the terminal are only known once
    // the window is laid out.
    let window = window.downgrade();
    glib::idle_add_local_once(move || {
        let Some(window) = window.upgrade() else {
            return;
        };
        let Some(handles) = WindowHandles::of(&window) else {
            return;
        };
        let Some(terminal) = handles.session_view.borrow().focused_terminal() else {
            return;
        };
        let chrome_width = window.width() - terminal.width();
        let chrome_height = window.height() - terminal.height();
        let width = chrome_width + terminal.char_width() as i32 * geometry.columns as i32;
        let height = chrome_height + terminal.char_height() as i32 * geometry.rows as i32;
        window.set_default_size(width, height);
    });
}

/// Takes any session row dropped on the window's own area (the
/// terminals, the header bar…) and does nothing with it. Without it such a
/// drop would find no target and count as "dropped outside the window",
/// which detaches the session (`session::sidebar`'s drag and drop) — only
/// a drop really outside the window should.
fn accept_session_drops(content: &gtk4::Box) {
    let target = gtk4::DropTarget::new(SessionId::static_type(), gtk4::gdk::DragAction::MOVE);
    target.connect_drop(|_, _, _, _| true);
    content.add_controller(target);
}

/// `launch`: what the command line asked for (`LaunchRequest::default()`
/// for a plain launch) — it decides this window's first session, see
/// `open_initial_sessions`.
pub fn build_window(app: &adw::Application, launch: &LaunchRequest) -> adw::ApplicationWindow {
    // Before this window exists: only the very first window restores the
    // last session, a second one (another launch, a detached session)
    // shouldn't open a copy of it.
    let first_window = !app
        .windows()
        .iter()
        .any(|window| window.has_css_class("rutile-window"));

    let header_bar = adw::HeaderBar::new();

    let prefs = Rc::new(RefCell::new(Preferences::load()));
    let profiles = Rc::new(RefCell::new(ProfileStore::load()));
    // Read-only for the lifetime of the window: schemes are only ever
    // added/edited by hand-editing files under
    // `$XDG_CONFIG_HOME/rutile/schemes/`, not through any UI yet, so
    // there's nothing that needs to invalidate this snapshot.
    let schemes = Rc::new(ColorSchemeStore::load());

    let resolver = ProfileResolver::new(profiles.clone(), schemes.clone());
    let resolve_config = {
        let resolver = resolver.clone();
        move |profile_id: &ProfileId| -> TerminalConfig { resolver.resolve(profile_id) }
    };

    let keymap = Rc::new(RefCell::new(Keymap::load()));

    let resolve_default: Rc<dyn Fn() -> (ProfileId, TerminalConfig)> = {
        let prefs = prefs.clone();
        let resolve_config = resolve_config.clone();
        Rc::new(move || {
            let profile_id = prefs.borrow().default_profile_id.clone();
            let config = resolve_config(&profile_id);
            (profile_id, config)
        })
    };
    let session_view = Rc::new(RefCell::new(SessionView::new(resolve_default)));
    session_view
        .borrow_mut()
        .set_layout_style(layout_style(&prefs.borrow()));

    open_initial_sessions(&session_view, &resolver, &prefs, launch, first_window);

    // Tilix-style session switcher: a left sidebar of session rows instead
    // of a top tab strip. Hidden by default — revealed via the toolbar's
    // sidebar button, which also opens a new session at the same time.
    let sidebar = SessionSidebar::new(session_view.clone(), prefs.clone());
    sidebar.widget().set_visible(false);

    build_toolbar(&header_bar, &session_view, &prefs, &profiles, app, &sidebar);

    // Named so `apply_window_preferences` can find them again, to hide the
    // header bar or move the sidebar to the other side.
    header_bar.set_widget_name(TOOLBAR_NAME);
    let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    body.set_widget_name(BODY_NAME);
    body.set_vexpand(true);
    body.append(sidebar.widget());
    let sidebar_separator = gtk4::Separator::new(gtk4::Orientation::Vertical);
    sidebar_separator.set_widget_name(SIDEBAR_SEPARATOR_NAME);
    body.append(&sidebar_separator);
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
    // See-through background, so a profile's background transparency
    // shows the desktop behind the terminal (chrome.css) — every chrome
    // widget paints its own opaque background instead.
    window.add_css_class("rutile-window");
    apply_window_preferences(&window, &prefs.borrow());
    accept_session_drops(&content);
    {
        let window = window.downgrade();
        let session_view = session_view.clone();
        sidebar.set_detach_handler(move |session_id| {
            let Some(window) = window.upgrade() else {
                return;
            };
            // `detach_session` takes the current session.
            session_view.borrow_mut().select_session(session_id);
            detach_session(&window, &session_view);
        });
    }

    // Wire the context menu (right-click: split + broadcast group) for
    // every pane of every session that exists so far — just the one blank
    // session on a fresh launch, but a restored session can already have
    // several sessions each with their own split tree.
    {
        let session_ids = session_view.borrow().session_ids();
        for session_id in session_ids {
            let pane_ids = session_view.borrow().pane_ids_for(session_id);
            for pane_id in pane_ids {
                wire_pane_context_menu_for(
                    &session_view,
                    &prefs,
                    &profiles,
                    app,
                    session_id,
                    pane_id,
                );
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
        let prefs = prefs.clone();
        // Set once the user has confirmed closing despite an active
        // process, so the `window.close()` call that re-enters this same
        // handler (see below) doesn't just prompt again forever.
        let confirmed_close = Rc::new(Cell::new(false));
        window.connect_close_request(move |window| {
            let has_process = !confirmed_close.get()
                && prefs.borrow().prompt_on_close_with_process
                && session_view.borrow().any_pane_has_foreground_process();

            if !has_process {
                return auto_save_on_close(&session_view.borrow(), window);
            }

            let confirmed_close = confirmed_close.clone();
            let window_for_close = window.clone();
            confirm_close::confirm_close(window, true, move || {
                confirmed_close.set(true);
                window_for_close.close();
            });
            glib::Propagation::Stop
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
        let profiles = profiles.clone();
        let app = app.clone();
        let window_weak = window.downgrade();
        let resolve_config = resolve_config.clone();
        session_open_action.connect_activate(move |_, _| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            open_session(
                &window,
                session_view.clone(),
                prefs.clone(),
                profiles.clone(),
                app.clone(),
                resolve_config.clone(),
            );
        });
    }
    window.add_action(&session_open_action);

    let new_session_action = gio::SimpleAction::new("new-session", None);
    {
        let session_view = session_view.clone();
        let prefs = prefs.clone();
        let profiles = profiles.clone();
        let app = app.clone();
        new_session_action.connect_activate(move |_, _| {
            new_session_and_wire(&session_view, &prefs, &profiles, &app)
        });
    }
    window.add_action(&new_session_action);

    let close_session_action = gio::SimpleAction::new("close-session", None);
    {
        let session_view = session_view.clone();
        let prefs = prefs.clone();
        let window_weak = window.downgrade();
        close_session_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                close_current_session(&session_view, &prefs, &window);
            }
        });
    }
    window.add_action(&close_session_action);

    let preferences_action = gio::SimpleAction::new("preferences", None);
    {
        let prefs = prefs.clone();
        let profiles = profiles.clone();
        let schemes = schemes.clone();
        let keymap = keymap.clone();
        let window_weak = window.downgrade();
        preferences_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                preferences::window::build(
                    &window,
                    prefs.clone(),
                    profiles.clone(),
                    &schemes,
                    keymap.clone(),
                )
                .present();
            }
        });
    }
    window.add_action(&preferences_action);

    let about_action = gio::SimpleAction::new("about", None);
    {
        let window_weak = window.downgrade();
        about_action.connect_activate(move |_, _| {
            if let Some(window) = window_weak.upgrade() {
                about_window(&window).present();
            }
        });
    }
    window.add_action(&about_action);

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let session_view = session_view.clone();
        let prefs = prefs.clone();
        let profiles = profiles.clone();
        let app = app.clone();
        let sidebar = sidebar.clone();
        let keymap = keymap.clone();
        let window_weak = window.downgrade();
        key_controller.connect_key_pressed(move |_controller, key, _keycode, state| {
            let Some(action) = keymap.borrow().lookup(key, state) else {
                return glib::Propagation::Proceed;
            };

            match action {
                Action::SplitHorizontal | Action::SplitVertical => {
                    split_focused_and_wire(
                        &session_view,
                        &prefs,
                        &profiles,
                        &app,
                        keymap::orientation_for(action).unwrap(),
                    );
                }
                Action::ClosePane => {
                    if let Some(window) = window_weak.upgrade() {
                        let has_process = prefs.borrow().prompt_on_close_with_process && {
                            let view = session_view.borrow();
                            view.current_session_id().is_some_and(|session_id| {
                                view.focused_pane_id(session_id).is_some_and(|pane_id| {
                                    view.pane_has_foreground_process(session_id, pane_id)
                                })
                            })
                        };
                        let session_view = session_view.clone();
                        confirm_close::confirm_close(&window, has_process, move || {
                            session_view.borrow_mut().close_focused_pane();
                        });
                    }
                }
                Action::Navigate(direction) => {
                    session_view.borrow_mut().navigate_focused(direction);
                }
                Action::NewSession => new_session_and_wire(&session_view, &prefs, &profiles, &app),
                Action::CloseSession => {
                    if let Some(window) = window_weak.upgrade() {
                        close_current_session(&session_view, &prefs, &window);
                    }
                }
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
                Action::SwitchToSession(n) => {
                    let ids = session_view.borrow().session_ids();
                    let id = n.checked_sub(1).and_then(|i| ids.get(i as usize)).copied();
                    if let Some(id) = id {
                        session_view.borrow_mut().select_session(id);
                    }
                }
                Action::ResizePane(direction) => {
                    session_view.borrow_mut().resize_focused(direction);
                }
                Action::ToggleSyncCurrentPane => {
                    session_view.borrow_mut().toggle_sync_current_pane();
                }
                Action::RenameSession => sidebar.start_rename_current(&session_view),
                Action::RenamePane => session_view.borrow().start_rename_focused_pane(),
                Action::DetachSession => {
                    if let Some(window) = window_weak.upgrade() {
                        detach_session(&window, &session_view);
                    }
                }
                Action::CopyAsHtml => {
                    if let Some(terminal) = session_view.borrow().focused_terminal() {
                        terminal.copy_clipboard_format(vte4::Format::Html);
                    }
                }
                Action::PasteAdvanced => {
                    if let Some(terminal) = session_view.borrow().focused_terminal() {
                        paste_advanced(&terminal);
                    }
                }
                Action::ToggleMargin => session_view.borrow().toggle_margin_focused(),
                Action::SelectBookmark => {
                    if let Some(terminal) = session_view.borrow().focused_terminal() {
                        let include_return = prefs.borrow().bookmark_include_return;
                        bookmark_picker::pick_and_insert(&terminal, include_return);
                    }
                }
                Action::AddBookmark => {
                    if let Some(terminal) = session_view.borrow().focused_terminal() {
                        bookmark_editor::add_from_terminal(&terminal);
                    }
                }
                Action::OpenPreferences => {
                    if let Some(window) = window_weak.upgrade() {
                        let _ = WidgetExt::activate_action(&window, "win.preferences", None);
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
        window.set_data("keymap", keymap);
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
    profiles: &Rc<RefCell<ProfileStore>>,
    app: &adw::Application,
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
        let profiles = profiles.clone();
        let app = app.clone();
        split_h.connect_clicked(move |_| {
            split_focused_and_wire(
                &session_view,
                &prefs,
                &profiles,
                &app,
                Orientation::Horizontal,
            )
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
        let profiles = profiles.clone();
        let app = app.clone();
        split_v.connect_clicked(move |_| {
            split_focused_and_wire(
                &session_view,
                &prefs,
                &profiles,
                &app,
                Orientation::Vertical,
            )
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
    preferences_section.append(Some("À propos de Rutile"), Some("win.about"));
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
    profiles: &Rc<RefCell<ProfileStore>>,
    app: &adw::Application,
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
                profiles.clone(),
                app.clone(),
                session_id,
                new_id,
                &terminal,
            );
        }
    }
}

/// `Action::PasteAdvanced`: pastes the clipboard's text with the same
/// cleanup Tilix's own advanced paste applies before feeding it to the
/// child process — CRLF/CR normalized to LF and trailing whitespace on each
/// line stripped, so pasted shell commands don't pick up stray `^M`s or
/// accidentally-included trailing spaces. The heuristic "this looks unsafe
/// to paste" warning dialog Tilix also has is a separate, later addition
/// (see `docs/ROADMAP.md` Phase 5) — this is just the transform itself.
fn paste_advanced(terminal: &vte4::Terminal) {
    let terminal = terminal.clone();
    terminal
        .clipboard()
        .read_text_async(gio::Cancellable::NONE, move |result| {
            let Ok(Some(text)) = result else { return };
            let cleaned: String = text
                .replace("\r\n", "\n")
                .replace('\r', "\n")
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            terminal.feed_child(cleaned.as_bytes());
        });
}

/// `Action::DetachSession`: pulls the currently selected session out of
/// `window` and into a brand new one, Tilix-style. Reuses `build_window`
/// wholesale (it's already fully self-contained per-window) rather than
/// inventing a second construction path — the new window starts with its
/// own blank default session, which gets replaced by the detached one and
/// closed. Like every other session restore, this respawns fresh shells in
/// the same working directories rather than actually moving the live PTYs
/// (see `session::persist`'s own doc comment on that same limitation).
fn detach_session(window: &adw::ApplicationWindow, session_view: &Rc<RefCell<SessionView>>) {
    let Some(app) = window
        .application()
        .and_then(|a| a.downcast::<adw::Application>().ok())
    else {
        return;
    };
    let Some(session_id) = session_view.borrow().current_session_id() else {
        return;
    };
    let Some(saved) = session_view.borrow().session_snapshot(session_id) else {
        return;
    };

    let new_window = build_window(&app, &LaunchRequest::default());
    new_window.present();

    if let Some(handles) = WindowHandles::of(&new_window) {
        let config = handles.resolver().resolve(&saved.profile_id);
        let stale_ids = handles.session_view.borrow().session_ids();

        let restored_id = handles
            .session_view
            .borrow_mut()
            .restore_session(&saved, config);
        for pane_id in handles.session_view.borrow().pane_ids_for(restored_id) {
            wire_pane_context_menu_for(
                &handles.session_view,
                &handles.prefs,
                &handles.profiles,
                &app,
                restored_id,
                pane_id,
            );
        }
        for stale_id in stale_ids {
            handles.session_view.borrow_mut().close_session(stale_id);
        }
    }

    session_view.borrow_mut().close_session(session_id);
}

fn new_session_and_wire(
    session_view: &Rc<RefCell<SessionView>>,
    prefs: &Rc<RefCell<Preferences>>,
    profiles: &Rc<RefCell<ProfileStore>>,
    app: &adw::Application,
) {
    let session_id = session_view.borrow_mut().new_session();
    wire_pane_context_menu(session_view, prefs, profiles, app, session_id);
}

fn close_current_session(
    session_view: &Rc<RefCell<SessionView>>,
    prefs: &Rc<RefCell<Preferences>>,
    window: &adw::ApplicationWindow,
) {
    let Some(id) = session_view.borrow().current_session_id() else {
        return;
    };
    let has_process = prefs.borrow().prompt_on_close_with_process
        && session_view.borrow().session_has_foreground_process(id);
    let session_view = session_view.clone();
    confirm_close::confirm_close(window, has_process, move || {
        session_view.borrow_mut().close_session(id);
    });
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

/// Snapshots and auto-saves the window, then allows the close to proceed —
/// the "nothing to confirm" path of the `close-request` handler above.
fn auto_save_on_close(
    session_view: &SessionView,
    window: &adw::ApplicationWindow,
) -> glib::Propagation {
    let saved = snapshot_window(session_view, window);
    if let Err(err) = saved.save_to_file(&persist::last_session_path()) {
        eprintln!("[rutile] failed to auto-save session: {err}");
    }
    glib::Propagation::Proceed
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
    profiles: Rc<RefCell<ProfileStore>>,
    app: adw::Application,
    resolve_config: impl Fn(&crate::profile::ProfileId) -> TerminalConfig + 'static,
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
        view.replace_all_with(&saved.sessions, &resolve_config);
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
                wire_pane_context_menu_for(
                    &session_view,
                    &prefs,
                    &profiles,
                    &app,
                    session_id,
                    pane_id,
                );
            }
        }
    });
}

/// Attaches the right-click context menu to a session's (single, initial)
/// focused pane. Used right after a new session/tab is created.
fn wire_pane_context_menu(
    session_view: &Rc<RefCell<SessionView>>,
    prefs: &Rc<RefCell<Preferences>>,
    profiles: &Rc<RefCell<ProfileStore>>,
    app: &adw::Application,
    session_id: SessionId,
) {
    let pane_id = session_view.borrow().focused_pane_id(session_id);
    if let Some(pane_id) = pane_id {
        wire_pane_context_menu_for(session_view, prefs, profiles, app, session_id, pane_id);
    }
}

/// Same as `wire_pane_context_menu`, but for a specific pane rather than
/// "whichever one is currently focused" — used to wire up every pane of a
/// session restored from a save file, since a restored tree can already
/// have several.
fn wire_pane_context_menu_for(
    session_view: &Rc<RefCell<SessionView>>,
    prefs: &Rc<RefCell<Preferences>>,
    profiles: &Rc<RefCell<ProfileStore>>,
    app: &adw::Application,
    session_id: SessionId,
    pane_id: PaneId,
) {
    let terminal = session_view.borrow().widget_for(session_id, pane_id);
    if let Some(terminal) = terminal {
        context_menu::attach(
            session_view.clone(),
            prefs.clone(),
            profiles.clone(),
            app.clone(),
            session_id,
            pane_id,
            &terminal,
        );
    }
}
