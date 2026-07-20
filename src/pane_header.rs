use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::layout::PaneId;
use crate::session::SessionView;
use crate::terminal::broadcast::SessionId;

/// Fills a pane's (empty) header box with the Tilix-style per-pane bar:
/// a per-pane sync exclusion toggle, a maximize/restore toggle, and a
/// close button.
pub fn attach(
    session_view: Rc<RefCell<SessionView>>,
    session_id: SessionId,
    pane_id: PaneId,
    header: &gtk4::Box,
) {
    let title = gtk4::Label::new(Some(&format!("Terminal {pane_id}")));
    title.set_halign(gtk4::Align::Start);
    title.set_hexpand(true);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    header.append(&title);

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
    close_button.connect_clicked(move |_| {
        session_view.borrow_mut().close_pane(session_id, pane_id);
    });
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
