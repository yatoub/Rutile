use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::gio;
use gtk4::prelude::*;
use vte4::TerminalExt;

use crate::preferences::Preferences;
use crate::terminal::title;

/// Schemes we'll actually hand to `AppInfo::launch_default_for_uri` for an
/// OSC 8 hyperlink or a plain-text URL match. Both come straight from
/// whatever's running in the terminal (an attacker-controlled `cat`/`curl`
/// output, a compromised build script, ...), so this isn't optional
/// hardening: an unfiltered scheme lets arbitrary output silently trigger
/// any URI handler registered on the system (custom app schemes,
/// `javascript:` under some handler, etc.), not just "open a browser tab".
const ALLOWED_SCHEMES: &[&str] = &["http://", "https://", "mailto:", "file://"];

/// PCRE2 pattern VTE matches natively against every visible line to decide
/// what to underline+hand-cursor on mere hover (see `sync_url_match`) —
/// this is what actually gives Tilix-parity underlining for bare
/// `http(s)://` URLs in plain output, independent of Ctrl.
const URL_PATTERN: &str = r"https?://[^\s<>\x22']+";

/// PCRE2's `PCRE2_MULTILINE` compile flag — VTE requires (and asserts at
/// runtime on) every regex passed to `match_add_regex` to have been
/// compiled with it, since it matches against one screen row at a time.
/// `vte4`/`pcre2` don't export this constant, so it's inlined from
/// `pcre2.h` (`0x00000400`).
const PCRE2_MULTILINE: u32 = 0x0000_0400;

/// Wires up hover feedback and Ctrl+Click-to-open on a terminal, in
/// priority order:
/// 1. An OSC 8 hyperlink (the escape sequence modern CLI tools —
///    `ls --hyperlink`, `rg`, various formatters — use to mark up a piece
///    of output as a real link) under the pointer, if any. VTE underlines
///    these itself the moment `allow-hyperlink` is on — nothing to add.
/// 2. A plain `http(s)://` URL token in the output — VTE doesn't know
///    these are links either, but `Terminal::match_add_regex` (see
///    `sync_url_match`) makes it treat the pattern as one purely for
///    hover styling (underline + hand cursor on mere hover, no Ctrl
///    needed), matching Tilix. Actually *opening* it on Ctrl+Click still
///    goes through our own token extraction below, not VTE's match
///    engine (see the next doc paragraph for why).
/// 3. Otherwise, a plain filesystem path token under the pointer (e.g. a
///    bare path in `ls`/`grep`/error output) that actually exists on
///    disk — opened the same way, so a directory opens in the file
///    manager (its registered default handler) and a file opens in
///    whatever app is default for it.
///
/// Cases 2 and 3 both resolve *what to open* via our own
/// `text_range_format`-based token extraction, not VTE's match engine:
/// `docs/ROADMAP.md`'s Phase 3 plan flagged "click a bare URL" as the
/// riskiest item, contingent on `vte_terminal_match_check_event` — a call
/// the `vte4` crate doesn't bind at all (checked: no
/// `match_check`/`hyperlink_check` binding anywhere in its generated
/// API). `match_add_regex` alone (no `match_check`) is enough for VTE's
/// *visual* hover feedback since that's driven entirely inside VTE's own
/// mouse handling, but doesn't let an app ask "what matched here" —
/// that's still exactly the missing piece, so the actual open action
/// keeps using our own detection instead.
///
/// Gated end-to-end by `Preferences::enable_hyperlinks` (checked live on
/// every motion/click, not just at attach time, so toggling it in
/// Preferences takes effect immediately without re-attaching): when off,
/// `allow-hyperlink` goes back off (killing native OSC 8 hover styling)
/// and the URL match registered in (2) is removed (killing its hover
/// styling too), on top of our own click handler no-op'ing.
///
/// Hover *cursor* feedback for the plain-path case (3) is still manual
/// (Ctrl held + pointer cursor) since, unlike a URL regex, "is this a
/// real path" can only be answered by resolving it against the
/// filesystem — not something `match_add_regex` alone can express.
pub fn attach(terminal: &vte4::Terminal, prefs: Rc<RefCell<Preferences>>) {
    terminal.set_allow_hyperlink(prefs.borrow().enable_hyperlinks);

    let url_match_tag: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
    sync_url_match(terminal, prefs.borrow().enable_hyperlinks, &url_match_tag);

    let showing_pointer = Rc::new(Cell::new(false));

    let motion = gtk4::EventControllerMotion::new();
    {
        let terminal = terminal.clone();
        let prefs = prefs.clone();
        let showing_pointer = showing_pointer.clone();
        let url_match_tag = url_match_tag.clone();
        motion.connect_motion(move |controller, x, y| {
            let enabled = prefs.borrow().enable_hyperlinks;
            terminal.set_allow_hyperlink(enabled);
            sync_url_match(&terminal, enabled, &url_match_tag);

            let ctrl = controller
                .current_event_state()
                .contains(gdk::ModifierType::CONTROL_MASK);
            let wants_pointer = enabled
                && ctrl
                && terminal.check_hyperlink_at(x, y).is_none()
                && plain_text_uri_at(&terminal, x, y).is_some();

            if wants_pointer != showing_pointer.get() {
                terminal.set_cursor_from_name(if wants_pointer { Some("pointer") } else { None });
                showing_pointer.set(wants_pointer);
            }
        });
    }
    {
        let terminal = terminal.clone();
        let showing_pointer = showing_pointer.clone();
        motion.connect_leave(move |_| {
            if showing_pointer.get() {
                terminal.set_cursor_from_name(None);
                showing_pointer.set(false);
            }
        });
    }
    terminal.add_controller(motion);

    let click = gtk4::GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    // Capture phase: run before VTE's own click handling gets a chance to
    // treat this as a normal selection click.
    click.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let terminal = terminal.clone();
        click.connect_pressed(move |gesture, _n_press, x, y| {
            if !prefs.borrow().enable_hyperlinks {
                return;
            }
            let state = gesture.current_event_state();
            if !state.contains(gdk::ModifierType::CONTROL_MASK) {
                return;
            }

            let uri = terminal
                .check_hyperlink_at(x, y)
                .map(|uri| uri.to_string())
                .or_else(|| plain_text_uri_at(&terminal, x, y));

            let Some(uri) = uri else { return };
            if !ALLOWED_SCHEMES.iter().any(|scheme| uri.starts_with(scheme)) {
                eprintln!("[rutile] refusing to open hyperlink with disallowed scheme: {uri}");
                return;
            }
            if let Err(err) =
                gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE)
            {
                eprintln!("[rutile] failed to open hyperlink {uri}: {err}");
            }
        });
    }
    terminal.add_controller(click);
}

