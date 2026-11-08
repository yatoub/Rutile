//! Tilix's triggers: regular expressions matched against what a terminal
//! prints, each running an action on a match. No GTK here — the terminal
//! is reached through the `Screen` trait, so the scanner is tested on a
//! fake screen; `terminal::widget` wires it to vte and runs the actions.
//!
//! # How output is scanned
//!
//! vte has no "a line was printed" signal: on every `contents-changed` the
//! scanner reads the rows it hasn't finished with yet, which a spike
//! (`docs/ROADMAP.md`, Phase 5) showed has to be done *by logical line*,
//! unlike Tilix's (row, column) watermark:
//!
//! - A line is complete once the cursor has moved below it; only then is
//!   it matched for good. The cursor's own line is matched again on every
//!   change, so a prompt that never ends with a line break (`Password: `)
//!   still triggers, and a line printed in several chunks, or rewritten
//!   with `\r` (a progress bar), is seen whole.
//! - Every match is remembered by (line start row, trigger, offset, text),
//!   so re-reading a line never fires the same match twice. Entries are
//!   kept for `DEDUPE_ROWS` rows behind the scan position, also across a
//!   resync, because leaving the alternate screen (vim, less) puts the
//!   cursor back below rows already scanned, which then get re-read.
//! - Rows wrapped by the terminal are joined back into one line: vte only
//!   ends a row with `\n` at a real line end.
//! - At most `max_lines` rows are read per change. When that cuts a line
//!   in the middle, the cut fragment is skipped up to its end: its start
//!   row would otherwise shift on every change and defeat the dedupe.
//!
//! Known limits, by design: a row rewritten *above* the cursor (cursor
//! movement) isn't seen again; output too fast for vte's scrollback can
//! scroll away between two changes unseen; and the alternate screen is
//! scanned like the normal one — vte doesn't tell them apart (Tilix only
//! skips it with its own patched vte).

use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::i18n::tr;

/// How many rows behind the scan position matches are remembered.
const DEDUPE_ROWS: i64 = 1024;

/// A trigger as stored in `preferences.toml` (global) or `profiles.toml`
/// (per profile).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trigger {
    pub pattern: String,
    pub action: TriggerAction,
    /// `$0` (whole match) to `$9` (groups) are replaced by what matched.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parameter: String,
}

/// What a trigger does. The first five only change what Rutile shows; the
/// last three act on the system (`is_active`) — any program's output can
/// fire them, a `cat` of a file or a remote server included, hence their
/// safeguards: captured text never reaches a shell and loses its control
/// characters, and a trigger firing too often is disabled (`FiringGuard`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerAction {
    /// Set the terminal's user, host and/or directory, as if the shell had
    /// reported them — for titles and automatic profile switching.
    /// Parameter: `username=$1;hostname=$2;directory=$3` (any subset).
    UpdateState,
    /// A desktop notification, when the terminal doesn't have the focus.
    /// Parameter: the body, or `title=…;body=…`.
    #[default]
    SendNotification,
    /// Replace the pane title template (`${title}`, `${directory}`… work).
    UpdateTitle,
    /// Replace the badge text template.
    UpdateBadge,
    /// Ring the bell.
    PlayBell,
    /// Type text into the terminal's program. Parameter: the text, with
    /// `\n`, `\r`, `\t` and `\\` escapes (in the template only).
    SendText,
    /// Start a program in the background. Parameter: a command line,
    /// split into arguments *before* `$n` are replaced.
    ExecuteCommand,
    /// Run a program and type its output into the terminal. Parameter:
    /// like `ExecuteCommand`.
    RunProcess,
}

impl TriggerAction {
    pub const ALL: [TriggerAction; 8] = [
        TriggerAction::UpdateState,
        TriggerAction::SendNotification,
        TriggerAction::UpdateTitle,
        TriggerAction::UpdateBadge,
        TriggerAction::PlayBell,
        TriggerAction::SendText,
        TriggerAction::ExecuteCommand,
        TriggerAction::RunProcess,
    ];

