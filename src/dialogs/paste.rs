//! Pasting the clipboard into a terminal the way Tilix does: the plain
//! paste with its cleanup and unsafe-paste warning, and the advanced paste
//! dialog. The text logic itself is in `terminal::paste`.
//!
//! Text goes in with `paste_text`, not `feed_child`: vte wraps it in
//! bracketed-paste markers when the shell asked for them, and it still
//! goes out through the terminal's `commit` signal, so synchronized panes
//! receive it like any other input.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use vte4::TerminalExt;

use crate::i18n::{n_, tr};
use crate::preferences::Preferences;
use crate::terminal::paste::{self, AdvancedTransform};

/// Set on a terminal once its user chose to paste an unsafe text anyway:
/// like Tilix, that terminal doesn't warn again.
const UNSAFE_PASTE_IGNORED: &str = "rutile-unsafe-paste-ignored";

const UNSAFE_PASTE_HEADING: &str = n_("This command asks for administrative access");
const UNSAFE_PASTE_BODY: &str = n_(
    "Copying commands from the internet can be dangerous. Be sure you understand what each part of this command does.",
);

/// The "Paste" action (Ctrl+Shift+V, context menu): the advanced paste
/// dialog if that's the default, else a plain paste.
pub fn paste_clipboard(terminal: &vte4::Terminal, prefs: &Rc<RefCell<Preferences>>) {
    if prefs.borrow().paste_advanced_default {
        advanced_paste(terminal, prefs);
        return;
    }
    with_clipboard_text(terminal, {
        let prefs = prefs.clone();
        move |terminal, text| plain_paste(&terminal, &prefs, &text)
    });
}

/// The "Paste (advanced)" action: review and transform a multi-line text
/// before pasting it. A single line is pasted right away, as Tilix does.
pub fn advanced_paste(terminal: &vte4::Terminal, prefs: &Rc<RefCell<Preferences>>) {
    with_clipboard_text(terminal, {
        let prefs = prefs.clone();
        move |terminal, text| {
            if paste::is_multiline(&text) {
                show_advanced_dialog(&terminal, &prefs, &text);
            } else {
                plain_paste(&terminal, &prefs, &text);
            }
        }
    });
}

fn with_clipboard_text(
    terminal: &vte4::Terminal,
    then: impl FnOnce(vte4::Terminal, String) + 'static,
) {
    let terminal = terminal.clone();
    terminal
        .clipboard()
        .read_text_async(gtk4::gio::Cancellable::NONE, move |result| {
            if let Ok(Some(text)) = result
                && !text.is_empty()
            {
                then(terminal, text.to_string());
            }
        });
}

/// Tilix's plain paste: cleanup from the preferences, then a warning
/// before an unsafe text.
fn plain_paste(terminal: &vte4::Terminal, prefs: &Rc<RefCell<Preferences>>, text: &str) {
    let (cleanup, alert) = {
        let prefs = prefs.borrow();
        (prefs.paste_cleanup(), prefs.unsafe_paste_alert)
    };
    let text = paste::clean(text, cleanup).to_string();
    if text.is_empty() {
        return;
    }
    if !alert || !paste::is_unsafe(&text) || unsafe_paste_ignored(terminal) {
        send(terminal, &text);
        return;
    }

    let dialog = adw::AlertDialog::new(Some(tr(UNSAFE_PASTE_HEADING)), Some(tr(UNSAFE_PASTE_BODY)));
    dialog.set_extra_child(Some(&preview(&text)));
    dialog.add_response("cancel", tr("Cancel"));
    dialog.add_response("paste", tr("Paste Anyway"));
    dialog.set_response_appearance("paste", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    {
        let terminal = terminal.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "paste" {
                // SAFETY: only ever set to, and read as, a `bool`.
                unsafe { terminal.set_data(UNSAFE_PASTE_IGNORED, true) };
                send(&terminal, &text);
            }
            terminal.grab_focus();
        });
    }
    dialog.present(Some(terminal));
}

fn unsafe_paste_ignored(terminal: &vte4::Terminal) -> bool {
    // SAFETY: only ever set to a `bool` (`plain_paste`).
    unsafe { terminal.data::<bool>(UNSAFE_PASTE_IGNORED) }
        .is_some_and(|ignored| unsafe { *ignored.as_ref() })
}

/// The text about to be pasted, read-only, in the unsafe paste warning.
fn preview(text: &str) -> gtk4::Widget {
    let label = gtk4::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk4::pango::WrapMode::WordChar)
        .selectable(true)
        .build();
    label.add_css_class("monospace");
    gtk4::ScrolledWindow::builder()
        .child(&label)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(200)
        .build()
        .upcast()
}

