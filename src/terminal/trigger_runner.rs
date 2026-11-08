//! Runs a terminal's triggers: feeds vte's output to the scanner of
//! `terminal::triggers` and carries out the actions that matched.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use vte4::TerminalExt;

use crate::i18n::{tr, tr_args};
use crate::layout::PaneId;
use crate::terminal::triggers::{
    self, CompiledTrigger, FiringGuard, LineScanner, Screen, StateUpdate, Trigger, TriggerAction,
    Verdict,
};

/// At most this many firings per trigger per second: a flood of matching
/// output doesn't become a flood of notifications or bells, and an active
/// trigger going over it is disabled (`FiringGuard`).
const MAX_FIRINGS_PER_SECOND: u32 = 5;

/// A `RunProcess` program is stopped after this long…
const RUN_PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
/// …and at most this much of its output is typed.
const RUN_PROCESS_MAX_OUTPUT: usize = 64 * 1024;

/// Set on a terminal while a trigger types into it (see `is_trigger_input`).
const TRIGGER_INPUT: &str = "rutile-trigger-input";

/// Whether what the terminal is sending its program right now was typed by
/// a trigger — such input isn't broadcast to synchronized panes: each runs
/// its own triggers, relaying would apply them twice.
pub fn is_trigger_input(terminal: &vte4::Terminal) -> bool {
    // SAFETY: only ever set to, and read as, a `bool` (`feed_from_trigger`).
    unsafe { terminal.data::<bool>(TRIGGER_INPUT) }.is_some_and(|flag| unsafe { *flag.as_ref() })
}

/// Types `text` into the terminal's program as trigger input. `commit` is
/// emitted synchronously from `feed_child`, so the flag covers it.
fn feed_from_trigger(terminal: &vte4::Terminal, text: &str) {
    if text.is_empty() {
        return;
    }
    // SAFETY: see `is_trigger_input`.
    unsafe { terminal.set_data(TRIGGER_INPUT, true) };
    terminal.feed_child(text.as_bytes());
    unsafe { terminal.set_data(TRIGGER_INPUT, false) };
}

/// What triggers have set on a terminal. They take precedence over what
/// the shell reports (OSC 0/2/7) — a prompt matched on an ssh server can
/// name the remote host, which OSC 7 doesn't do there — and stay until a
/// trigger changes them again.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    /// Replaces the pane title template (`title::DEFAULT_TEMPLATE`).
    pub title_template: Option<String>,
    /// Replaces the profile's badge text template.
    pub badge_template: Option<String>,
    pub user: Option<String>,
    pub host: Option<String>,
    pub directory: Option<String>,
}

/// `vte4::Terminal` as the scanner's `Screen`.
struct VteScreen<'a>(&'a vte4::Terminal);

impl Screen for VteScreen<'_> {
    fn cursor_row(&self) -> i64 {
        self.0.cursor_position().1
    }

    fn row_text(&self, row: i64) -> String {
        self.0
            .text_range_format(vte4::Format::Text, row, 0, row + 1, 0)
            .0
            .map(|text| text.to_string())
            .unwrap_or_default()
    }
}

pub struct TriggerRunner {
    pane_id: PaneId,
    scanner: RefCell<LineScanner>,
    triggers: RefCell<Vec<CompiledTrigger>>,
    max_lines: Cell<Option<usize>>,
    guard: RefCell<FiringGuard>,
    /// Above the terminal: a trigger was disabled, or its command failed.
    banner: adw::Banner,
    overrides: Rc<RefCell<Overrides>>,
    /// Called after triggers changed `overrides` (title, badge, state).
    listeners: RefCell<Vec<Rc<dyn Fn()>>>,
}

impl TriggerRunner {
    pub fn new(terminal: &vte4::Terminal, pane_id: PaneId) -> Rc<Self> {
        let runner = Rc::new(Self {
            pane_id,
            scanner: RefCell::default(),
            triggers: RefCell::default(),
            max_lines: Cell::new(Some(256)),
            guard: RefCell::new(FiringGuard::new(
                MAX_FIRINGS_PER_SECOND,
                Duration::from_secs(1),
            )),
            banner: adw::Banner::new(""),
            overrides: Rc::default(),
            listeners: RefCell::default(),
        });
        {
            // Hides the banner; after a trigger was disabled, re-enables it.
            let weak = Rc::downgrade(&runner);
            runner.banner.connect_button_clicked(move |banner| {
                banner.set_revealed(false);
                if let Some(runner) = weak.upgrade() {
                    runner.guard.borrow_mut().reset();
                }
            });
        }
        let weak = Rc::downgrade(&runner);
        // After vte's own handlers, like Tilix: the screen is up to date.
        terminal.connect_contents_changed(move |terminal| {
            if let Some(runner) = weak.upgrade() {
                runner.scan(terminal);
            }
        });
        runner
    }

