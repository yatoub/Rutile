use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use vte4::TerminalExt;
use vte4::TerminalExtManual;

use crate::profile::spawn::{self, SpawnCommand};
use crate::profile::{ExitAction, TerminalConfig};

/// A command that exits sooner than this after being (re)spawned is held
/// instead of restarted under `ExitAction::Restart` — otherwise a typo'd
/// custom command would respawn in a tight loop forever.
const MIN_UPTIME_FOR_RESTART: Duration = Duration::from_secs(1);

pub struct TerminalWidget {
    pub terminal: vte4::Terminal,
    spawner: Rc<Spawner>,
    exit_action: ExitAction,
}

/// Everything needed to (re)start the terminal's command — shared with the
/// banner's "Relaunch" button, which outlives any single spawn.
struct Spawner {
    terminal: vte4::Terminal,
    /// Shown above the terminal when its command isn't running anymore
    /// (`ExitAction::Hold`) or never started (spawn failure, unparsable
    /// custom command) — Tilix's relaunch info bar.
    banner: adw::Banner,
    command: Result<SpawnCommand, String>,
    cwd: Option<String>,
    /// The command's pid, captured from `spawn_async`'s own callback (vte
    /// doesn't expose a `child_pid()` getter) — `None` until the spawn
    /// actually completes, while the pane is held after an exit, and
    /// forever `None` if it failed. Used by `terminal::monitor` to poll
    /// `/proc/<pid>/stat` for its "silence after activity" notification.
    pid: Cell<Option<i32>>,
    spawned_at: Cell<Instant>,
}

impl Spawner {
    fn spawn(self: &Rc<Self>) {
        self.banner.set_revealed(false);
        self.spawned_at.set(Instant::now());

        let command = match &self.command {
            Ok(command) => command,
            Err(message) => {
                self.hold(&format!("Invalid custom command: {message}"));
                return;
            }
        };
        let argv: Vec<&str> = command.argv.iter().map(String::as_str).collect();
        let mut flags = glib::SpawnFlags::DEFAULT;
        if command.search_path {
            flags |= glib::SpawnFlags::SEARCH_PATH;
        }
        if command.argv_zero {
            flags |= glib::SpawnFlags::FILE_AND_ARGV_ZERO;
        }

        let program = command.argv[0].clone();
        let this = Rc::downgrade(self);
        self.terminal.spawn_async(
            vte4::PtyFlags::DEFAULT,
            self.cwd.as_deref(),
            &argv,
            &[],
            flags,
            || {},
            -1,
            gtk4::gio::Cancellable::NONE,
            move |result| {
                let Some(this) = this.upgrade() else {
                    return;
                };
                match result {
                    Ok(child_pid) => this.pid.set(Some(child_pid.0)),
                    Err(err) => this.hold(&format!("Failed to start {program}: {err}")),
                }
            },
        );
    }

    fn hold(&self, message: &str) {
        self.pid.set(None);
        self.banner.set_title(message);
        self.banner.set_revealed(true);
    }
}

impl TerminalWidget {
    /// `cwd`: the command's starting directory when restoring a saved
    /// session (`session::persist`) — `None` falls back to the profile's
    /// `working_directory`, then to vte's own default (inherited from this
    /// process). See `profile::spawn::working_directory_for`.
    pub fn new(config: &TerminalConfig, cwd: Option<&str>) -> Self {
        let terminal = vte4::Terminal::new();
        // Without this, a terminal nested inside a Paned doesn't claim its
        // share of space, so deeper splits can collapse a pane down to
        // (near) zero size instead of an even split.
        terminal.set_vexpand(true);
        terminal.set_hexpand(true);

        let scheme = &config.scheme;
        let foreground = scheme.foreground_rgba();
        let background = scheme.background_rgba();
        let palette = scheme.palette_rgba();
        let palette_refs: Vec<&gtk4::gdk::RGBA> = palette.iter().collect();
        terminal.set_colors(Some(&foreground), Some(&background), &palette_refs);

        let banner = adw::Banner::builder().button_label("Relaunch").build();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let spawner = Rc::new(Spawner {
            terminal: terminal.clone(),
            banner: banner.clone(),
            command: spawn::command_for(&config.profile, &shell),
            cwd: spawn::working_directory_for(&config.profile, cwd, &glib::home_dir()),
            pid: Cell::new(None),
            spawned_at: Cell::new(Instant::now()),
        });
        {
            let spawner = Rc::downgrade(&spawner);
            banner.connect_button_clicked(move |_| {
                if let Some(spawner) = spawner.upgrade() {
                    spawner.spawn();
                }
            });
        }
        spawner.spawn();

        Self {
            terminal,
            spawner,
            exit_action: config.profile.exit_action,
        }
    }

    /// The command's pid, once `spawn_async` has completed and while it's
    /// still running. See `Spawner::pid`'s doc comment.
    pub fn child_pid(&self) -> Option<i32> {
        self.spawner.pid.get()
    }

    pub fn widget(&self) -> &vte4::Terminal {
        &self.terminal
    }

    /// The relaunch banner — placed above the terminal by `PaneView`.
    pub fn banner(&self) -> &adw::Banner {
        &self.spawner.banner
    }

    /// Applies the profile's exit action once the command has exited
    /// (`status` is vte's raw `waitpid` status). Returns `true` when the
    /// pane should be closed — the caller owns the split tree, this
    /// widget doesn't.
    pub fn handle_child_exited(&self, status: i32) -> bool {
        match self.exit_action {
            ExitAction::Close => true,
            ExitAction::Restart
                if self.spawner.spawned_at.get().elapsed() >= MIN_UPTIME_FOR_RESTART =>
            {
                self.spawner.spawn();
                false
            }
            ExitAction::Restart => {
                self.spawner.hold(&format!(
                    "{} — too quickly to restart",
                    spawn::describe_exit(status)
                ));
                false
            }
            ExitAction::Hold => {
                self.spawner.hold(&spawn::describe_exit(status));
                false
            }
        }
    }

    pub fn feed(&self, bytes: &[u8]) {
        self.terminal.feed_child(bytes);
    }

    /// The shell's current working directory, if vte has picked it up via
    /// OSC 7 (most modern shell configs emit it automatically). `None` if
    /// the shell never reported one — the caller falls back to no explicit
    /// cwd on restore, same as a brand new terminal.
    pub fn current_directory(&self) -> Option<String> {
        let uri = self.terminal.current_directory_uri()?;
        gtk4::gio::File::for_uri(&uri)
            .path()
            .map(|path| path.to_string_lossy().into_owned())
    }
}
