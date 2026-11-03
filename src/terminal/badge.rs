use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, pango};
use vte4::TerminalExt;

use crate::layout::PaneId;
use crate::profile::{Badge, BadgePosition};
use crate::terminal::title;

/// Distance from the terminal's edges, in pixels.
const MARGIN: i32 = 12;

/// Tilix's badge: large text in one corner of the terminal, re-rendered
/// from the profile's template (`${title}`, `${directory}`, ...) whenever
/// the shell reports a new title or directory.
///
/// Tilix paints it into the terminal's own background, under the text.
/// Here it's a `gtk4::Label` overlaid on top of the terminal instead —
/// semi-transparent and ignoring the pointer so it never gets in the way
/// of selecting what's underneath — which avoids hooking into vte's
/// drawing.
pub struct BadgeOverlay {
    label: gtk4::Label,
    state: Rc<RefCell<BadgeState>>,
}

/// What the template is rendered from, updated by the terminal's signals.
struct BadgeState {
    template: String,
    pane_id: PaneId,
    title: Option<String>,
    directory: Option<String>,
}

impl BadgeState {
    fn render(&self) -> String {
        let host = title::local_hostname();
        let user = title::current_user();
        title::render_template(
            &self.template,
            &title::TitleContext {
                id: self.pane_id,
                title: self.title.as_deref(),
                directory: self.directory.as_deref(),
                host: host.as_deref(),
                user: user.as_deref(),
            },
        )
    }
}

impl BadgeOverlay {
    /// An empty, hidden badge — `update` gives it the profile's settings.
    /// It tracks the terminal's title and directory from the start, so a
    /// later `update` (automatic profile switching) renders right away.
    pub fn new(terminal: &vte4::Terminal, pane_id: PaneId) -> Self {
        let label = gtk4::Label::builder()
            .margin_top(MARGIN)
            .margin_bottom(MARGIN)
            .margin_start(MARGIN)
            .margin_end(MARGIN)
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .opacity(0.35)
            .can_target(false)
            .visible(false)
            .build();

        let state = Rc::new(RefCell::new(BadgeState {
            template: String::new(),
            pane_id,
            title: None,
            directory: None,
        }));
        {
            let state = state.clone();
            let label = label.downgrade();
            terminal.connect_window_title_changed(move |terminal| {
                let Some(label) = label.upgrade() else {
                    return;
                };
                state.borrow_mut().title = terminal.window_title().map(|s| s.to_string());
                refresh(&label, &state.borrow());
            });
        }
        {
            let state = state.clone();
            let label = label.downgrade();
            terminal.connect_current_directory_uri_changed(move |terminal| {
                let Some(label) = label.upgrade() else {
                    return;
                };
                state.borrow_mut().directory = terminal
                    .current_directory_uri()
                    .and_then(|uri| title::directory_from_uri(&uri));
                refresh(&label, &state.borrow());
            });
        }

        Self { label, state }
    }

    /// Applies a profile's badge settings. `color`: the profile's badge
    /// color, or the scheme's foreground. Call after the terminal's font
    /// is set — a "system font" badge is sized from it.
    pub fn update(&self, terminal: &vte4::Terminal, badge: &Badge, color: &gdk::RGBA) {
        let (halign, valign) = match badge.badge_position {
            BadgePosition::Northwest => (gtk4::Align::Start, gtk4::Align::Start),
            BadgePosition::Northeast => (gtk4::Align::End, gtk4::Align::Start),
            BadgePosition::Southwest => (gtk4::Align::Start, gtk4::Align::End),
            BadgePosition::Southeast => (gtk4::Align::End, gtk4::Align::End),
        };
        self.label.set_halign(halign);
        self.label.set_valign(valign);
        self.label.set_justify(if halign == gtk4::Align::End {
            gtk4::Justification::Right
        } else {
            gtk4::Justification::Left
        });
        self.label
            .set_attributes(Some(&attributes(terminal, badge, color)));

        let mut state = self.state.borrow_mut();
        state.template = badge.badge_text.clone();
        refresh(&self.label, &state);
    }

    pub fn widget(&self) -> &gtk4::Label {
        &self.label
    }
}

fn refresh(label: &gtk4::Label, state: &BadgeState) {
    if state.template.trim().is_empty() {
        label.set_visible(false);
        return;
    }
    label.set_text(&state.render());
    label.set_visible(true);
}

/// Color and font as Pango attributes rather than CSS, so each pane's
/// badge can differ without a `CssProvider` per label.
fn attributes(terminal: &vte4::Terminal, badge: &Badge, color: &gdk::RGBA) -> pango::AttrList {
    let font = if badge.badge_use_system_font {
        // Tilix: twice the terminal's own font size.
        terminal.font().map(|mut font| {
            font.set_size(font.size() * 2);
            font
        })
    } else {
        Some(pango::FontDescription::from_string(&badge.badge_font))
    };

    let attrs = pango::AttrList::new();
    let to_u16 = |channel: f32| (channel.clamp(0.0, 1.0) * f32::from(u16::MAX)) as u16;
    attrs.insert(pango::AttrColor::new_foreground(
        to_u16(color.red()),
        to_u16(color.green()),
        to_u16(color.blue()),
    ));
    if let Some(font) = font {
        attrs.insert(pango::AttrFontDesc::new(&font));
    }
    attrs
}