    /// Replaces the triggers (a new profile was applied, or the triggers
    /// were edited). What was already scanned isn't scanned again.
    /// Disabled triggers are enabled again.
    pub fn set_triggers(&self, triggers: &[Trigger], max_lines: Option<usize>) {
        self.triggers.replace(triggers::compile(triggers));
        self.max_lines.set(max_lines);
        self.guard.borrow_mut().reset();
        self.banner.set_revealed(false);
    }

    /// The banner reporting disabled or failing triggers — placed above the
    /// terminal by `PaneView`.
    pub fn banner(&self) -> &adw::Banner {
        &self.banner
    }

    fn show_banner(&self, message: &str, button: &str) {
        self.banner.set_title(message);
        self.banner.set_button_label(Some(button));
        self.banner.set_revealed(true);
    }

    pub fn overrides(&self) -> Rc<RefCell<Overrides>> {
        self.overrides.clone()
    }

    pub fn connect_overrides_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Rc::new(listener));
    }

    fn scan(&self, terminal: &vte4::Terminal) {
        // `try_borrow`: an action below can re-apply the profile (automatic
        // switching), which replaces the triggers — never mid-scan, as the
        // matches are copied out first.
        let Ok(triggers) = self.triggers.try_borrow() else {
            return;
        };
        if triggers.is_empty() {
            return;
        }
        let Ok(mut scanner) = self.scanner.try_borrow_mut() else {
            return;
        };
        let found = scanner.scan(&VteScreen(terminal), &triggers, self.max_lines.get());
        let now = Instant::now();
        let mut guard = self.guard.borrow_mut();
        let mut to_run = Vec::new();
        let mut disabled = Vec::new();
        for m in found {
            let trigger = &triggers[m.trigger];
            match guard.check(m.trigger, trigger.action.is_active(), now) {
                Verdict::Run => to_run.push(Firing {
                    index: m.trigger,
                    action: trigger.action,
                    template: trigger.parameter.clone(),
                    groups: m.groups,
                }),
                Verdict::Drop => {}
                Verdict::Disable => disabled.push(trigger.pattern.clone()),
            }
        }
        drop((triggers, scanner, guard));

        let mut changed = false;
        for firing in to_run {
            changed |= self.run(terminal, firing);
        }
        if let Some(pattern) = disabled.first() {
            self.show_banner(
                &tr_args(
                    "Trigger \u{201c}{pattern}\u{201d} disabled: it fired more than {count} times a second",
                    &[("pattern", pattern), ("count", &MAX_FIRINGS_PER_SECOND)],
                ),
                tr("Re-enable"),
            );
        }
        if changed {
            let listeners: Vec<_> = self.listeners.borrow().clone();
            for listener in listeners {
                listener();
            }
        }
    }

    /// Carries out one action. Returns whether `overrides` changed.
    fn run(&self, terminal: &vte4::Terminal, firing: Firing) -> bool {
        let Firing {
            index,
            action,
            template,
            groups,
        } = firing;
        // The display-only actions take the template with `$n` replaced
        // as is; the active ones build their text or command themselves,
        // from sanitized groups.
        let parameter = || triggers::substitute(&template, &groups);
        match action {
            TriggerAction::UpdateState => {
                let update = StateUpdate::parse(&parameter());
                let mut guard = self.overrides.borrow_mut();
                let overrides = &mut *guard;
                let mut changed = false;
                for (slot, value) in [
                    (&mut overrides.user, update.username),
                    (&mut overrides.host, update.hostname),
                    (&mut overrides.directory, update.directory),
                ] {
                    if value.is_some() && *slot != value {
                        *slot = value;
                        changed = true;
                    }
                }
                changed
            }
            TriggerAction::UpdateTitle => {
                set_if_changed(&mut self.overrides.borrow_mut().title_template, parameter())
            }
            TriggerAction::UpdateBadge => {
                set_if_changed(&mut self.overrides.borrow_mut().badge_template, parameter())
            }
            TriggerAction::PlayBell => {
                terminal.error_bell();
                false
            }
            TriggerAction::SendNotification => {
                self.notify(terminal, index, &parameter());
                false
            }
            TriggerAction::SendText => {
                feed_from_trigger(terminal, &triggers::send_text(&template, &groups));
                false
            }
            TriggerAction::ExecuteCommand => {
                if let Err(error) = self.spawn(terminal, &template, &groups, false) {
                    self.show_banner(&error, tr("Dismiss"));
                }
                false
            }
            TriggerAction::RunProcess => {
                if let Err(error) = self.spawn(terminal, &template, &groups, true) {
                    self.show_banner(&error, tr("Dismiss"));
                }
                false
            }
        }
    }

    /// Starts the command of an `ExecuteCommand` (`capture` unset: in the
    /// background, left alone) or `RunProcess` trigger (`capture` set: its
    /// output is typed into the terminal once it exits, or it's stopped
    /// after `RUN_PROCESS_TIMEOUT`). The argv runs as is — no shell — in
    /// the terminal's current directory when known.
    fn spawn(
        &self,
        terminal: &vte4::Terminal,
        template: &str,
        groups: &[String],
        capture: bool,
    ) -> Result<(), String> {
        let argv = triggers::command_argv(template, groups).map_err(|error| {
            tr_args(
                "Trigger command can't be parsed: {error}",
                &[("error", &error)],
            )
        })?;
        let flags = if capture {
            gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE
        } else {
            gio::SubprocessFlags::NONE
        };
        let launcher = gio::SubprocessLauncher::new(flags);
        if let Some(dir) = terminal
            .current_directory_uri()
            .and_then(|uri| crate::terminal::title::directory_from_uri(&uri))
            .filter(|dir| std::path::Path::new(dir).is_dir())
        {
            launcher.set_cwd(dir);
        }
        let args: Vec<&std::ffi::OsStr> = argv.iter().map(std::ffi::OsStr::new).collect();
        let process = launcher.spawn(&args).map_err(|err| {
            tr_args(
                "Trigger command failed to start: {error}",
                &[("error", &err.message())],
            )
        })?;
        if !capture {
            return Ok(());
        }

        // Whichever comes first, the timeout or the exit, takes the id:
        // removing a timeout that already fired would panic.
        let timeout: Rc<Cell<Option<glib::SourceId>>> = Rc::default();
        {
            let process = process.clone();
            let slot = timeout.clone();
            timeout.set(Some(glib::timeout_add_local_once(
                RUN_PROCESS_TIMEOUT,
                move || {
                    slot.take();
                    process.force_exit();
                },
            )));
        }
        let terminal = terminal.downgrade();
        process.communicate_utf8_async(None, gio::Cancellable::NONE, move |result| {
            if let Some(id) = timeout.take() {
                id.remove();
            }
            let Some(terminal) = terminal.upgrade() else {
                return;
            };
            if let Ok((Some(output), _)) = result {
                let text = triggers::sanitize_process_output(&output, RUN_PROCESS_MAX_OUTPUT);
                feed_from_trigger(&terminal, &text);
            }
        });
        Ok(())
    }

    /// Like Tilix, only while the terminal isn't what the user is looking
    /// at. One notification per pane and trigger: a new one replaces the
    /// previous.
    fn notify(&self, terminal: &vte4::Terminal, index: usize, parameter: &str) {
        if crate::terminal::monitor::pane_is_active(terminal) {
            return;
        }
        let Some(app) = terminal
            .root()
            .and_downcast::<gtk4::Window>()
            .and_then(|window| window.application())
        else {
            return;
        };
        let (title, body) = triggers::notification(parameter);
        let title = title.unwrap_or_else(|| {
            terminal
                .window_title()
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "Rutile".to_string())
        });
        let notification = gio::Notification::new(&title);
        if !body.is_empty() {
            notification.set_body(Some(&body));
        }
        let id = format!("trigger-{}-{index}", self.pane_id);
        app.send_notification(Some(&id), &notification);
    }
}

/// A trigger that matched and may run, copied out of the scan.
struct Firing {
    index: usize,
    action: TriggerAction,
    template: String,
    groups: Vec<String>,
}

/// A non-empty template replaces the current one; an empty one (a trigger
/// with no parameter) goes back to the default.
fn set_if_changed(slot: &mut Option<String>, template: String) -> bool {
    let value = (!template.is_empty()).then_some(template);
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}
