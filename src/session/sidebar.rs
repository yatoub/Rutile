use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

use crate::session::SessionView;
use crate::terminal::broadcast::SessionId;

/// Tilix-style session switcher: a scrollable list of session rows in a
/// left-hand sidebar (title + close button), replacing the top `AdwTabBar`.
/// Selecting a row switches `AdwTabView`'s selected page; selecting a page
/// another way (keyboard, closing a session) is reflected back onto the
/// row list via `AdwTabView`'s own `selected-page` notification.
pub struct SessionSidebar {
    root: gtk4::ScrolledWindow,
    list_box: gtk4::ListBox,
    /// Guards against feedback loops between the list box's selection and
    /// `AdwTabView`'s: selecting a row programmatically (to mirror the tab
    /// view) must not re-trigger the "user clicked a row" handler, and
    /// vice versa.
    syncing: Rc<Cell<bool>>,
}

impl SessionSidebar {
    pub fn new(session_view: Rc<RefCell<SessionView>>) -> Rc<Self> {
        let list_box = gtk4::ListBox::new();
        list_box.add_css_class("navigation-sidebar");
        list_box.set_selection_mode(gtk4::SelectionMode::Single);

        let root = gtk4::ScrolledWindow::builder()
            .child(&list_box)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .width_request(180)
            .vexpand(true)
            .build();

        let syncing = Rc::new(Cell::new(false));

        {
            let session_view = session_view.clone();
            let syncing = syncing.clone();
            list_box.connect_row_selected(move |_list_box, row| {
                if syncing.get() {
                    return;
                }
                let Some(row) = row else { return };
                let session_id = unsafe { row.data::<SessionId>("session-id") };
                if let Some(session_id) = session_id {
                    let session_id = unsafe { *session_id.as_ref() };
                    session_view.borrow_mut().select_session(session_id);
                }
            });
        }

        let sidebar = Rc::new(Self {
            root,
            list_box,
            syncing,
        });

        // Follow AdwTabView's own selection changes (keyboard session
        // navigation, a session closing and another becoming selected...).
        {
            let sidebar = sidebar.clone();
            let session_view_for_notify = session_view.clone();
            session_view
                .borrow()
                .tab_view()
                .connect_selected_page_notify(move |_| {
                    sidebar.highlight_current(&session_view_for_notify);
                });
        }

        {
            let sidebar = sidebar.clone();
            let session_view_for_listener = session_view.clone();
            session_view
                .borrow_mut()
                .register_session_listener(move || {
                    sidebar.rebuild(&session_view_for_listener);
                });
        }

        sidebar.rebuild(&session_view);
        sidebar
    }

    pub fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    /// Rebuilds the row list from scratch. Simplest correct approach given
    /// how few sessions a user realistically has open at once — same
    /// trade-off `PaneView` makes for its own widget tree.
    pub fn rebuild(&self, session_view: &Rc<RefCell<SessionView>>) {
        while let Some(row) = self.list_box.row_at_index(0) {
            self.list_box.remove(&row);
        }

        let (ids, current) = {
            let sv = session_view.borrow();
            (sv.session_ids(), sv.current_session_id())
        };

        for session_id in ids {
            let row = build_row(session_view, session_id);
            unsafe {
                row.set_data("session-id", session_id);
            }
            self.list_box.append(&row);

            if Some(session_id) == current {
                self.syncing.set(true);
                self.list_box.select_row(Some(&row));
                self.syncing.set(false);
            }
        }
    }

    fn highlight_current(&self, session_view: &Rc<RefCell<SessionView>>) {
        // `AdwTabView::set_selected_page()` (called e.g. from
        // `SessionView::new_session()`) emits `notify::selected-page`
        // *synchronously*, which lands here while that same method's own
        // `borrow_mut()` is still on the stack. Skipping is safe: whichever
        // code just changed the selection already knows the right session,
        // and the sidebar gets a further chance to sync on the next actual
        // (non-reentrant) notification or the next `rebuild()`.
        let Ok(current) = session_view.try_borrow().map(|sv| sv.current_session_id()) else {
            return;
        };

        let mut index = 0;
        while let Some(row) = self.list_box.row_at_index(index) {
            let row_session_id =
                unsafe { row.data::<SessionId>("session-id").map(|p| *p.as_ref()) };
            if row_session_id == current {
                self.syncing.set(true);
                self.list_box.select_row(Some(&row));
                self.syncing.set(false);
                return;
            }
            index += 1;
        }
    }
}

/// Thumbnail height in pixels; width follows the sidebar's own width.
const THUMBNAIL_HEIGHT: i32 = 90;

fn build_row(session_view: &Rc<RefCell<SessionView>>, session_id: SessionId) -> gtk4::ListBoxRow {
    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    vbox.set_margin_start(6);
    vbox.set_margin_end(6);
    vbox.set_margin_top(4);
    vbox.set_margin_bottom(4);

    // Live thumbnail: `WidgetPaintable` mirrors the session's actual
    // rendered content continuously (no manual snapshot/refresh timer
    // needed), the same way Tilix's session sidebar previews look.
    if let Some(container) = session_view.borrow().container_for(session_id) {
        let paintable = gtk4::WidgetPaintable::new(Some(&container));
        let picture = gtk4::Picture::for_paintable(&paintable);
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_size_request(-1, THUMBNAIL_HEIGHT);
        picture.add_css_class("session-thumbnail");
        vbox.append(&picture);
    }

    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);

    let label = gtk4::Label::new(Some(&format!("Session {session_id}")));
    label.set_hexpand(true);
    label.set_halign(gtk4::Align::Start);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&label);

    let close_button = gtk4::Button::from_icon_name("window-close-symbolic");
    close_button.add_css_class("flat");
    close_button.set_tooltip_text(Some("Fermer la session"));
    {
        let session_view = session_view.clone();
        close_button.connect_clicked(move |_| {
            session_view.borrow_mut().close_session(session_id);
        });
    }
    hbox.append(&close_button);

    vbox.append(&hbox);

    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&vbox));
    row
}
