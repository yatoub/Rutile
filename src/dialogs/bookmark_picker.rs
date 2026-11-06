use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::bookmarks::{self, Bookmark, BookmarkStore};

/// Tilix's "Select bookmark" (Ctrl+Shift+B): a searchable list of every
/// bookmark — every typed word must appear in its name, command or folder.
/// Activating a row (click, or Enter in the search field for the first
/// one) hands the bookmark to `on_pick` and closes the dialog.
pub fn show(parent: &impl IsA<gtk4::Widget>, on_pick: impl Fn(Bookmark) + 'static) {
    let store = BookmarkStore::load();

    let search = gtk4::SearchEntry::builder()
        .placeholder_text("Search bookmarks")
        .hexpand(true)
        .build();
    let search_bar = gtk4::Box::builder()
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(6)
        .build();
    search_bar.append(&search);

    let list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::Browse)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(12)
        .valign(gtk4::Align::Start)
        .build();
    list.add_css_class("boxed-list");
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.add_top_bar(&search_bar);
    toolbar.set_content(Some(&scrolled));
    let dialog = adw::Dialog::builder()
        .title("Bookmarks")
        .content_width(560)
        .content_height(480)
        .child(&toolbar)
        .build();

    let on_pick = Rc::new(on_pick);
    let pick: Rc<dyn Fn(Bookmark)> = {
        let dialog = dialog.downgrade();
        Rc::new(move |bookmark| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
            on_pick(bookmark);
        })
    };

    // The rows' bookmarks, in display order, for `row-activated`.
    let shown: Rc<std::cell::RefCell<Vec<Bookmark>>> = Rc::default();
    let refill = {
        let list = list.clone();
        let shown = shown.clone();
        let empty_store = store.is_empty();
        move |query: &str| {
            while let Some(row) = list.first_child() {
                list.remove(&row);
            }
            let matches: Vec<Bookmark> = store
                .matching(query)
                .into_iter()
                .map(|(_, bookmark)| bookmark.clone())
                .collect();
            for bookmark in &matches {
                let folder = bookmarks::format_folder(&bookmark.folder);
                let subtitle = if folder.is_empty() {
                    bookmark.command.clone()
                } else {
                    format!("{folder} — {}", bookmark.command)
                };
                let row = adw::ActionRow::builder()
                    .title(glib_escape(&bookmark.name))
                    .subtitle(glib_escape(&subtitle))
                    .subtitle_lines(1)
                    .activatable(true)
                    .build();
                list.append(&row);
            }
            if matches.is_empty() {
                let message = if empty_store {
                    "No bookmarks yet — add one from the terminal's right-click menu or Preferences → Bookmarks"
                } else {
                    "No bookmark matches"
                };
                let row = adw::ActionRow::builder()
                    .title(message)
                    .activatable(false)
                    .selectable(false)
                    .build();
                list.append(&row);
            }
            shown.replace(matches);
        }
    };
    refill("");

    {
        let shown = shown.clone();
        let pick = pick.clone();
        list.connect_row_activated(move |_, row| {
            let bookmark = shown.borrow().get(row.index() as usize).cloned();
            if let Some(bookmark) = bookmark {
                pick(bookmark);
            }
        });
    }
    search.connect_search_changed(move |entry| refill(&entry.text()));
    {
        let shown = shown.clone();
        search.connect_activate(move |_| {
            let first = shown.borrow().first().cloned();
            if let Some(bookmark) = first {
                pick(bookmark);
            }
        });
    }
    // Typing while the list has the focus goes on filtering...
    search.set_key_capture_widget(Some(&list));
    // ...and Down moves from the search field to the first row.
    {
        let list = list.clone();
        let keys = gtk4::EventControllerKey::new();
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk4::gdk::Key::Down
                && let Some(row) = list.row_at_index(0)
            {
                row.grab_focus();
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
        search.add_controller(keys);
    }

    dialog.present(Some(parent));
    search.grab_focus();
}

/// `ActionRow` titles are Pango markup: a command like `a && b < c` must
/// be escaped to show as typed.
fn glib_escape(text: &str) -> String {
    gtk4::glib::markup_escape_text(text).to_string()
}

/// Picks a bookmark and types its command into `terminal` (with Enter when
/// `include_return`). Goes through `feed_child`, like a keystroke: the
/// terminal's `commit` signal fires, so an active synchronized input group
/// receives it too (`SessionView::broadcast_from`).
pub fn pick_and_insert(terminal: &vte4::Terminal, include_return: bool) {
    let target = terminal.downgrade();
    show(terminal, move |bookmark| {
        let Some(terminal) = target.upgrade() else {
            return;
        };
        let text = bookmarks::text_to_insert(&bookmark.command, include_return);
        vte4::TerminalExt::feed_child(&terminal, text.as_bytes());
        terminal.grab_focus();
    });
}
