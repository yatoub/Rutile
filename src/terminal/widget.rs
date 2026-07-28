use gtk4::glib;
use gtk4::prelude::*;
use vte4::TerminalExt;
use vte4::TerminalExtManual;

use crate::profile::ColorScheme;

pub struct TerminalWidget {
    pub terminal: vte4::Terminal,
}

impl TerminalWidget {
    /// `cwd`: the shell's starting directory, used when restoring a saved
    /// session (`session::persist`) — `None` spawns in vte's own default
    /// (inherited from this process), same as before session persistence.
    pub fn new(scheme: &ColorScheme, cwd: Option<&str>) -> Self {
        let terminal = vte4::Terminal::new();
        // Without this, a terminal nested inside a Paned doesn't claim its
        // share of space, so deeper splits can collapse a pane down to
        // (near) zero size instead of an even split.
        terminal.set_vexpand(true);
        terminal.set_hexpand(true);

        let foreground = scheme.foreground_rgba();
        let background = scheme.background_rgba();
        let palette = scheme.palette_rgba();
        let palette_refs: Vec<&gtk4::gdk::RGBA> = palette.iter().collect();
        terminal.set_colors(Some(&foreground), Some(&background), &palette_refs);

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        terminal.spawn_async(
            vte4::PtyFlags::DEFAULT,
            cwd,
            &[&shell],
            &[],
            glib::SpawnFlags::DEFAULT,
            || {},
            -1,
            gtk4::gio::Cancellable::NONE,
            |result| {
                if let Err(err) = result {
                    eprintln!("[rutile] spawn_async failed: {err}");
                }
            },
        );

        Self { terminal }
    }

    pub fn widget(&self) -> &vte4::Terminal {
        &self.terminal
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
