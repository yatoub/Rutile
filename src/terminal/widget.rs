use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use vte4::TerminalExt;
use vte4::TerminalExtManual;

use crate::layout::PaneId;
use crate::profile::scheme::parse_or_fallback;
use crate::profile::spawn::{self, SpawnCommand};
use crate::profile::{
    BellMode, CursorBlink, CursorShape, ExitAction, ProfileColors, ProfileId, ProfileResolver,
    TerminalConfig, TerminalOptions, TerminalState, TextBlink,
};
use crate::terminal::badge::BadgeOverlay;
use crate::terminal::title;

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
    /// `ScrolledWindow` (which draws its scrollbar), with the badge
    /// overlaid on top.
    root: gtk4::Overlay,
    /// Shown in the pane header for a few seconds when the terminal rings
    /// its bell, if the profile's `BellMode` asks for an icon.
    bell_icon: gtk4::Image,
    spawner: Rc<Spawner>,
    appearance: Rc<Appearance>,
}

/// Everything about the terminal that comes from its profile and can
/// change while it runs — re-applied as a whole when automatic profile
/// switching picks another profile. What the command *is* (`Spawner`)
/// isn't part of it: like Tilix, switching profiles doesn't restart it.
struct Appearance {
    terminal: vte4::Terminal,
    scrolled: gtk4::ScrolledWindow,
    badge: BadgeOverlay,
    /// Shared with the bell handler (see `connect_bell_icon`).
    bell: Rc<Cell<BellMode>>,
    exit_action: Cell<ExitAction>,
    /// The profile currently applied: the session's own, or one picked by
    /// automatic switching.
    profile_id: RefCell<ProfileId>,
}

