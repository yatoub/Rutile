use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::bookmarks::{self, Bookmark, BookmarkStore};
use crate::dialogs::bookmark_editor;
use crate::i18n::tr;
use crate::preferences::config::Preferences;

/// The "Bookmarks" page: the "include return" setting, then one group per
/// folder (top level first) listing its bookmarks with edit/delete
/// buttons, and an "Add" button. Rebuilt from a fresh `BookmarkStore::load`
/// after every change — another window may have added one meanwhile — and
/// bookmarks are found again by value (`BookmarkStore::position`) rather
/// than by an index that could have shifted.
pub struct BookmarksPage {
    page: adw::PreferencesPage,
    groups: RefCell<Vec<adw::PreferencesGroup>>,
}

impl BookmarksPage {
    pub fn new(prefs: Rc<RefCell<Preferences>>) -> Rc<Self> {
        let page = adw::PreferencesPage::builder()
            .title(tr("Bookmarks"))
            .icon_name("user-bookmarks-symbolic")
            .build();

        let settings = adw::PreferencesGroup::builder()
            .description(
                "Commands to type into the focused terminal: Ctrl+Shift+B, or the \
                 terminal's right-click menu.",
            )
            .build();
        let return_row = adw::SwitchRow::builder()
            .title(tr("Press Enter after inserting a bookmark"))
            .active(prefs.borrow().bookmark_include_return)
            .build();
        return_row.connect_active_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            prefs.bookmark_include_return = row.is_active();
            prefs.save();
        });
        settings.add(&return_row);
        page.add(&settings);

        let this = Rc::new(Self {
            page,
            groups: RefCell::new(Vec::new()),
        });
        this.rebuild();
        this
    }

    pub fn page(&self) -> &adw::PreferencesPage {
        &self.page
    }

    fn rebuild(self: &Rc<Self>) {
        for group in self.groups.borrow_mut().drain(..) {
            self.page.remove(&group);
        }

        let store = BookmarkStore::load();
        let add_button = gtk4::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text(tr("Add Bookmark"))
            .valign(gtk4::Align::Center)
            .build();
        add_button.add_css_class("flat");
        {
            let this = Rc::downgrade(self);
            add_button.connect_clicked(move |button| {
                let this = this.clone();
                let empty = Bookmark {
                    name: String::new(),
                    command: String::new(),
                    folder: Vec::new(),
                };
                bookmark_editor::show(button, tr("Add Bookmark"), empty, move |bookmark| {
                    BookmarkStore::load().add(bookmark);
                    if let Some(this) = this.upgrade() {
                        this.rebuild();
                    }
                });
            });
        }

        let mut current: Option<(Vec<String>, adw::PreferencesGroup)> = None;
        for (_, bookmark) in store.sorted() {
            let same_folder = current
                .as_ref()
                .is_some_and(|(folder, _)| *folder == bookmark.folder);
            if !same_folder {
                if let Some((_, group)) = current.take() {
                    self.push_group(group);
                }
                let title = if bookmark.folder.is_empty() {
                    tr("Bookmarks").to_string()
                } else {
                    bookmarks::format_folder(&bookmark.folder)
                };
                let group = adw::PreferencesGroup::builder()
                    .title(gtk4::glib::markup_escape_text(&title))
                    .build();
                current = Some((bookmark.folder.clone(), group));
            }
            if let Some((_, group)) = &current {
                group.add(&self.row(bookmark));
            }
        }
        if let Some((_, group)) = current {
            self.push_group(group);
        }

        let first = self.groups.borrow().first().cloned();
        match first {
            Some(group) => group.set_header_suffix(Some(&add_button)),
            None => {
                let group = adw::PreferencesGroup::builder()
                    .title(tr("Bookmarks"))
                    .description(tr("No bookmarks yet."))
                    .build();
                group.set_header_suffix(Some(&add_button));
                self.push_group(group);
            }
        }
    }

    fn push_group(&self, group: adw::PreferencesGroup) {
        self.page.add(&group);
        self.groups.borrow_mut().push(group);
    }

    fn row(self: &Rc<Self>, bookmark: &Bookmark) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(gtk4::glib::markup_escape_text(&bookmark.name))
            .subtitle(gtk4::glib::markup_escape_text(&bookmark.command))
            .subtitle_lines(1)
            .build();

        let edit = gtk4::Button::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text(tr("Edit"))
            .valign(gtk4::Align::Center)
            .build();
        edit.add_css_class("flat");
        {
            let this = Rc::downgrade(self);
            let original = bookmark.clone();
            edit.connect_clicked(move |button| {
                let this = this.clone();
                let original = original.clone();
                bookmark_editor::show(
                    button,
                    tr("Edit Bookmark"),
                    original.clone(),
                    move |edited| {
                        let mut store = BookmarkStore::load();
                        if let Some(index) = store.position(&original) {
                            store.replace(index, edited);
                        }
                        if let Some(this) = this.upgrade() {
                            this.rebuild();
                        }
                    },
                );
            });
        }

        let delete = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text(tr("Delete"))
            .valign(gtk4::Align::Center)
            .build();
        delete.add_css_class("flat");
        {
            let this = Rc::downgrade(self);
            let original = bookmark.clone();
            delete.connect_clicked(move |_| {
                let mut store = BookmarkStore::load();
                if let Some(index) = store.position(&original) {
                    store.remove(index);
                }
                if let Some(this) = this.upgrade() {
                    this.rebuild();
                }
            });
        }

        row.add_suffix(&edit);
        row.add_suffix(&delete);
        row
    }
}
