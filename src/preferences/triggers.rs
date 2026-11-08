//! The trigger list editor, shared by the global Advanced page and the
//! profile editor's Advanced tab.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::i18n::{tr, tr_args};
use crate::terminal::triggers::{self, ParameterIssue, Trigger, TriggerAction};

/// A group listing `initial` with add/edit/remove. `on_change` gets the
/// whole list after every edit (saved and applied by the caller). An
/// invalid pattern is kept — so it can be finished — but flagged, and
/// terminals skip it.
pub fn group(
    title: &str,
    description: &str,
    initial: Vec<Trigger>,
    on_change: impl Fn(Vec<Trigger>) + 'static,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::builder()
        .title(title)
        .description(description)
        .build();
    let add = gtk4::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text(tr("Add a trigger"))
        .valign(gtk4::Align::Center)
        .build();
    add.add_css_class("flat");
    group.set_header_suffix(Some(&add));

    let editor = Rc::new(Editor {
        group: group.downgrade(),
        triggers: RefCell::new(initial),
        rows: RefCell::default(),
        on_change: Box::new(on_change),
    });
    editor.rebuild(None);
    {
        let editor = editor.clone();
        add.connect_clicked(move |_| {
            editor.triggers.borrow_mut().push(Trigger {
                pattern: String::new(),
                action: TriggerAction::default(),
                parameter: String::new(),
            });
            let new_index = editor.triggers.borrow().len() - 1;
            editor.changed();
            editor.rebuild(Some(new_index));
        });
    }
    // The rows' callbacks only hold weak refs to the editor; the group
    // keeps it alive.
    unsafe { group.set_data("rutile-trigger-editor", editor) };
    group
}

struct Editor {
    group: gtk4::glib::WeakRef<adw::PreferencesGroup>,
    triggers: RefCell<Vec<Trigger>>,
    rows: RefCell<Vec<gtk4::Widget>>,
    on_change: Box<dyn Fn(Vec<Trigger>)>,
}

impl Editor {
    fn changed(&self) {
        (self.on_change)(self.triggers.borrow().clone());
    }

    /// Rebuilds every row, `expanded` (a just-added trigger) open.
    fn rebuild(self: &Rc<Self>, expanded: Option<usize>) {
        let Some(group) = self.group.upgrade() else {
            return;
        };
        for row in self.rows.borrow_mut().drain(..) {
            group.remove(&row);
        }
        let triggers = self.triggers.borrow().clone();
        if triggers.is_empty() {
            let empty = adw::ActionRow::builder()
                .title(tr("No triggers"))
                .subtitle(tr("Add one with the + button"))
                .sensitive(false)
                .build();
            group.add(&empty);
            self.rows.borrow_mut().push(empty.upcast());
            return;
        }
        for (index, trigger) in triggers.iter().enumerate() {
            let row = self.row(index, trigger);
            row.set_expanded(expanded == Some(index));
            group.add(&row);
            self.rows.borrow_mut().push(row.upcast());
        }
    }

    fn row(self: &Rc<Self>, index: usize, trigger: &Trigger) -> adw::ExpanderRow {
        let row = adw::ExpanderRow::new();
        describe(&row, trigger);

        let pattern = adw::EntryRow::builder()
            .title(tr("Regular expression"))
            .text(trigger.pattern.as_str())
            .build();
        pattern.add_css_class("monospace");
        flag_pattern(&pattern, &trigger.pattern);

        let action = adw::ComboRow::builder()
            .factory(&super::combo_value_factory())
            .title(tr("Action"))
            .model(&gtk4::StringList::new(
                &TriggerAction::ALL.map(TriggerAction::label),
            ))
            .build();
        if let Some(position) = TriggerAction::ALL.iter().position(|a| *a == trigger.action) {
            action.set_selected(position as u32);
        }

        let parameter = adw::EntryRow::builder()
            .title(parameter_title(trigger.action))
            .text(trigger.parameter.as_str())
            .sensitive(trigger.action != TriggerAction::PlayBell)
            .build();
        // What an active action means, and what's wrong with its command.
        let notice = adw::ActionRow::builder().activatable(false).build();
        notice.add_prefix(&gtk4::Image::from_icon_name("dialog-warning-symbolic"));
        // Error messages are plain text, not Pango markup.
        notice.set_use_markup(false);
        notice.set_title_lines(0);
        notice.set_subtitle_lines(0);
        update_notice(&notice, &parameter, trigger.action, &trigger.parameter);

        let remove = adw::ActionRow::builder()
            .title(tr("Remove this trigger"))
            .build();
        let remove_button = gtk4::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk4::Align::Center)
            .build();
        remove_button.add_css_class("flat");
        remove_button.add_css_class("destructive-action");
        remove.add_suffix(&remove_button);
        remove.set_activatable_widget(Some(&remove_button));

        row.add_row(&pattern);
        row.add_row(&action);
        row.add_row(&parameter);
        row.add_row(&notice);
        row.add_row(&remove);

