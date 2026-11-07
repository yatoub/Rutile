//! What happens to clipboard text between the clipboard and the terminal —
//! Tilix's paste settings and its advanced paste transforms. No GTK here:
//! the dialogs and the actual paste live in `dialogs::paste`.

/// Tilix's heuristic for a paste worth a warning: it mentions `sudo` and
/// spans several lines, so it would run commands as root on its own.
pub fn is_unsafe(text: &str) -> bool {
    text.contains("sudo") && text.contains('\n')
}

/// Whether the advanced paste dialog has anything to offer — Tilix pastes
/// a single line straight away, dialog or not.
pub fn is_multiline(text: &str) -> bool {
    text.contains('\n')
}

/// The global paste preferences that rewrite the text of a plain paste.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PasteCleanup {
    /// Drop trailing whitespace — including the final line break, so a
    /// copied command line doesn't run as soon as it's pasted.
    pub strip_trailing_whitespace: bool,
    /// Drop a leading `#` or `$`: a command copied along with its shell
    /// prompt (or commented out) in a how-to.
    pub strip_first_char: bool,
}

/// Applies `cleanup` to a plain paste, in Tilix's order: trailing
/// whitespace first, then the first character.
pub fn clean(text: &str, cleanup: PasteCleanup) -> &str {
    let mut text = text;
    if cleanup.strip_trailing_whitespace {
        text = text.trim_end();
    }
    if cleanup.strip_first_char
        && let Some(rest) = text.strip_prefix(['#', '$'])
    {
        text = rest;
    }
    text
}

/// The advanced paste dialog's transforms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AdvancedTransform {
    /// Replace tabs with spaces up to the next tab stop, every this many
    /// columns. `None`: keep tabs.
    pub tabs_to_spaces: Option<usize>,
    /// Turn CRLF and lone CR line endings into LF.
    pub normalize_line_endings: bool,
}

pub fn transform(text: &str, transform: AdvancedTransform) -> String {
    let mut text = text.to_string();
    if let Some(width) = transform.tabs_to_spaces {
        text = detab(&text, width);
    }
    if transform.normalize_line_endings {
        text = text.replace("\r\n", "\n").replace('\r', "\n");
    }
    text
}

/// Expands tabs into spaces, aligned on tab stops every `width` columns
/// (at least 1), the column count restarting on every line.
fn detab(text: &str, width: usize) -> String {
    let width = width.max(1);
    let mut out = String::with_capacity(text.len());
    let mut column = 0;
    for c in text.chars() {
        match c {
            '\t' => {
                let spaces = width - column % width;
                out.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' | '\r' => {
                out.push(c);
                column = 0;
            }
            _ => {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_needs_both_sudo_and_a_line_break() {
        assert!(is_unsafe("sudo rm -rf /tmp/x\n"));
        assert!(is_unsafe("echo hi\nsudo reboot"));
        assert!(!is_unsafe("sudo apt update"));
        assert!(!is_unsafe("ls\npwd\n"));
    }

    #[test]
    fn clean_strips_trailing_whitespace_then_the_prompt_character() {
        let both = PasteCleanup {
            strip_trailing_whitespace: true,
            strip_first_char: true,
        };
        assert_eq!(clean("$ ls -la  \n", both), " ls -la");
        assert_eq!(clean("# pacman -Syu\n", both), " pacman -Syu");
        // Only one character, and only `#` or `$`.
        assert_eq!(clean("$$x", both), "$x");
        assert_eq!(clean("ls", both), "ls");
        // Leading whitespace is kept, so is inner whitespace.
        assert_eq!(clean("  a\n b \n", both), "  a\n b");
    }

    #[test]
    fn clean_does_nothing_unless_asked() {
        let text = "$ echo hi \n";
        assert_eq!(clean(text, PasteCleanup::default()), text);
        let first_only = PasteCleanup {
            strip_first_char: true,
            ..PasteCleanup::default()
        };
        assert_eq!(clean(text, first_only), " echo hi \n");
    }

    #[test]
    fn stripping_the_line_break_can_make_a_paste_safe() {
        let trailing = PasteCleanup {
            strip_trailing_whitespace: true,
            ..PasteCleanup::default()
        };
        assert!(is_unsafe("sudo reboot\n"));
        assert!(!is_unsafe(clean("sudo reboot\n", trailing)));
    }

    #[test]
    fn detab_aligns_on_tab_stops_per_line() {
        let t = AdvancedTransform {
            tabs_to_spaces: Some(4),
            ..AdvancedTransform::default()
        };
        assert_eq!(transform("\tx", t), "    x");
        assert_eq!(transform("ab\tc", t), "ab  c");
        assert_eq!(transform("abcd\te", t), "abcd    e");
        assert_eq!(transform("a\tb\n\tc", t), "a   b\n    c");
        // A zero width (the spin button allows it) acts as 1.
        let zero = AdvancedTransform {
            tabs_to_spaces: Some(0),
            ..AdvancedTransform::default()
        };
        assert_eq!(transform("a\tb", zero), "a b");
    }

    #[test]
    fn line_endings_become_lf() {
        let t = AdvancedTransform {
            normalize_line_endings: true,
            ..AdvancedTransform::default()
        };
        assert_eq!(transform("a\r\nb\rc\n", t), "a\nb\nc\n");
        assert_eq!(
            transform("a\r\n", AdvancedTransform::default()),
            "a\r\n",
            "untouched unless asked"
        );
    }
}
