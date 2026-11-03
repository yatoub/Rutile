use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use vte4::TerminalExt;
use vte4::TerminalExtManual;

use crate::profile::spawn::{self, SpawnCommand};
use crate::profile::{
    CursorBlink, CursorShape, ExitAction, TerminalConfig, TerminalOptions, TextBlink,
};

/// A command that exits sooner than this after being (re)spawned is held
/// instead of restarted under `ExitAction::Restart` — otherwise a typo'd
/// custom command would respawn in a tight loop forever.
const MIN_UPTIME_FOR_RESTART: Duration = Duration::from_secs(1);

/// How long the bell icon stays in the pane header after the last bell —
/// same 5 seconds as Tilix.
const BELL_ICON_DURATION: Duration = Duration::from_secs(5);

pub struct TerminalWidget {
    pub terminal: vte4::Terminal,
    /// What `PaneView` actually places in the pane: the terminal inside a
    /// `ScrolledWindow`, which is what draws its scrollbar.
    root: gtk4::ScrolledWindow,
    /// Shown in the pane header for a few seconds when the terminal rings
    /// its bell, if the profile's `BellMode` asks for an icon.
    bell_icon: gtk4::Image,
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
        apply_options(&terminal, &config.profile.options);

        let root = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            // `External` rather than `Never` when hidden: the terminal
            // still scrolls through its scrollback (mouse wheel,
            // Shift+PageUp), only the scrollbar itself goes away. `Never`
            // would make the window size itself to the whole scrollback.
            .vscrollbar_policy(if config.profile.options.show_scrollbar {
                gtk4::PolicyType::Automatic
            } else {
                gtk4::PolicyType::External
            })
            .hexpand(true)
            .vexpand(true)
            .child(&terminal)
            .build();

        let bell_icon = gtk4::Image::builder()
            .icon_name("alarm-symbolic")
            .tooltip_text("Bell")
            .visible(false)
            .build();
        if config.profile.options.bell.shows_icon() {
            connect_bell_icon(&terminal, &bell_icon);
        }

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
            root,
            bell_icon,
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

    /// The widget to place in the pane (the terminal plus its scrollbar).
    pub fn root(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    /// The bell icon, placed in the pane header by `PaneView`.
    pub fn bell_icon(&self) -> &gtk4::Image {
        &self.bell_icon
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

/// Applies a profile's font, cursor, scrolling and bell settings to a
/// freshly created terminal.
fn apply_options(terminal: &vte4::Terminal, options: &TerminalOptions) {
    let font = if options.use_system_font {
        system_monospace_font()
    } else {
        Some(options.font.clone())
    };
    // `None` (no GNOME setting available) leaves vte's own default font.
    if let Some(font) = font {
        terminal.set_font(Some(&gtk4::pango::FontDescription::from_string(&font)));
    }
    // vte clamps these to 1.0..=2.0 itself, but a hand-edited TOML value
    // outside that range is clamped here too so what's applied matches
    // what Preferences shows.
    terminal.set_cell_width_scale(options.cell_width_scale.clamp(1.0, 2.0));
    terminal.set_cell_height_scale(options.cell_height_scale.clamp(1.0, 2.0));

    terminal.set_text_blink_mode(match options.text_blink {
        TextBlink::Never => vte4::TextBlinkMode::Never,
        TextBlink::Focused => vte4::TextBlinkMode::Focused,
        TextBlink::Unfocused => vte4::TextBlinkMode::Unfocused,
        TextBlink::Always => vte4::TextBlinkMode::Always,
    });
    terminal.set_cursor_shape(match options.cursor_shape {
        CursorShape::Block => vte4::CursorShape::Block,
        CursorShape::Ibeam => vte4::CursorShape::Ibeam,
        CursorShape::Underline => vte4::CursorShape::Underline,
    });
    terminal.set_cursor_blink_mode(match options.cursor_blink {
        CursorBlink::System => vte4::CursorBlinkMode::System,
        CursorBlink::On => vte4::CursorBlinkMode::On,
        CursorBlink::Off => vte4::CursorBlinkMode::Off,
    });
    terminal.set_word_char_exceptions(&options.word_chars);

    terminal.set_scroll_on_output(options.scroll_on_output);
    terminal.set_scroll_on_keystroke(options.scroll_on_keystroke);
    // vte treats a negative line count as "unlimited".
    terminal.set_scrollback_lines(if options.scrollback_unlimited {
        -1
    } else {
        options.scrollback_lines.into()
    });

    terminal.set_audible_bell(options.bell.plays_sound());
}

/// GNOME's monospace font setting, if its GSettings schema is installed —
/// checked first because `gio::Settings::new` aborts the whole process on
/// an unknown schema instead of returning an error.
fn system_monospace_font() -> Option<String> {
    const SCHEMA: &str = "org.gnome.desktop.interface";
    gtk4::gio::SettingsSchemaSource::default()?.lookup(SCHEMA, true)?;
    let font = gtk4::gio::Settings::new(SCHEMA).string("monospace-font-name");
    (!font.is_empty()).then(|| font.to_string())
}

/// Shows `icon` on every bell and hides it once no bell has rung for
/// `BELL_ICON_DURATION` — a new bell while it's shown restarts the timer.
fn connect_bell_icon(terminal: &vte4::Terminal, icon: &gtk4::Image) {
    let pending: Rc<Cell<Option<glib::SourceId>>> = Rc::default();
    let icon = icon.downgrade();
    terminal.connect_bell(move |_| {
        let Some(icon) = icon.upgrade() else {
            return;
        };
        icon.set_visible(true);
        if let Some(previous) = pending.take() {
            previous.remove();
        }
        let pending_for_timeout = pending.clone();
        let icon = icon.downgrade();
        let id = glib::timeout_add_local_once(BELL_ICON_DURATION, move || {
            pending_for_timeout.set(None);
            if let Some(icon) = icon.upgrade() {
                icon.set_visible(false);
            }
        });
        pending.set(Some(id));
    });
}