        let weak = Rc::downgrade(self);
        let edit = {
            let row = row.downgrade();
            move |change: &dyn Fn(&mut Trigger)| {
                let Some(editor) = weak.upgrade() else {
                    return;
                };
                {
                    let mut triggers = editor.triggers.borrow_mut();
                    let Some(trigger) = triggers.get_mut(index) else {
                        return;
                    };
                    change(trigger);
                    if let Some(row) = row.upgrade() {
                        describe(&row, trigger);
                    }
                }
                editor.changed();
            }
        };
        let edit = Rc::new(edit);
        {
            let edit = edit.clone();
            pattern.connect_changed(move |entry| {
                let text = entry.text().to_string();
                flag_pattern(entry, &text);
                edit(&|t| t.pattern = text.clone());
            });
        }
        {
            let edit = edit.clone();
            let parameter = parameter.clone();
            let notice = notice.clone();
            action.connect_selected_notify(move |combo| {
                let Some(&chosen) = TriggerAction::ALL.get(combo.selected() as usize) else {
                    return;
                };
                parameter.set_title(&parameter_title(chosen));
                parameter.set_sensitive(chosen != TriggerAction::PlayBell);
                update_notice(&notice, &parameter, chosen, &parameter.text());
                edit(&|t| t.action = chosen);
            });
        }
        {
            let edit = edit.clone();
            let action = action.clone();
            let notice = notice.clone();
            parameter.connect_changed(move |entry| {
                let text = entry.text().to_string();
                if let Some(&chosen) = TriggerAction::ALL.get(action.selected() as usize) {
                    update_notice(&notice, entry, chosen, &text);
                }
                edit(&|t| t.parameter = text.clone());
            });
        }
        {
            let weak = Rc::downgrade(self);
            remove_button.connect_clicked(move |_| {
                let Some(editor) = weak.upgrade() else {
                    return;
                };
                {
                    let mut triggers = editor.triggers.borrow_mut();
                    if index < triggers.len() {
                        triggers.remove(index);
                    }
                }
                editor.changed();
                // Deferred: this button belongs to a row about to go.
                gtk4::glib::idle_add_local_once(move || editor.rebuild(None));
            });
        }
        row
    }
}

/// The row's title (the pattern) and subtitle (the action, or why the
/// pattern can't be used).
fn describe(row: &adw::ExpanderRow, trigger: &Trigger) {
    let title = if trigger.pattern.is_empty() {
        tr("New trigger").to_string()
    } else {
        trigger.pattern.clone()
    };
    // Titles are Pango markup by default.
    row.set_use_markup(false);
    row.set_title(&title);
    match triggers::pattern_error(&trigger.pattern) {
        Some(error) => {
            row.set_subtitle(&tr_args(
                "Not used — {error}",
                &[("error", &error_summary(&error))],
            ));
            row.add_css_class("error");
        }
        None => {
            row.set_subtitle(trigger.action.label());
            row.remove_css_class("error");
        }
    }
}

fn flag_pattern(entry: &adw::EntryRow, pattern: &str) {
    match triggers::pattern_error(pattern) {
        Some(error) => {
            entry.add_css_class("error");
            entry.set_tooltip_text(Some(&error));
        }
        None => {
            entry.remove_css_class("error");
            entry.set_tooltip_text(None);
        }
    }
}

/// Shows, under the parameter of an active action, what it implies and
/// anything wrong with its command; hidden for the display-only actions.
fn update_notice(
    notice: &adw::ActionRow,
    parameter: &adw::EntryRow,
    action: TriggerAction,
    text: &str,
) {
    parameter.remove_css_class("error");
    parameter.remove_css_class("warning");
    notice.set_visible(action.is_active());
    if !action.is_active() {
        return;
    }
    notice.set_title(tr(
        "Acts on your system, on any program's output — remote ones and files being displayed included",
    ));
    let safeguards = match action {
        TriggerAction::SendText => tr(
            "Captured text is typed without its control characters: only the parameter itself can contain \\n.",
        ),
        _ => tr(
            "The command runs without a shell; each $n is one argument, without its control characters. $$ is a literal $.",
        ),
    };
    let issue = triggers::parameter_issue(action, text);
    let subtitle = match &issue {
        Some(ParameterIssue::Error(error)) => {
            parameter.add_css_class("error");
            tr_args("Not usable: {error}", &[("error", error)])
        }
        Some(ParameterIssue::Warning(warning)) => {
            parameter.add_css_class("warning");
            warning.clone()
        }
        None => format!(
            "{safeguards} {}",
            tr("Disabled if it fires more than 5 times a second.")
        ),
    };
    notice.set_subtitle(&subtitle);
}

/// The parameter field's title, with what it expects — `EntryRow` has no
/// placeholder of its own.
fn parameter_title(action: TriggerAction) -> String {
    tr_args(
        "Parameter — e.g. {example}",
        &[("example", &action.parameter_hint())],
    )
}

/// `regex`'s syntax errors span several lines (pattern, caret, message):
/// its message is the last one.
fn error_summary(error: &str) -> &str {
    error
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(error)
        .trim()
        .trim_start_matches("error: ")
}
