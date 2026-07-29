use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gtk4::gio;
use gtk4::gio::prelude::ApplicationExt;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use vte4::TerminalExt;

/// How often `/proc/<pid>/stat` is polled for CPU-tick deltas — cheap
/// enough at this cadence to not matter, frequent enough that a command
/// finishing is still noticed promptly.
const POLL_INTERVAL_SECS: u32 = 2;

/// Tilix-style background notifications for one pane (`Action`-less: these
/// fire on their own, not from a keybinding), combining the two techniques
/// `docs/ROADMAP.md` calls for under one gate (`enabled`, `pid`,
/// `silence_seconds` are all `Fn`s so the caller can back them with live
/// preferences/profile lookups rather than a value frozen at attach time):
///
/// - **Bell**: `vte4::Terminal`'s "bell" signal (BEL / OSC output) fires a
///   desktop notification immediately.
/// - **"Silence" after activity**: this VTE version exposes no shell-
///   integration "command finished" signal (that needs OSC 133, which vte4
///   0.10 doesn't surface), so completion is approximated by polling
///   `/proc/<pid>/stat`'s cumulative CPU-tick fields (14+15, see proc(5))
///   to detect the child process going from "consuming CPU" (a command is
///   running) to quiet. Once it's stayed quiet — no new output via
///   `connect_contents_changed` — for the profile's `silence_seconds`, a
///   "command finished" notification fires once per busy period.
///
/// Both triggers are skipped while the pane's terminal has keyboard focus
/// in the active window — no point notifying about what's already on
/// screen. The polling timer holds only a weak ref to `terminal`, so it
/// stops itself (`ControlFlow::Break`) once the pane is closed instead of
/// keeping it alive forever.
pub fn attach<Pid, Enabled, Silence>(
    app: adw::Application,
    terminal: &vte4::Terminal,
    pid: Pid,
    enabled: Enabled,
    silence_seconds: Silence,
) where
    Pid: Fn() -> Option<i32> + Clone + 'static,
    Enabled: Fn() -> bool + Clone + 'static,
    Silence: Fn() -> u32 + Clone + 'static,
{
    {
        let app = app.clone();
        let enabled = enabled.clone();
        terminal.connect_bell(move |terminal| {
            if !enabled() || pane_is_active(terminal) {
                return;
            }
            notify(&app, terminal, "Bell");
        });
    }

    let last_output = Rc::new(Cell::new(Instant::now()));
    {
        let last_output = last_output.clone();
        terminal.connect_contents_changed(move |_| {
            last_output.set(Instant::now());
        });
    }

    let last_ticks: Rc<Cell<Option<u64>>> = Rc::new(Cell::new(None));
    let was_busy = Rc::new(Cell::new(false));
    let terminal_weak = terminal.downgrade();
    glib::timeout_add_seconds_local(POLL_INTERVAL_SECS, move || {
        let Some(terminal) = terminal_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if !enabled() {
            return glib::ControlFlow::Continue;
        }
        let Some(child_pid) = pid() else {
            return glib::ControlFlow::Continue;
        };
        let Some(ticks_now) = read_cpu_ticks(child_pid) else {
            return glib::ControlFlow::Continue;
        };

        let became_active = last_ticks
            .replace(Some(ticks_now))
            .is_some_and(|prev| ticks_now > prev);
        if became_active {
            was_busy.set(true);
            return glib::ControlFlow::Continue;
        }

        if was_busy.get()
            && last_output.get().elapsed().as_secs() >= u64::from(silence_seconds())
            && !pane_is_active(&terminal)
        {
            was_busy.set(false);
            notify(&app, &terminal, "Command finished");
        }

        glib::ControlFlow::Continue
    });
}

fn pane_is_active(terminal: &vte4::Terminal) -> bool {
    if !terminal.has_focus() {
        return false;
    }
    terminal
        .root()
        .and_then(|root| root.downcast::<gtk4::Window>().ok())
        .is_some_and(|window| window.is_active())
}

fn notify(app: &adw::Application, terminal: &vte4::Terminal, body: &str) {
    let title = terminal
        .window_title()
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Rutile".to_string());
    let notification = gio::Notification::new(&title);
    notification.set_body(Some(body));
    app.send_notification(None, &notification);
}

/// Sums `/proc/<pid>/stat`'s `utime`+`stime` fields (cumulative scheduler
/// ticks used by the process). `comm` (the 2nd field) can itself contain
/// spaces/parentheses, so this anchors on the *last* `)` to skip past it
/// reliably rather than naively splitting on whitespace from the start.
fn read_cpu_ticks(pid: i32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_cpu_ticks(&stat)
}

fn parse_cpu_ticks(stat: &str) -> Option<u64> {
    let after_comm = stat.rsplit_once(')')?.1;
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    // Fields here start at `state` (proc(5) field 3); utime/stime are
    // fields 14/15 overall, i.e. indices 11/12 in this zero-indexed slice.
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

#[cfg(test)]
mod tests {
    use super::parse_cpu_ticks;

    #[test]
    fn parses_utime_stime_from_a_realistic_proc_stat_line() {
        // Real-world example (truncated after the fields this cares about),
        // including a `comm` field that itself contains a space and parens
        // to exercise the "anchor on the last `)`" parsing.
        let stat =
            "1234 (some (weird) comm) S 1 1234 1234 0 -1 4194560 100 0 0 0 42 17 0 0 20 0 1 0";
        assert_eq!(parse_cpu_ticks(stat), Some(42 + 17));
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse_cpu_ticks("not a stat line"), None);
        assert_eq!(parse_cpu_ticks("1234 (sh) S 1 1234"), None);
    }
}
