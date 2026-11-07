//! Runs a terminal's triggers: feeds vte's output to the scanner of
//! `terminal::triggers` and carries out the actions that matched.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::gio;
use gtk4::prelude::*;
use vte4::TerminalExt;

use crate::layout::PaneId;
use crate::terminal::triggers::{
    self, CompiledTrigger, LineScanner, RateLimiter, Screen, StateUpdate, Trigger, TriggerAction,
};

/// At most this many firings per trigger per second: a flood of matching
/// output doesn't become a flood of notifications or bells.
const MAX_FIRINGS_PER_SECOND: u32 = 5;

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
    limiter: RefCell<RateLimiter>,
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
            limiter: RefCell::new(RateLimiter::new(
                MAX_FIRINGS_PER_SECOND,
                Duration::from_secs(1),
            )),
            overrides: Rc::default(),
            listeners: RefCell::default(),
        });
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
    pub fn set_triggers(&self, triggers: &[Trigger], max_lines: Option<usize>) {
        self.triggers.replace(triggers::compile(triggers));
        self.max_lines.set(max_lines);
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
        let mut limiter = self.limiter.borrow_mut();
        let to_run: Vec<(usize, TriggerAction, String)> = found
            .into_iter()
            .filter(|m| limiter.allow(m.trigger, now))
            .map(|m| {
                let trigger = &triggers[m.trigger];
                let parameter = triggers::substitute(&trigger.parameter, &m.groups);
                (m.trigger, trigger.action, parameter)
            })
            .collect();
        drop((triggers, scanner, limiter));

        let mut changed = false;
        for (index, action, parameter) in to_run {
            changed |= self.run(terminal, index, action, parameter);
        }
        if changed {
            let listeners: Vec<_> = self.listeners.borrow().clone();
            for listener in listeners {
                listener();
            }
        }
    }

    /// Carries out one action. Returns whether `overrides` changed.
    fn run(
        &self,
        terminal: &vte4::Terminal,
        index: usize,
        action: TriggerAction,
        parameter: String,
    ) -> bool {
        match action {
            TriggerAction::UpdateState => {
                let update = StateUpdate::parse(&parameter);
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
                set_if_changed(&mut self.overrides.borrow_mut().title_template, parameter)
            }
            TriggerAction::UpdateBadge => {
                set_if_changed(&mut self.overrides.borrow_mut().badge_template, parameter)
            }
            TriggerAction::PlayBell => {
                terminal.error_bell();
                false
            }
            TriggerAction::SendNotification => {
                self.notify(terminal, index, &parameter);
                false
            }
        }
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