/// Registers (or unregisters) `URL_PATTERN` as a VTE match so it gets
/// underlined with a hand cursor on mere hover — purely visual, VTE does
/// this hit-testing itself on every mouse-motion event internally, no
/// polling needed from us. Idempotent: cheap to call from the motion
/// handler on every event to keep it in sync with a live preference
/// toggle.
fn sync_url_match(terminal: &vte4::Terminal, enabled: bool, tag: &Cell<Option<i32>>) {
    match (enabled, tag.get()) {
        (true, None) => {
            if let Ok(regex) = vte4::Regex::for_match(URL_PATTERN, PCRE2_MULTILINE) {
                let new_tag = terminal.match_add_regex(&regex, 0);
                terminal.match_set_cursor_name(new_tag, "pointer");
                tag.set(Some(new_tag));
            }
        }
        (false, Some(old_tag)) => {
            terminal.match_remove(old_tag);
            tag.set(None);
        }
        _ => {}
    }
}

/// If the token of non-whitespace text under pixel position `(x, y)`
/// is a `http(s)://` URL, or looks like (and resolves to) a real
/// filesystem path, returns the URI to open. `None` for a path that
/// doesn't exist on disk — the guard against false positives (`col + 1`
/// off some unrelated word, a partially-selected token, ...), not just a
/// nicety.
fn plain_text_uri_at(terminal: &vte4::Terminal, x: f64, y: f64) -> Option<String> {
    let char_width = terminal.char_width();
    let char_height = terminal.char_height();
    if char_width <= 0 || char_height <= 0 {
        return None;
    }
    let col = (x / char_width as f64) as usize;
    let first_row = terminal.vadjustment()?.value() as i64;
    let row = first_row + (y / char_height as f64) as i64;

    let (text, _len) =
        terminal.text_range_format(vte4::Format::Text, row, 0, row, terminal.column_count());
    let text = text?;
    let token = word_at(&text, col)?;
    let token = token.trim_matches(|c: char| "\"'()[]{}:,;".contains(c));
    if token.is_empty() {
        return None;
    }

    if token.starts_with("http://") || token.starts_with("https://") {
        return Some(token.to_string());
    }

    let path = resolve_path(terminal, token)?;
    std::fs::metadata(&path).ok()?;
    Some(gio::File::for_path(&path).uri().to_string())
}

/// The contiguous run of non-whitespace characters in `text` that
/// contains character column `col`, if any.
fn word_at(text: &str, col: usize) -> Option<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let (byte_at_col, c) = *chars.get(col)?;
    if c.is_whitespace() {
        return None;
    }
    let start = chars[..=col]
        .iter()
        .rev()
        .take_while(|(_, c)| !c.is_whitespace())
        .last()
        .map(|(i, _)| *i)
        .unwrap_or(byte_at_col);
    let end = chars[col..]
        .iter()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, _)| *i)
        .unwrap_or(text.len());
    Some(&text[start..end])
}

/// Resolves `token` (an absolute path, `~`-relative, or relative to the
/// terminal's own cwd via OSC 7) into an absolute `PathBuf`. Doesn't
/// check existence — the caller does that as the actual validity gate.
fn resolve_path(terminal: &vte4::Terminal, token: &str) -> Option<std::path::PathBuf> {
    if let Some(rest) = token.strip_prefix('~') {
        let home = std::env::var("HOME").ok()?;
        return Some(std::path::PathBuf::from(home).join(rest.trim_start_matches('/')));
    }
    if token.starts_with('/') {
        return Some(std::path::PathBuf::from(token));
    }
    let cwd = terminal
        .current_directory_uri()
        .and_then(|uri| title::directory_from_uri(&uri))?;
    Some(std::path::PathBuf::from(cwd).join(token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_at_finds_token_containing_column() {
        assert_eq!(word_at("open /etc/hosts now", 6), Some("/etc/hosts"));
    }

    #[test]
    fn word_at_returns_none_on_whitespace_column() {
        assert_eq!(word_at("open /etc/hosts now", 4), None);
    }

    #[test]
    fn word_at_returns_none_past_end_of_line() {
        assert_eq!(word_at("short", 50), None);
    }

    #[test]
    fn word_at_finds_url_token() {
        assert_eq!(
            word_at("see https://example.com/path for details", 5),
            Some("https://example.com/path")
        );
    }
}
