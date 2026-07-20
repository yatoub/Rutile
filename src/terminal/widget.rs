use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use vte4::TerminalExt;
use vte4::TerminalExtManual;

pub struct TerminalWidget {
    pub terminal: vte4::Terminal,
}

/// Catppuccin Mocha ANSI 16-color palette + default fg/bg. vte does not
/// theme through GTK CSS, so these are applied directly via `set_colors()`.
fn catppuccin_mocha_colors() -> (gdk::RGBA, gdk::RGBA, [gdk::RGBA; 16]) {
    let rgba = |hex: &str| gdk::RGBA::parse(hex).expect("valid hex color");

    let foreground = rgba("#cdd6f4");
    let background = rgba("#1e1e2e");
    let palette = [
        rgba("#45475a"), // black
        rgba("#f38ba8"), // red
        rgba("#a6e3a1"), // green
        rgba("#f9e2af"), // yellow
        rgba("#89b4fa"), // blue
        rgba("#f5c2e7"), // magenta
        rgba("#94e2d5"), // cyan
        rgba("#bac2de"), // white
        rgba("#585b70"), // bright black
        rgba("#f38ba8"), // bright red
        rgba("#a6e3a1"), // bright green
        rgba("#f9e2af"), // bright yellow
        rgba("#89b4fa"), // bright blue
        rgba("#f5c2e7"), // bright magenta
        rgba("#94e2d5"), // bright cyan
        rgba("#a6adc8"), // bright white
    ];

    (foreground, background, palette)
}

impl TerminalWidget {
    pub fn new() -> Self {
        let terminal = vte4::Terminal::new();
        // Without this, a terminal nested inside a Paned doesn't claim its
        // share of space, so deeper splits can collapse a pane down to
        // (near) zero size instead of an even split.
        terminal.set_vexpand(true);
        terminal.set_hexpand(true);

        let (foreground, background, palette) = catppuccin_mocha_colors();
        let palette_refs: Vec<&gdk::RGBA> = palette.iter().collect();
        terminal.set_colors(Some(&foreground), Some(&background), &palette_refs);

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        terminal.spawn_async(
            vte4::PtyFlags::DEFAULT,
            None,
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
}

impl Default for TerminalWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalWidget {
    pub fn widget(&self) -> &vte4::Terminal {
        &self.terminal
    }

    pub fn feed(&self, bytes: &[u8]) {
        self.terminal.feed_child(bytes);
    }
}
