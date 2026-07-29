use gtk4::gdk;
use gtk4::gio;
use gtk4::prelude::*;
use vte4::TerminalExt;

use crate::terminal::title;

/// Schemes we'll actually hand to `AppInfo::launch_default_for_uri` for an
/// OSC 8 hyperlink. OSC 8 payloads come straight from whatever's running
/// in the terminal (an attacker-controlled `cat`/`curl` output, a
/// compromised build script, ...), so this isn't optional hardening: an
/// unfiltered scheme lets arbitrary output silently trigger any URI
/// handler registered on the system (custom app schemes, `javascript:`
/// under some handler, etc.), not just "open a browser tab".
const ALLOWED_SCHEMES: &[&str] = &["http://", "https://", "mailto:", "file://"];

/// Wires up Ctrl+Click-to-open on a terminal, in priority order:
/// 1. An OSC 8 hyperlink (the escape sequence modern CLI tools —
///    `ls --hyperlink`, `rg`, various formatters — use to mark up a piece
///    of output as a real link) under the pointer, if any.
/// 2. Otherwise, a plain filesystem path token under the pointer (e.g. a
///    bare path in `ls`/`grep`/error output) that actually exists on
///    disk — opened the same way, so a directory opens in the file
///    manager (its registered default handler) and a file opens in
///    whatever app is default for it.
///
/// Bare-URL detection (arbitrary `https://...` text that isn't a real OSC
/// 8 hyperlink) is deliberately NOT attempted: `docs/ROADMAP.md`'s Phase 3
/// plan flagged that as the riskiest item, contingent on
/// `vte_terminal_match_check_event` — a call the `vte4` crate doesn't
/// bind at all (checked: no `match_check`/`hyperlink_check` binding
/// anywhere in its generated API). Reaching for raw FFI onto a function
/// the crate doesn't expose was rejected as disproportionate for one
/// feature. The plain-path case above sidesteps that gap entirely: it
/// reads the row's text via `text_range_format` (available, unlike
/// match-checking) and does its own token/existence check instead of
/// relying on VTE's internal match engine.
pub fn attach(terminal: &vte4::Terminal) {
    terminal.set_allow_hyperlink(true);

    let click = gtk4::GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    // Capture phase: run before VTE's own click handling gets a chance to
    // treat this as a normal selection click.
    click.set_propagation_phase(gtk4::PropagationPhase::Capture);
    {
        let terminal = terminal.clone();
        click.connect_pressed(move |gesture, _n_press, x, y| {
            let state = gesture.current_event_state();
            if !state.contains(gdk::ModifierType::CONTROL_MASK) {
                return;
            }

            let uri = terminal
                .check_hyperlink_at(x, y)
                .map(|uri| uri.to_string())
                .or_else(|| system_path_uri_at(&terminal, x, y));

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

/// If the token of non-whitespace text under pixel position `(x, y)`
/// looks like (and resolves to) a real filesystem path, returns its
/// `file://` URI. `None` for anything that doesn't exist on disk — this
/// is the guard against false positives (`col + 1` off some unrelated
/// word, a partially-selected token, ...), not just a nicety.
fn system_path_uri_at(terminal: &vte4::Terminal, x: f64, y: f64) -> Option<String> {
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
}