/// Pastes `text`, then scrolls to the bottom if the profile scrolls on
/// keystrokes — a paste is input too (Tilix does the same).
fn send(terminal: &vte4::Terminal, text: &str) {
    terminal.paste_text(text);
    crate::terminal::widget::scroll_on_input(terminal);
}

/// Tilix's advanced paste dialog: the text, editable, an unsafe warning if
/// it applies, and the transforms (remembered in the preferences).
/// Ctrl+Enter pastes.
fn show_advanced_dialog(terminal: &vte4::Terminal, prefs: &Rc<RefCell<Preferences>>, text: &str) {
    let content = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_start(18)
        .margin_end(18)
        .margin_top(6)
        .margin_bottom(18)
        .build();

    let unsafe_text = paste::is_unsafe(text);
    if unsafe_text {
        let warning = gtk4::Label::builder()
            .label(format!(
                "<b>{}</b>\n{}",
                glib::markup_escape_text(tr(UNSAFE_PASTE_HEADING)),
                glib::markup_escape_text(tr(UNSAFE_PASTE_BODY)),
            ))
            .use_markup(true)
            .wrap(true)
            .xalign(0.0)
            .build();
        warning.add_css_class("warning");
        content.append(&warning);
    }

    let buffer = gtk4::TextBuffer::new(None);
    buffer.set_text(text);
    let view = gtk4::TextView::builder()
        .buffer(&buffer)
        .monospace(true)
        .top_margin(6)
        .bottom_margin(6)
        .left_margin(6)
        .right_margin(6)
        .build();
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(160)
        .vexpand(true)
        .build();
    scrolled.add_css_class("card");
    content.append(&scrolled);

    let current = prefs.borrow().advanced_paste_transform();
    let tabs_row = adw::SpinRow::builder()
        .title(tr("Convert tabs to spaces"))
        .subtitle(tr("Tab width"))
        .adjustment(&gtk4::Adjustment::new(
            f64::from(prefs.borrow().advanced_paste_tab_width),
            1.0,
            32.0,
            1.0,
            4.0,
            0.0,
        ))
        .sensitive(current.tabs_to_spaces.is_some())
        .build();
    let tabs_switch = gtk4::Switch::builder()
        .active(current.tabs_to_spaces.is_some())
        .valign(gtk4::Align::Center)
        .build();
    tabs_row.add_prefix(&tabs_switch);
    let crlf_row = adw::SwitchRow::builder()
        .title(tr("Convert CRLF and CR to LF"))
        .active(current.normalize_line_endings)
        .build();
    let transforms = adw::PreferencesGroup::builder()
        .title(tr("Transform"))
        .build();
    transforms.add(&tabs_row);
    transforms.add(&crlf_row);
    content.append(&transforms);

    {
        let prefs = prefs.clone();
        let tabs_row = tabs_row.clone();
        tabs_switch.connect_active_notify(move |switch| {
            tabs_row.set_sensitive(switch.is_active());
            let mut prefs = prefs.borrow_mut();
            prefs.advanced_paste_tabs_to_spaces = switch.is_active();
            prefs.save();
        });
    }
    {
        let prefs = prefs.clone();
        tabs_row.connect_value_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            prefs.advanced_paste_tab_width = row.value() as u32;
            prefs.save();
        });
    }
    {
        let prefs = prefs.clone();
        crlf_row.connect_active_notify(move |row| {
            let mut prefs = prefs.borrow_mut();
            prefs.advanced_paste_normalize_line_endings = row.is_active();
            prefs.save();
        });
    }

    let paste_button = gtk4::Button::with_label(tr("Paste"));
    paste_button.add_css_class(if unsafe_text {
        "destructive-action"
    } else {
        "suggested-action"
    });
    let header = adw::HeaderBar::new();
    header.pack_end(&paste_button);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .title(tr("Advanced Paste"))
        .content_width(560)
        .content_height(440)
        .child(&toolbar)
        .build();

    let submit: Rc<dyn Fn()> = {
        let dialog = dialog.downgrade();
        let terminal = terminal.clone();
        let prefs = prefs.clone();
        Rc::new(move || {
            let (start, end) = buffer.bounds();
            let edited = buffer.text(&start, &end, false);
            let transform: AdvancedTransform = prefs.borrow().advanced_paste_transform();
            let text = paste::transform(&edited, transform);
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
            if !text.is_empty() {
                send(&terminal, &text);
            }
        })
    };
    {
        let submit = submit.clone();
        paste_button.connect_clicked(move |_| submit());
    }
    let keys = gtk4::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, state| {
        if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter)
            && state.contains(gdk::ModifierType::CONTROL_MASK)
        {
            submit();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    view.add_controller(keys);
    {
        let terminal = terminal.clone();
        dialog.connect_closed(move |_| {
            terminal.grab_focus();
        });
    }

    dialog.present(Some(terminal));
    view.grab_focus();
}