    /// Whether this action acts on the system rather than on the display.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            TriggerAction::SendText | TriggerAction::ExecuteCommand | TriggerAction::RunProcess
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            TriggerAction::UpdateState => tr("Update state"),
            TriggerAction::SendNotification => tr("Send notification"),
            TriggerAction::UpdateTitle => tr("Update title"),
            TriggerAction::UpdateBadge => tr("Update badge"),
            TriggerAction::PlayBell => tr("Play bell"),
            TriggerAction::SendText => tr("Send text"),
            TriggerAction::ExecuteCommand => tr("Execute command"),
            TriggerAction::RunProcess => tr("Run process and send its output"),
        }
    }

    /// What the parameter field expects, as its placeholder.
    pub fn parameter_hint(self) -> &'static str {
        match self {
            TriggerAction::UpdateState => "username=$1;hostname=$2;directory=$3",
            TriggerAction::SendNotification => "title=…;body=$0",
            TriggerAction::UpdateTitle => "${title} — $1",
            TriggerAction::UpdateBadge => "$1",
            TriggerAction::PlayBell => tr("(no parameter)"),
            TriggerAction::SendText => r"cd $1\n",
            TriggerAction::ExecuteCommand => r#"notify-send "Build done" $1"#,
            TriggerAction::RunProcess => "pass show $1",
        }
    }
}

/// What's wrong with a trigger's parameter, for the editor: an error makes
/// the trigger unusable, a warning flags a risk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterIssue {
    Error(String),
    Warning(String),
}

pub fn parameter_issue(action: TriggerAction, parameter: &str) -> Option<ParameterIssue> {
    if action == TriggerAction::SendText && has_placeholder(parameter) {
        // Control characters are stripped, but `$(…)`, backquotes or `;`
        // are printable: typed into a shell, captured text runs.
        return Some(ParameterIssue::Warning(
            tr(
                "Captured text is typed as is: a shell reading it would run $(…), `…` or ; in it — only use $n on output you trust",
            )
            .to_string(),
        ));
    }
    if !matches!(
        action,
        TriggerAction::ExecuteCommand | TriggerAction::RunProcess
    ) {
        return None;
    }
    let argv = match parse_command(parameter) {
        Ok(argv) => argv,
        Err(error) => return Some(ParameterIssue::Error(error)),
    };
    code_argument(&argv)
        .filter(|code| has_placeholder(code))
        .map(|_| {
            ParameterIssue::Warning(
                tr(
                    "Captured text would be run as code there: pass it as a separate argument instead (sh -c '… \"$$1\"' sh $1)",
                )
                .to_string(),
            )
        })
}

