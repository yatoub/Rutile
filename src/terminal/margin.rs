use gtk4::prelude::*;
use vte4::TerminalExt;

/// Tilix's "Toggle Margin": a thin vertical guide line drawn at
/// `COLUMN` characters from the left edge, to help keep output within a
/// conventional line width. Purely cosmetic — never clips or wraps
/// anything, just overlays a `gtk4::Separator` positioned by the
/// terminal's current character width (kept in sync via
/// `connect_char_size_changed`, since resizing the font/window changes how
/// many pixels 80 columns actually spans).
pub struct MarginGuide {
    separator: gtk4::Separator,
}

const COLUMN: i32 = 80;

impl MarginGuide {
    pub fn new(terminal: &vte4::Terminal) -> Self {
        let separator = gtk4::Separator::new(gtk4::Orientation::Vertical);
        separator.add_css_class("margin-guide");
        separator.set_halign(gtk4::Align::Start);
        separator.set_valign(gtk4::Align::Fill);
        separator.set_visible(false);
        separator.set_can_target(false);
        update_position(&separator, terminal);

        {
            let separator = separator.clone();
            terminal.connect_char_size_changed(move |terminal, _width, _height| {
                update_position(&separator, terminal);
            });
        }

        Self { separator }
    }

    pub fn widget(&self) -> &gtk4::Separator {
        &self.separator
    }

    pub fn toggle(&self) {
        self.separator.set_visible(!self.separator.is_visible());
    }
}

fn update_position(separator: &gtk4::Separator, terminal: &vte4::Terminal) {
    let char_width = terminal.char_width().max(1) as i32;
    separator.set_margin_start(char_width * COLUMN);
}
