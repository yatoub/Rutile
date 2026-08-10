use std::os::fd::AsRawFd;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use vte4::TerminalExt;

/// True if `foreground_pgid` names a real, distinct process group from the
/// shell's own — i.e. a child (editor, long-running command...) currently
/// owns the terminal foreground, not just the shell idling at its prompt.
/// An interactive shell is its own process group leader, so `pgid == pid`
/// is the "nothing running" case; `tcgetpgrp` returning `<= 0` means the
/// query itself failed (pty gone, no controlling terminal), also "nothing
/// to warn about".
fn foreground_pgid_differs(foreground_pgid: i32, shell_pid: i32) -> bool {
    foreground_pgid > 0 && foreground_pgid != shell_pid
}

/// Whether `terminal`'s shell currently has a foreground child process
/// (e.g. `vim`, `sleep 100`) rather than idling at its own prompt.
/// Mirrors Tilix's own "prompt before closing a running process" check,
/// simplified to a pgid comparison since we don't need Tilix's full
/// process-tree description, just a yes/no.
pub fn pane_has_foreground_process(terminal: &vte4::Terminal, shell_pid: Option<i32>) -> bool {
    let Some(shell_pid) = shell_pid else {
        // spawn_async hasn't completed yet (or failed) — nothing to warn
        // about, see `TerminalWidget::child_pid`'s doc comment.
        return false;
    };
    let Some(pty) = terminal.pty() else {
        return false;
    };
    // SAFETY: `pty.fd()` borrows a valid fd owned by `pty` for the
    // duration of this call; `tcgetpgrp` only reads the fd's associated
    // session state and returns a pid_t (or -1 on error), no memory is
    // touched through the raw fd.
    let foreground_pgid = unsafe { libc::tcgetpgrp(pty.fd().as_raw_fd()) };
    foreground_pgid_differs(foreground_pgid, shell_pid)
}

/// Invokes `on_confirmed` immediately if `has_process` is false. Otherwise
/// shows a confirmation dialog and only invokes it if the user confirms;
/// does nothing on cancel.
pub fn confirm_close(
    parent: &impl IsA<gtk4::Widget>,
    has_process: bool,
    on_confirmed: impl Fn() + 'static,
) {
    if !has_process {
        on_confirmed();
        return;
    }

    let dialog = adw::AlertDialog::new(
        Some("Close this?"),
        Some("There is still a process running. Closing will terminate it."),
    );
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("close", "Close");
    dialog.set_response_appearance("close", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    dialog.connect_response(None, move |_, response| {
        if response == "close" {
            on_confirmed();
        }
    });

    dialog.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_foreground_child_when_pgid_matches_shell() {
        assert!(!foreground_pgid_differs(1234, 1234));
    }

    #[test]
    fn foreground_child_when_pgid_differs_from_shell() {
        assert!(foreground_pgid_differs(5678, 1234));
    }

    #[test]
    fn no_foreground_child_when_tcgetpgrp_failed() {
        assert!(!foreground_pgid_differs(-1, 1234));
        assert!(!foreground_pgid_differs(0, 1234));
    }
}