fn parse_command(template: &str) -> Result<Vec<String>, String> {
    if template.trim().is_empty() {
        return Err(tr("Empty command").to_string());
    }
    // GLib's shell-style splitting only handles quotes and backslashes:
    // nothing is expanded, nothing runs.
    gtk4::glib::shell_parse_argv(template)
        .map(|argv| {
            argv.into_iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(|err| err.message().to_string())
}

/// The command's argument that a shell or an interpreter runs as code
/// (`sh -c CODE`, `python -c CODE`, `perl -e CODE`…), if any — `env` and
/// its `VAR=value` arguments skipped.
fn code_argument(argv: &[String]) -> Option<&str> {
    let mut args = argv.iter().map(String::as_str).peekable();
    let mut program = args.next()?;
    while basename(program) == "env" || program.contains('=') {
        program = args.next()?;
    }
    let flag = match basename(program) {
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "mksh" | "ash" | "fish" | "busybox" => 'c',
        name if name.starts_with("python") => 'c',
        "perl" | "ruby" | "node" | "nodejs" => 'e',
        _ => return None,
    };
    while let Some(arg) = args.next() {
        // `-c`, or bundled short options like `-lc`.
        if arg.len() > 1 && arg.starts_with('-') && !arg.starts_with("--") && arg.contains(flag) {
            return args.next();
        }
    }
    None
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Whether `text` contains a `$n` that `substitute` would replace.
fn has_placeholder(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek() {
                Some('$') => {
                    chars.next();
                }
                Some(d) if d.is_ascii_digit() => return true,
                _ => {}
            }
        }
    }
    false
}

/// Drops control characters (line breaks, escape sequences, DEL, C1) from
/// captured text before it's typed or passed to a program: output could
/// otherwise end a command line and start another, or drive the shell's
/// line editor.
pub fn sanitize(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// The program and arguments an `ExecuteCommand`/`RunProcess` trigger
/// runs: the template split into arguments first, then `$n` replaced in
/// each with the sanitized groups — a group is always one argument's
/// content, whatever its quotes or spaces.
pub fn command_argv(template: &str, groups: &[String]) -> Result<Vec<String>, String> {
    let groups: Vec<String> = groups.iter().map(|g| sanitize(g)).collect();
    Ok(parse_command(template)?
        .iter()
        .map(|arg| substitute(arg, &groups))
        .collect())
}

/// The text a `SendText` trigger types: the template's escapes (`\n`,
/// `\r`, `\t`, `\\`) decoded, its `$n` replaced with the sanitized groups —
/// only the template can contain a line break.
pub fn send_text(template: &str, groups: &[String]) -> String {
    let groups: Vec<String> = groups.iter().map(|g| sanitize(g)).collect();
    expand(template, &groups, true)
}

/// A `RunProcess` program's output, before it's typed: at most `max_bytes`
/// (cut on a character boundary), control characters dropped except line
/// breaks and tabs — what the program prints is meant to be typed, escape
/// sequences aren't.
pub fn sanitize_process_output(output: &str, max_bytes: usize) -> String {
    let mut end = output.len().min(max_bytes);
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    output[..end]
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect()
}

/// Why a pattern can't be used, for the editor — `None` when it's valid.
pub fn pattern_error(pattern: &str) -> Option<String> {
    if pattern.is_empty() {
        return Some(tr("Empty pattern").to_string());
    }
    compile_pattern(pattern).err()
}

fn compile_pattern(pattern: &str) -> Result<regex::Regex, String> {
    // `regex` runs in linear time whatever the pattern and the output,
    // unlike a backtracking engine (PCRE): output isn't trusted, and a
    // pathological pattern must not be able to freeze the window.
    regex::RegexBuilder::new(pattern)
        .size_limit(1 << 20)
        .build()
        .map_err(|err| match err {
            regex::Error::Syntax(message) => message,
            regex::Error::CompiledTooBig(_) => tr("Pattern too complex").to_string(),
            other => other.to_string(),
        })
}

pub struct CompiledTrigger {
    regex: regex::Regex,
    pub pattern: String,
    pub action: TriggerAction,
    pub parameter: String,
}

/// Compiles the usable triggers, in order — an invalid pattern (the editor
/// flags those) is skipped.
pub fn compile(triggers: &[Trigger]) -> Vec<CompiledTrigger> {
    triggers
        .iter()
        .filter_map(|trigger| {
            Some(CompiledTrigger {
                regex: compile_pattern(&trigger.pattern).ok()?,
                pattern: trigger.pattern.clone(),
                action: trigger.action,
                parameter: trigger.parameter.clone(),
            })
        })
        .collect()
}

/// A trigger that matched: its index in the compiled list, and `$0`..`$n`
/// (an unmatched optional group is empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerMatch {
    pub trigger: usize,
    pub groups: Vec<String>,
}

/// Replaces `$0`..`$9` in `template` with `groups`, in a single pass:
/// matched text is never itself searched for `$n` (Tilix substitutes
/// repeatedly, so output containing `$2` would be expanded again). `$$` is
/// a literal `$` — to write a shell's own `$1`. Any other `$`, or a group
/// that doesn't exist, is kept as is.
pub fn substitute(template: &str, groups: &[String]) -> String {
    expand(template, groups, false)
}

/// `substitute`, plus backslash escapes when `escapes` is set — decoded in
/// the same single pass, so neither a group nor a decoded character is
/// ever read again.
fn expand(template: &str, groups: &[String], escapes: bool) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if escapes => {
                let decoded = match chars.peek() {
                    Some('n') => Some('\n'),
                    Some('r') => Some('\r'),
                    Some('t') => Some('\t'),
                    Some('\\') => Some('\\'),
                    _ => None,
                };
                match decoded {
                    Some(d) => {
                        chars.next();
                        out.push(d);
                    }
                    None => out.push('\\'),
                }
            }
            '$' => match chars.peek() {
                Some('$') => {
                    chars.next();
                    out.push('$');
                }
                Some(d) if d.is_ascii_digit() => {
                    let index = d.to_digit(10).unwrap_or(0) as usize;
                    match groups.get(index) {
                        Some(group) => {
                            chars.next();
                            out.push_str(group);
                        }
                        None => out.push('$'),
                    }
                }
                _ => out.push('$'),
            },
            c => out.push(c),
        }
    }
    out
}

/// `key=value;key=value` parameters (Tilix's syntax for notifications and
/// state updates), keys and values trimmed.
pub fn parameters(text: &str) -> Vec<(String, String)> {
    text.split(';')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

/// What an `UpdateState` trigger sets — unset parts are left alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateUpdate {
    pub username: Option<String>,
    pub hostname: Option<String>,
    pub directory: Option<String>,
}

impl StateUpdate {
    pub fn parse(parameter: &str) -> Self {
        let mut update = Self::default();
        for (key, value) in parameters(parameter) {
            let slot = match key.as_str() {
                "username" => &mut update.username,
                "hostname" => &mut update.hostname,
                "directory" => &mut update.directory,
                _ => continue,
            };
            if !value.is_empty() {
                *slot = Some(value);
            }
        }
        update
    }
}

