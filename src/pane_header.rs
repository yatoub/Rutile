use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use vte4::TerminalExt;

use crate::dialogs::confirm_close;
use crate::layout::PaneId;
use crate::preferences::Preferences;
use crate::session::SessionView;
use crate::terminal::broadcast::SessionId;
use crate::terminal::title;

/// Live-updated title state for one pane's header label — rebuilt into
/// text via `title::render_template` on every OSC 0/2 (`raw_title`) or
/// OSC 7 (`directory`) update, unless the user has typed a manual
/// override by editing the `GtkEditableLabel` directly (Tilix-style:
/// clearing the override text back to empty restores the template).
#[derive(Default)]
struct PaneTitleState {
    raw_title: Option<String>,
    directory: Option<String>,
    override_text: Option<String>,
}

fn refresh_title(label: &gtk4::EditableLabel, state: &PaneTitleState, pane_id: PaneId) {
    let text = match &state.override_text {
        Some(text) => text.clone(),
        None => {
            let host = title::local_hostname();
            let user = title::current_user();
            let ctx = title::TitleContext {
                id: pane_id,
                title: state.raw_title.as_deref(),
                directory: state.directory.as_deref(),
                host: host.as_deref(),
                user: user.as_deref(),
            };
            title::render_template(title::DEFAULT_TEMPLATE, &ctx)
        }
    };
    // Avoid clobbering the cursor/selection mid-edit if this ever fires
    // while the user is typing (it shouldn't — OSC updates only refresh
    // `raw_title`/`directory`, editing state is untouched by them).
    if label.text() != text {
        label.set_text(&text);
    }
}

/// Fills a pane's (empty) header box with the Tilix-style per-pane bar:
/// a live/editable title, a per-pane sync exclusion toggle, a
/// maximize/restore toggle, and a close button.
pub fn attach(
    session_view: Rc<RefCell<SessionView>>,
    prefs: Rc<RefCell<Preferences>>,
    session_id: SessionId,
    pane_id: PaneId,
    header: &gtk4::Box,
    terminal: &vte4::Terminal,
) {
    let title_label = gtk4::EditableLabel::new("");
    title_label.set_halign(gtk4::Align::Start);
    title_label.set_hexpand(true);

    let title_state = Rc::new(RefCell::new(PaneTitleState::default()));
    refresh_title(&title_label, &title_state.borrow(), pane_id);

    {
        let title_state = title_state.clone();
        let title_label = title_label.clone();
        terminal.connect_window_title_changed(move |terminal| {
            title_state.borrow_mut().raw_title = terminal.window_title().map(|s| s.to_string());
            refresh_title(&title_label, &title_state.borrow(), pane_id);
        });
    }
    {
        let title_state = title_state.clone();
        let title_label = title_label.clone();
        terminal.connect_current_directory_uri_changed(move |terminal| {
            title_state.borrow_mut().directory = terminal
                .current_directory_uri()
                .and_then(|uri| title::directory_from_uri(&uri));
            refresh_title(&title_label, &title_state.borrow(), pane_id);
        });
    }
    {
        let title_state = title_state.clone();
        let title_label_for_notify = title_label.clone();
        title_label.connect_notify_local(Some("editing"), move |label, _| {
            if label.is_editing() {
                return;
            }
            let typed = label.text().to_string();
            let mut state = title_state.borrow_mut();
            state.override_text = if typed.trim().is_empty() {
                None
            } else {
                Some(typed)
            };
            refresh_title(&title_label_for_notify, &state, pane_id);
        });
    }
    session_view
        .borrow_mut()
        .register_pane_title_label(pane_id, title_label.clone());
    header.append(&title_label);

    let sync_button = gtk4::Button::new();
    sync_button.add_css_class("flat");
    {
        // Kept in sync with the *global* broadcast group from anywhere
        // (right-click menu, another pane's own toggle) via this listener,
        // not just this button's own clicks.
        let sync_button_for_listener = sync_button.clone();
        session_view
            .borrow_mut()
            .register_sync_listener(pane_id, move |visible, excluded| {
                update_sync_button(&sync_button_for_listener, visible, excluded);
            });
    }
    {
        let session_view = session_view.clone();
        sync_button.connect_clicked(move |_| {
            session_view
                .borrow_mut()
                .toggle_pane_sync_exclusion(pane_id);
        });
    }
    header.append(&sync_button);

    let maximize_button = gtk4::Button::from_icon_name(maximize_icon_name(false));
    maximize_button.add_css_class("flat");
    maximize_button.set_tooltip_text(Some("Maximiser le pane"));
    {
        let session_view = session_view.clone();
        let maximize_button_for_click = maximize_button.clone();
        maximize_button.connect_clicked(move |_| {
            let now_maximized = session_view
                .borrow_mut()
                .toggle_maximize(session_id, pane_id);
            maximize_button_for_click.set_icon_name(maximize_icon_name(now_maximized));
            maximize_button_for_click.set_tooltip_text(Some(if now_maximized {
                "Restaurer le pane"
            } else {
                "Maximiser le pane"
            }));
        });
    }
    header.append(&maximize_button);

    let close_button = gtk4::Button::from_icon_name("window-close-symbolic");
    close_button.add_css_class("flat");
    close_button.set_tooltip_text(Some("Fermer le pane"));
    {
        let header = header.clone();
        close_button.connect_clicked(move |_| {
            let has_process = prefs.borrow().prompt_on_close_with_process
                && session_view
                    .borrow()
                    .pane_has_foreground_process(session_id, pane_id);
            let session_view = session_view.clone();
            confirm_close::confirm_close(&header, has_process, move || {
                session_view.borrow_mut().close_pane(session_id, pane_id);
            });
        });
    }
    header.append(&close_button);
}

fn maximize_icon_name(maximized: bool) -> &'static str {
    if maximized {
        "view-restore-symbolic"
    } else {
        "view-fullscreen-symbolic"
    }
}

/// Reflects the pane's sync state on its header button: hidden entirely
/// while no broadcast group is active at all; otherwise shown, either
/// "activated" or, if the user opted this pane out locally, dimmed/marked
/// excluded.
fn update_sync_button(button: &gtk4::Button, visible: bool, excluded: bool) {
    button.set_visible(visible);
    if !visible {
        return;
    }

    if excluded {
        button.set_icon_name("action-unavailable-symbolic");
        button.set_tooltip_text(Some(
            "Saisie synchro : désactivée pour ce pane (cliquer pour réactiver)",
        ));
        button.add_css_class("pane-sync-excluded");
        button.remove_css_class("suggested-action");
    } else {
        button.set_icon_name("input-keyboard-symbolic");
        button.set_tooltip_text(Some(
            "Saisie synchro active pour ce pane (cliquer pour désactiver)",
        ));
        button.remove_css_class("pane-sync-excluded");
        button.add_css_class("suggested-action");
    }
}
