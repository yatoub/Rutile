use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::bookmarks::{self, Bookmark, BookmarkStore};

/// Adds or edits a bookmark: name, command and folder (`/` nests folders,
/// existing ones are listed as a reminder). `on_save` gets the result once
/// "Save" is pressed — only possible with a name and a command.
pub fn show(
    parent: &impl IsA<gtk4::Widget>,
    title: &str,
    initial: Bookmark,
    on_save: impl Fn(Bookmark) + 'static,
) {
    let name_row = adw::EntryRow::builder()
        .title("Name")
        .text(initial.name.as_str())
        .build();
    let command_row = adw::EntryRow::builder()
        .title("Command")
        .text(initial.command.as_str())
        .build();
    command_row.add_css_class("monospace");
    let folder_row = adw::EntryRow::builder()
        .title("Folder (nested with /, empty: top level)")
        .text(bookmarks::format_folder(&initial.folder))
        .build();

    let existing: Vec<String> = BookmarkStore::load()
        .folders()
        .iter()
        .map(|folder| bookmarks::format_folder(folder))
        .collect();
    let group = adw::PreferencesGroup::builder()
        .description(if existing.is_empty() {
            "Inserted into the terminal as if typed.".to_string()
        } else {
            format!(
                "Inserted into the terminal as if typed. Existing folders: {}",
                existing.join(", ")
            )
        })
        .build();
    group.add(&name_row);
    group.add(&command_row);
    group.add(&folder_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let save = gtk4::Button::builder()
        .label("Save")
        .sensitive(false)
        .build();
    save.add_css_class("suggested-action");
    let header = adw::HeaderBar::new();
    header.pack_end(&save);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&page));

    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(480)
        .child(&toolbar)
        .build();

    let valid = {
        let (name_row, command_row) = (name_row.clone(), command_row.clone());
        move || !name_row.text().trim().is_empty() && !command_row.text().trim().is_empty()
    };
    for row in [&name_row, &command_row] {
        let save = save.clone();
        let valid = valid.clone();
        row.connect_changed(move |_| save.set_sensitive(valid()));
    }
    save.set_sensitive(valid());

    let on_save = Rc::new(on_save);
    let submit: Rc<dyn Fn()> = {
        let dialog = dialog.downgrade();
        let (name_row, command_row, folder_row) =
            (name_row.clone(), command_row.clone(), folder_row.clone());
        Rc::new(move || {
            if !valid() {
                return;
            }
            on_save(Bookmark {
                name: name_row.text().trim().to_string(),
                command: command_row.text().to_string(),
                folder: bookmarks::parse_folder(&folder_row.text()),
            });
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        })
    };
    {
        let submit = submit.clone();
        save.connect_clicked(move |_| submit());
    }
    // Enter in any field saves, like a form.
    for row in [&name_row, &command_row, &folder_row] {
        let submit = submit.clone();
        row.connect_entry_activated(move |_| submit());
    }

    dialog.present(Some(parent));
    name_row.grab_focus();
}

/// "Add bookmark" from a terminal: the command starts as the terminal's
/// selected text, if any — handy to keep a command line just typed.
pub fn add_from_terminal(terminal: &vte4::Terminal) {
    let selection = vte4::TerminalExt::text_selected(terminal, vte4::Format::Text)
        .map(|text| text.trim().to_string())
        .unwrap_or_default();
    let initial = Bookmark {
        name: String::new(),
        command: selection,
        folder: Vec::new(),
    };
    show(terminal, "Add Bookmark", initial, |bookmark| {
        BookmarkStore::load().add(bookmark);
    });
}