/// A notification's title and body: `title=…;body=…`, or the whole
/// parameter as the body (Tilix's rule).
pub fn notification(parameter: &str) -> (Option<String>, String) {
    let params = parameters(parameter);
    let get = |key: &str| {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let title = get("title");
    let body = get("body").unwrap_or_else(|| {
        if title.is_some() {
            String::new()
        } else {
            parameter.to_string()
        }
    });
    (title, body)
}

/// What the scanner needs from a terminal.
pub trait Screen {
    /// The cursor's row, in vte's absolute row numbers (they keep growing,
    /// even past a `reset` or once old rows leave the scrollback).
    fn cursor_row(&self) -> i64;
    /// Row `row`'s text, as vte returns the range from (row, 0) to
    /// (row + 1, 0): ending with `\n` at a real line end, without one when
    /// the line wraps onto the next row — except the cursor's row, which
    /// always ends with one. A row out of the scrollback reads as `\n`.
    fn row_text(&self, row: i64) -> String;
}

/// Finds trigger matches in a terminal's output as it grows. See the
/// module documentation for the rules.
#[derive(Default)]
pub struct LineScanner {
    /// The first row whose logical line isn't complete yet.
    mark: i64,
    /// Matches already reported, by the row their line starts on.
    fired: BTreeMap<i64, HashSet<(usize, usize, String)>>,
    /// The line at `mark` lost its beginning to the `max_lines` window: it
    /// is skipped until it ends.
    skipping_cut_line: bool,
}

impl LineScanner {
    /// Reads what changed since the last call and returns the new matches,
    /// in the order they appear. `max_lines`: `None` for no limit.
    pub fn scan(
        &mut self,
        screen: &impl Screen,
        triggers: &[CompiledTrigger],
        max_lines: Option<usize>,
    ) -> Vec<TriggerMatch> {
        let mut found = Vec::new();
        if triggers.is_empty() {
            return found;
        }
        let cursor_row = screen.cursor_row();
        if cursor_row < self.mark {
            // Cursor moved up: leaving the alternate screen, a `clear`
            // into a fresh screen, an application redrawing. Resume from
            // there; the dedupe entries stay (see the module doc).
            self.mark = cursor_row;
            self.skipping_cut_line = false;
        }

        let mut row = self.mark;
        if let Some(max) = max_lines {
            let first_allowed = cursor_row - max as i64;
            if row < first_allowed {
                row = first_allowed;
                self.skipping_cut_line = true;
            }
        }

        while row <= cursor_row {
            let start = row;
            let mut line = String::new();
            let mut complete = false;
            while row <= cursor_row {
                let text = screen.row_text(row);
                let is_cursor_row = row == cursor_row;
                row += 1;
                let (text, hard_end) = match text.strip_suffix('\n') {
                    Some(text) => (text, !is_cursor_row),
                    None => (text.as_str(), false),
                };
                line.push_str(text);
                if hard_end {
                    complete = true;
                    break;
                }
            }
            if self.skipping_cut_line {
                // This line's beginning is out of reach: it's left alone,
                // until it ends.
                if complete {
                    self.skipping_cut_line = false;
                    self.mark = row;
                } else {
                    self.mark = start;
                }
                continue;
            }
            self.match_line(start, &line, triggers, &mut found);
            // A complete line is done; the cursor's line will be read again.
            self.mark = if complete { row } else { start };
        }

        let keep_from = self.mark - DEDUPE_ROWS;
        self.fired = self.fired.split_off(&keep_from);
        found
    }

    fn match_line(
        &mut self,
        start_row: i64,
        line: &str,
        triggers: &[CompiledTrigger],
        found: &mut Vec<TriggerMatch>,
    ) {
        let mut line_matches = Vec::new();
        for (index, trigger) in triggers.iter().enumerate() {
            for captures in trigger.regex.captures_iter(line) {
                let whole = captures.get(0).expect("group 0 always matches");
                let key = (index, whole.start(), whole.as_str().to_string());
                if !self.fired.entry(start_row).or_default().insert(key) {
                    continue;
                }
                let groups = captures
                    .iter()
                    .map(|group| group.map_or(String::new(), |g| g.as_str().to_string()))
                    .collect();
                line_matches.push((
                    whole.start(),
                    TriggerMatch {
                        trigger: index,
                        groups,
                    },
                ));
            }
        }
        line_matches.sort_by_key(|(offset, _)| *offset);
        found.extend(line_matches.into_iter().map(|(_, m)| m));
    }
}

/// Caps how often each trigger may fire: a flood of matching output (a log
/// replayed, a trigger matching its own effect) must not turn into a flood
/// of notifications or bells.
pub struct RateLimiter {
    max_per_window: u32,
    window: Duration,
    /// Per trigger index: when its current window started, and how many
    /// firings it has had in it.
    windows: Vec<(Instant, u32)>,
}

impl RateLimiter {
    pub fn new(max_per_window: u32, window: Duration) -> Self {
        Self {
            max_per_window,
            window,
            windows: Vec::new(),
        }
    }

    /// Whether `trigger` may fire at `now`, counting it if so.
    pub fn allow(&mut self, trigger: usize, now: Instant) -> bool {
        if self.windows.len() <= trigger {
            self.windows.resize(trigger + 1, (now, 0));
        }
        let (started, count) = &mut self.windows[trigger];
        if now.duration_since(*started) >= self.window {
            *started = now;
            *count = 0;
        }
        if *count >= self.max_per_window {
            return false;
        }
        *count += 1;
        true
    }
}

/// What to do with a trigger that just matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Run,
    /// Over its rate: skipped this time.
    Drop,
    /// An active trigger over its rate: probably matching its own effect
    /// (text it sends, echoed back) — disabled until `reset`.
    Disable,
}