impl Appearance {
    fn apply(&self, config: &TerminalConfig) {
        let scheme = &config.scheme;
        let foreground = scheme.foreground_rgba();
        let background = scheme.background_rgba();
        let palette = scheme.palette_rgba();
        let palette_refs: Vec<&gtk4::gdk::RGBA> = palette.iter().collect();
        self.terminal
            .set_colors(Some(&foreground), Some(&background), &palette_refs);
        apply_options(&self.terminal, &config.profile.options);
        apply_colors(&self.terminal, &config.profile.colors);

        // `External` rather than `Never` when hidden: the terminal still
        // scrolls through its scrollback (mouse wheel, Shift+PageUp), only
        // the scrollbar itself goes away. `Never` would make the window
        // size itself to the whole scrollback.
        self.scrolled
            .set_vscrollbar_policy(if config.profile.options.show_scrollbar {
                gtk4::PolicyType::Automatic
            } else {
                gtk4::PolicyType::External
            });

        // After `apply_options`: a "system font" badge is sized from the
        // terminal's font.
        let badge_color = config
            .profile
            .colors
            .badge_color
            .as_deref()
            .map(parse_or_fallback)
            .unwrap_or(foreground);
        self.badge
            .update(&self.terminal, &config.profile.badge, &badge_color);

        self.bell.set(config.profile.options.bell);
        self.exit_action.set(config.profile.exit_action);
        self.profile_id.replace(config.profile.id.clone());
    }
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
    /// `pane_id` is only used for the badge's `${id}`. `cwd`: the command's
    /// starting directory when restoring a saved
    /// session (`session::persist`) — `None` falls back to the profile's
    /// `working_directory`, then to vte's own default (inherited from this
    /// process). See `profile::spawn::working_directory_for`.
    pub fn new(config: &TerminalConfig, pane_id: PaneId, cwd: Option<&str>) -> Self {
        let terminal = vte4::Terminal::new();
        // Without this, a terminal nested inside a Paned doesn't claim its
        // share of space, so deeper splits can collapse a pane down to
        // (near) zero size instead of an even split.
        terminal.set_vexpand(true);
        terminal.set_hexpand(true);

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .child(&terminal)
            .build();
        let badge = BadgeOverlay::new(&terminal, pane_id);
        let root = gtk4::Overlay::builder().child(&scrolled).build();
        root.add_overlay(badge.widget());

        let bell_icon = gtk4::Image::builder()
            .icon_name("alarm-symbolic")
            .tooltip_text("Bell")
            .visible(false)
            .build();
        let bell = Rc::new(Cell::new(config.profile.options.bell));
        connect_bell_icon(&terminal, &bell_icon, bell.clone());

        let appearance = Rc::new(Appearance {
            terminal: terminal.clone(),
            scrolled,
            badge,
            bell,
            exit_action: Cell::new(config.profile.exit_action),
            profile_id: RefCell::new(config.profile.id.clone()),
        });
        appearance.apply(config);
        if let Some(resolver) = config.resolver.clone() {
            connect_automatic_switch(&terminal, &appearance, resolver, config.profile.id.clone());
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
            appearance,
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

    /// The widget to place in the pane (the terminal, its scrollbar and
    /// its badge).
    pub fn root(&self) -> &gtk4::Overlay {
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
        match self.appearance.exit_action.get() {
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
    // `None` (no GNOME setting available) is vte's own default font.
    let font = font.map(|font| gtk4::pango::FontDescription::from_string(&font));
    terminal.set_font(font.as_ref());
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

/// Applies a profile's color overrides on top of the scheme `set_colors`
/// already applied. Unset colors are passed as `None`, which keeps vte's
/// default for that element.
fn apply_colors(terminal: &vte4::Terminal, colors: &ProfileColors) {
    terminal.set_bold_is_bright(colors.bold_is_bright);

    let cursor = colors.cursor_colors.as_ref();
    let cursor_bg = cursor.map(|pair| parse_or_fallback(&pair.background));
    let cursor_fg = cursor.map(|pair| parse_or_fallback(&pair.foreground));
    terminal.set_color_cursor(cursor_bg.as_ref());
    terminal.set_color_cursor_foreground(cursor_fg.as_ref());

    let highlight = colors.highlight_colors.as_ref();
    let highlight_bg = highlight.map(|pair| parse_or_fallback(&pair.background));
    let highlight_fg = highlight.map(|pair| parse_or_fallback(&pair.foreground));
    terminal.set_color_highlight(highlight_bg.as_ref());
    terminal.set_color_highlight_foreground(highlight_fg.as_ref());

    let bold = colors.bold_color.as_deref().map(parse_or_fallback);
    terminal.set_color_bold(bold.as_ref());
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

/// Shows `icon` on every bell — when the profile's current `BellMode`
/// asks for it — and hides it once no bell has rung for
/// `BELL_ICON_DURATION`; a new bell while it's shown restarts the timer.
fn connect_bell_icon(terminal: &vte4::Terminal, icon: &gtk4::Image, bell: Rc<Cell<BellMode>>) {
    let pending: Rc<Cell<Option<glib::SourceId>>> = Rc::default();
    let icon = icon.downgrade();
    terminal.connect_bell(move |_| {
        let Some(icon) = icon.upgrade() else {
            return;
        };
        if !bell.get().shows_icon() {
            return;
        }
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

/// Tilix's automatic profile switching: every time the shell reports a new
/// directory (OSC 7), look for a profile whose rules match where the
/// terminal now is, and fall back to the session's own profile
/// (`base_profile_id`) once none does. Only re-applies when the matched
/// profile actually changes.
fn connect_automatic_switch(
    terminal: &vte4::Terminal,
    appearance: &Rc<Appearance>,
    resolver: ProfileResolver,
    base_profile_id: ProfileId,
) {
    let appearance = Rc::downgrade(appearance);
    terminal.connect_current_directory_uri_changed(move |terminal| {
        let Some(appearance) = appearance.upgrade() else {
            return;
        };
        let Some(uri) = terminal.current_directory_uri() else {
            return;
        };
        let Some(directory) = title::directory_from_uri(&uri) else {
            return;
        };
        let state = TerminalState {
            user: title::current_user(),
            host: title::host_from_uri(&uri).or_else(title::local_hostname),
            directory,
        };
        let target = resolver
            .resolve_automatic(&state)
            .or_else(|| resolver.try_resolve(&base_profile_id));
        if let Some(target) = target
            && target.profile.id != *appearance.profile_id.borrow()
        {
            appearance.apply(&target);
        }
    });
}
