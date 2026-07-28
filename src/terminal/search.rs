use gtk4::glib;
use gtk4::prelude::*;
use vte4::TerminalExt;

/// Tilix-style search-in-terminal overlay bar: a `gtk4::Revealer` anchored
/// to the top of a pane's `gtk4::Overlay`, holding an entry plus
/// prev/next/close buttons. Owns no terminal state itself — every action
/// just calls straight into `vte4::Terminal`'s own PCRE2-backed search
/// (`search_set_regex`/`search_find_next`/`search_find_previous`), VTE
/// keeps track of the current match.
pub struct SearchBar {
    revealer: gtk4::Revealer,
    entry: gtk4::SearchEntry,
}

impl SearchBar {
    pub fn new(terminal: &vte4::Terminal) -> Self {
        let entry = gtk4::SearchEntry::new();
        entry.set_placeholder_text(Some("Find in terminal…"));
        entry.set_hexpand(true);

        let prev = gtk4::Button::from_icon_name("go-up-symbolic");
        prev.set_tooltip_text(Some("Previous match"));
        let next = gtk4::Button::from_icon_name("go-down-symbolic");
        next.set_tooltip_text(Some("Next match"));
        let close = gtk4::Button::from_icon_name("window-close-symbolic");
        close.set_tooltip_text(Some("Close"));

        let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        bar.add_css_class("search-bar");
        bar.set_margin_top(4);
        bar.set_margin_bottom(4);
        bar.set_margin_start(4);
        bar.set_margin_end(4);
        bar.append(&entry);
        bar.append(&prev);
        bar.append(&next);
        bar.append(&close);

        let revealer = gtk4::Revealer::new();
        revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        revealer.set_halign(gtk4::Align::Fill);
        revealer.set_valign(gtk4::Align::Start);
        revealer.set_child(Some(&bar));
        revealer.set_reveal_child(false);

        {
            let terminal = terminal.clone();
            entry.connect_search_changed(move |entry| {
                set_search_pattern(&terminal, &entry.text());
                terminal.search_find_next();
            });
        }
        {
            let terminal = terminal.clone();
            entry.connect_activate(move |_| {
                terminal.search_find_next();
            });
        }
        {
            let terminal = terminal.clone();
            prev.connect_clicked(move |_| {
                terminal.search_find_previous();
            });
        }
        {
            let terminal = terminal.clone();
            next.connect_clicked(move |_| {
                terminal.search_find_next();
            });
        }
        {
            let revealer = revealer.clone();
            let terminal = terminal.clone();
            close.connect_clicked(move |_| {
                revealer.set_reveal_child(false);
                terminal.grab_focus();
            });
        }
        {
            let revealer = revealer.clone();
            let terminal = terminal.clone();
            let key_controller = gtk4::EventControllerKey::new();
            key_controller.connect_key_pressed(move |_, key, _, _| {
                if key == gtk4::gdk::Key::Escape {
                    revealer.set_reveal_child(false);
                    terminal.grab_focus();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            entry.add_controller(key_controller);
        }

        terminal.search_set_wrap_around(true);

        Self { revealer, entry }
    }

    pub fn widget(&self) -> &gtk4::Revealer {
        &self.revealer
    }

    /// Shows the bar and focuses the entry if hidden, hides it (and
    /// returns focus to the terminal) if already shown — same toggle
    /// affordance as Ctrl+Shift+F in most terminal emulators.
    pub fn toggle(&self, terminal: &vte4::Terminal) {
        if self.revealer.reveals_child() {
            self.revealer.set_reveal_child(false);
            terminal.grab_focus();
        } else {
            self.revealer.set_reveal_child(true);
            self.entry.grab_focus();
        }
    }
}

fn set_search_pattern(terminal: &vte4::Terminal, pattern: &str) {
    if pattern.is_empty() {
        terminal.search_set_regex(None::<&vte4::Regex>, 0);
        return;
    }
    match vte4::Regex::for_search(pattern, 0) {
        Ok(regex) => terminal.search_set_regex(Some(&regex), 0),
        Err(_) => terminal.search_set_regex(None::<&vte4::Regex>, 0),
    }
}