/// `RateLimiter`, plus disabling the active triggers that exceed it: a
/// trigger typing text that matches itself would otherwise loop for as
/// long as the terminal is open, 5 times a second.
pub struct FiringGuard {
    limiter: RateLimiter,
    max_per_window: u32,
    window: Duration,
    disabled: HashSet<usize>,
}

impl FiringGuard {
    pub fn new(max_per_window: u32, window: Duration) -> Self {
        Self {
            limiter: RateLimiter::new(max_per_window, window),
            max_per_window,
            window,
            disabled: HashSet::new(),
        }
    }

    pub fn check(&mut self, trigger: usize, active: bool, now: Instant) -> Verdict {
        if self.disabled.contains(&trigger) {
            return Verdict::Drop;
        }
        if self.limiter.allow(trigger, now) {
            Verdict::Run
        } else if active {
            self.disabled.insert(trigger);
            Verdict::Disable
        } else {
            Verdict::Drop
        }
    }

    /// Re-enables every trigger, with fresh rate counts — the triggers
    /// changed, or the user asked.
    pub fn reset(&mut self) {
        self.limiter = RateLimiter::new(self.max_per_window, self.window);
        self.disabled.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terminal grid: each row is its text and whether it ends the line
    /// (`false`: wraps onto the next row), plus the cursor's row. Rows
    /// before `first_row` have left the scrollback.
    struct FakeScreen {
        rows: Vec<(String, bool)>,
        first_row: i64,
        cursor: i64,
    }

    impl FakeScreen {
        fn new() -> Self {
            Self {
                rows: vec![(String::new(), true)],
                first_row: 0,
                cursor: 0,
            }
        }

        /// Prints `text` like a terminal would: `\n` ends the line, `\r`
        /// goes back to the start of the row, `width` wraps.
        fn print(&mut self, text: &str, width: usize) {
            for c in text.chars() {
                let row = self.cursor as usize;
                match c {
                    '\n' => {
                        self.rows[row].1 = true;
                        self.cursor += 1;
                        if self.rows.len() <= self.cursor as usize {
                            self.rows.push((String::new(), true));
                        }
                    }
                    '\r' => self.rows[row].0.clear(),
                    c => {
                        if self.rows[row].0.chars().count() == width {
                            self.rows[row].1 = false;
                            self.cursor += 1;
                            self.rows.push((String::new(), true));
                        }
                        let row = self.cursor as usize;
                        self.rows[row].0.push(c);
                    }
                }
            }
        }

        /// Leaves the cursor on `row`, as cursor movement or switching
        /// screens would.
        fn move_cursor(&mut self, row: i64) {
            while self.rows.len() <= row as usize {
                self.rows.push((String::new(), true));
            }
            self.cursor = row;
        }
    }

    impl Screen for FakeScreen {
        fn cursor_row(&self) -> i64 {
            self.cursor
        }

        fn row_text(&self, row: i64) -> String {
            if row < self.first_row {
                return "\n".to_string();
            }
            let Some((text, ends)) = self.rows.get(row as usize) else {
                return "\n".to_string();
            };
            if *ends || row == self.cursor {
                format!("{text}\n")
            } else {
                text.clone()
            }
        }
    }

    fn trigger(pattern: &str) -> Trigger {
        Trigger {
            pattern: pattern.to_string(),
            action: TriggerAction::SendNotification,
            parameter: String::new(),
        }
    }

    /// The whole match of every match found by one scan.
    fn scan(scanner: &mut LineScanner, screen: &FakeScreen, patterns: &[&str]) -> Vec<String> {
        let triggers = compile(&patterns.iter().map(|p| trigger(p)).collect::<Vec<_>>());
        scanner
            .scan(screen, &triggers, Some(256))
            .into_iter()
            .map(|m| m.groups[0].clone())
            .collect()
    }

    #[test]
    fn a_line_printed_in_chunks_is_matched_whole_and_once() {
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        let p = ["Download complete"];
        screen.print("Downl", 80);
        assert!(scan(&mut scanner, &screen, &p).is_empty());
        screen.print("oad complete\nnext", 80);
        assert_eq!(scan(&mut scanner, &screen, &p), ["Download complete"]);
        screen.print(" line\n", 80);
        assert!(scan(&mut scanner, &screen, &p).is_empty());
    }

    #[test]
    fn a_prompt_without_line_break_fires_once() {
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        let p = [r"password for \w+: $"];
        screen.print("[sudo] password for paul: ", 80);
        assert_eq!(scan(&mut scanner, &screen, &p), ["password for paul: "]);
        // Nothing new: rescanning the same partial line doesn't refire.
        assert!(scan(&mut scanner, &screen, &p).is_empty());
        // Nor does it fire again once the line ends — though `$` no longer
        // matches there anyway; check with a pattern that still does.
        let p = ["password for"];
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        screen.print("password for paul: ", 80);
        assert_eq!(scan(&mut scanner, &screen, &p), ["password for"]);
        screen.print("\n", 80);
        assert!(scan(&mut scanner, &screen, &p).is_empty());
    }

    #[test]
    fn a_line_rewritten_with_carriage_returns_is_seen_in_each_state() {
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        let p = [r"\d+%"];
        screen.print("\r50%", 80);
        assert_eq!(scan(&mut scanner, &screen, &p), ["50%"]);
        screen.print("\r75%", 80);
        assert_eq!(scan(&mut scanner, &screen, &p), ["75%"]);
        screen.print("\r100%\n", 80);
        assert_eq!(scan(&mut scanner, &screen, &p), ["100%"]);
    }

    #[test]
    fn a_wrapped_line_is_joined_back() {
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        let line = format!("{}Download complete{}\n", "x".repeat(70), "x".repeat(70));
        screen.print(&line, 80);
        assert_eq!(
            scan(&mut scanner, &screen, &["Download complete"]),
            ["Download complete"]
        );
    }

    #[test]
    fn the_same_text_on_two_lines_fires_twice() {
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        screen.print("ok\nok\n", 80);
        assert_eq!(scan(&mut scanner, &screen, &["ok"]), ["ok", "ok"]);
        screen.print("ok ok\n", 80);
        assert_eq!(scan(&mut scanner, &screen, &["ok"]), ["ok", "ok"]);
    }

    #[test]
    fn matches_come_in_order_of_appearance_across_triggers() {
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        screen.print("b a\nc\n", 80);
        assert_eq!(scan(&mut scanner, &screen, &["a", "b|c"]), ["b", "a", "c"]);
    }

    #[test]
    fn rows_scanned_before_the_cursor_went_up_are_not_refired() {
        // Leaving the alternate screen: the cursor comes back below rows
        // already scanned on the normal screen, which get re-read.
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        let p = ["ERROR"];
        screen.print("ERROR one\nfine\nERROR two\n", 80);
        assert_eq!(scan(&mut scanner, &screen, &p), ["ERROR", "ERROR"]);
        screen.move_cursor(1);
        assert!(scan(&mut scanner, &screen, &p).is_empty());
        screen.move_cursor(3);
        assert!(scan(&mut scanner, &screen, &p).is_empty());
        screen.print("ERROR three\n", 80);
        assert_eq!(scan(&mut scanner, &screen, &p), ["ERROR"]);
    }

    #[test]
    fn at_most_max_lines_rows_are_read_and_a_cut_line_is_skipped() {
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        let triggers = compile(&[trigger("hit")]);
        // A wrapped line whose start falls outside the window, then a
        // complete line inside it.
        screen.print(&format!("hit{}\nhit\n", "x".repeat(200)), 10);
        let found = scanner.scan(&screen, &triggers, Some(3));
        assert_eq!(found.len(), 1, "only the line fully in the window");
        // Unlimited sees both.
        let mut scanner = LineScanner::default();
        assert_eq!(scanner.scan(&screen, &triggers, None).len(), 2);
    }

    #[test]
    fn a_huge_unfinished_line_never_fires_repeatedly() {
        // Longer than the window and still growing: its start is cut off
        // on every scan, which must not refire what's in view.
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        let triggers = compile(&[trigger("hit")]);
        // While it still fits in the window, its matches fire (once).
        screen.print(&"hit-".repeat(10), 10);
        assert_eq!(scanner.scan(&screen, &triggers, Some(3)).len(), 10);
        let mut total = 0;
        for _ in 0..20 {
            screen.print(&"hit-".repeat(10), 10);
            total += scanner.scan(&screen, &triggers, Some(3)).len();
        }
        assert_eq!(total, 0);
        // Nor when a change comes without the cursor moving.
        assert!(scanner.scan(&screen, &triggers, Some(3)).is_empty());
        screen.print("\nhit\n", 10);
        assert_eq!(scanner.scan(&screen, &triggers, Some(3)).len(), 1);
    }

    #[test]
    fn rows_gone_from_the_scrollback_read_as_empty() {
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        screen.print("hit\nhit\nhit\n", 80);
        screen.first_row = 2;
        assert_eq!(scan(&mut scanner, &screen, &["hit"]), ["hit"]);
    }

    #[test]
    fn groups_are_captured_and_substituted_in_one_pass() {
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        let triggers = compile(&[trigger(r"(\w+)@(\w+)(:)?(x)?")]);
        screen.print("paul@box\n", 80);
        let found = scanner.scan(&screen, &triggers, None);
        assert_eq!(found[0].groups, ["paul@box", "paul", "box", "", ""]);
        assert_eq!(
            substitute("$1 on $2, all: $0", &found[0].groups),
            "paul on box, all: paul@box"
        );
        // Captured text isn't expanded again, unknown groups and lone `$`
        // stay.
        let groups = vec!["$2".to_string(), "boom".to_string()];
        assert_eq!(
            substitute("$0 / $1 / $7 / $ / $x", &groups),
            "$2 / boom / $7 / $ / $x"
        );
    }

    #[test]
    fn invalid_patterns_are_reported_and_skipped() {
        assert!(pattern_error("ok (\\d+)").is_none());
        assert!(pattern_error("").is_some());
        assert!(pattern_error("(unclosed").is_some());
        // Lookaround isn't supported by the linear-time engine.
        assert!(pattern_error("foo(?=bar)").is_some());
        let compiled = compile(&[trigger("(bad"), trigger("good")]);
        assert_eq!(compiled.len(), 1);
    }

    #[test]
    fn a_pathological_pattern_stays_fast() {
        // Catastrophic for a backtracking engine (PCRE gives up after
        // 30-220 ms per line); linear here.
        let (mut screen, mut scanner) = (FakeScreen::new(), LineScanner::default());
        let triggers = compile(&[trigger(r"^(a+)+$")]);
        for _ in 0..256 {
            screen.print(&format!("{}!\n", "a".repeat(60)), 80);
        }
        let started = Instant::now();
        assert!(scanner.scan(&screen, &triggers, None).is_empty());
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn state_and_notification_parameters() {
        assert_eq!(
            StateUpdate::parse("username = paul ;hostname=box;directory=/srv/a=b;junk"),
            StateUpdate {
                username: Some("paul".into()),
                hostname: Some("box".into()),
                directory: Some("/srv/a=b".into()),
            }
        );
        assert_eq!(StateUpdate::parse("hostname="), StateUpdate::default());
        assert_eq!(notification("Build done"), (None, "Build done".into()));
        assert_eq!(
            notification("title=CI;body=failed"),
            (Some("CI".into()), "failed".into())
        );
        assert_eq!(notification("title=CI"), (Some("CI".into()), String::new()));
    }

    #[test]
    fn the_rate_limiter_caps_each_trigger_per_window() {
        let mut limiter = RateLimiter::new(2, Duration::from_secs(1));
        let t0 = Instant::now();
        assert!(limiter.allow(0, t0));
        assert!(limiter.allow(0, t0));
        assert!(!limiter.allow(0, t0 + Duration::from_millis(500)));
        assert!(limiter.allow(1, t0), "triggers are counted separately");
        assert!(limiter.allow(0, t0 + Duration::from_secs(1)));
    }

    #[test]
    fn dollar_dollar_is_a_literal_dollar() {
        let groups = vec!["all".to_string(), "one".to_string()];
        assert_eq!(substitute("$$1 is $1, $$$1", &groups), "$1 is one, $one");
    }

    fn groups(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_group_is_always_a_single_argument_and_never_reaches_a_shell() {
        let hostile = groups(&["x", "a; rm -rf ~ \"$(id)\"\nreboot"]);
        let argv = command_argv(r#"notify-send "Build: $1" $1"#, &hostile).unwrap();
        assert_eq!(
            argv,
            [
                "notify-send",
                r#"Build: a; rm -rf ~ "$(id)"reboot"#,
                r#"a; rm -rf ~ "$(id)"reboot"#,
            ],
            "quotes and spaces in the group don't split it, the line break is gone"
        );
        assert!(command_argv("", &hostile).is_err());
        assert!(command_argv("echo 'unclosed", &hostile).is_err());
    }

    #[test]
    fn sent_text_decodes_template_escapes_but_strips_controls_from_groups() {
        let hostile = groups(&["x", "ls\n\u{1b}[31mrm -rf ~\r"]);
        assert_eq!(send_text(r"echo $1\n", &hostile), "echo ls[31mrm -rf ~\n");
        assert_eq!(send_text(r"a\tb\\n\q", &[]), "a\tb\\n\\q");
        // A backslash from a group is data, not an escape.
        assert_eq!(send_text("$1", &groups(&["", r"\n"])), r"\n");
    }

    #[test]
    fn process_output_is_capped_and_stripped_of_escape_sequences() {
        assert_eq!(
            sanitize_process_output("secret\u{1b}[2J\r\n\tok", 100),
            "secret[2J\n\tok"
        );
        assert_eq!(
            sanitize_process_output("ééé", 3),
            "é",
            "cut on a char boundary"
        );
    }

    #[test]
    fn code_run_by_an_interpreter_is_flagged() {
        let warn = |p: &str| {
            matches!(
                parameter_issue(TriggerAction::ExecuteCommand, p),
                Some(ParameterIssue::Warning(_))
            )
        };
        assert!(warn(r#"sh -c "notify-send $1""#));
        assert!(warn(r#"/usr/bin/bash -lc 'echo $1'"#));
        assert!(warn(r#"env FOO=1 python3 -c "print('$1')""#));
        assert!(warn(r#"perl -e 'print "$0"'"#));
        // The safe forms.
        assert!(!warn(r#"sh -c 'notify-send "$$1"' sh $1"#));
        assert!(!warn(r#"notify-send "Build" $1"#));
        assert!(!warn(r#"sh -c 'date'"#));
        assert!(matches!(
            parameter_issue(TriggerAction::RunProcess, "pass 'show"),
            Some(ParameterIssue::Error(_))
        ));
        assert!(matches!(
            parameter_issue(TriggerAction::SendText, r"cd $1\n"),
            Some(ParameterIssue::Warning(_))
        ));
        assert!(parameter_issue(TriggerAction::SendText, r"yes\n").is_none());
        assert!(parameter_issue(TriggerAction::SendText, r"cost: $$5").is_none());
    }

    #[test]
    fn an_active_trigger_over_its_rate_is_disabled_a_passive_one_throttled() {
        let mut guard = FiringGuard::new(2, Duration::from_secs(1));
        let t0 = Instant::now();
        for trigger in [0, 1] {
            assert_eq!(guard.check(trigger, trigger == 1, t0), Verdict::Run);
            assert_eq!(guard.check(trigger, trigger == 1, t0), Verdict::Run);
        }
        assert_eq!(guard.check(0, false, t0), Verdict::Drop);
        assert_eq!(guard.check(1, true, t0), Verdict::Disable);
        let later = t0 + Duration::from_secs(5);
        assert_eq!(
            guard.check(0, false, later),
            Verdict::Run,
            "throttled, then back"
        );
        assert_eq!(
            guard.check(1, true, later),
            Verdict::Drop,
            "disabled stays off"
        );
        guard.reset();
        assert_eq!(guard.check(1, true, later), Verdict::Run);
    }

    #[test]
    fn triggers_round_trip_through_toml() {
        #[derive(Serialize, Deserialize)]
        struct File {
            triggers: Vec<Trigger>,
        }
        let file = File {
            triggers: vec![
                Trigger {
                    pattern: r"(\w+)@(\w+)".into(),
                    action: TriggerAction::UpdateState,
                    parameter: "username=$1;hostname=$2".into(),
                },
                Trigger {
                    pattern: "BEEP".into(),
                    action: TriggerAction::PlayBell,
                    parameter: String::new(),
                },
            ],
        };
        let text = toml::to_string(&file).unwrap();
        assert!(text.contains(r#"action = "update-state""#), "{text}");
        assert!(!text.contains("parameter = \"\""), "{text}");
        let back: File = toml::from_str(&text).unwrap();
        assert_eq!(back.triggers, file.triggers);
    }
}
