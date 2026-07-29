//! GTK-free title-template rendering for pane headers (`pane_header.rs`).
//! Kept separate from GTK wiring so it's testable with plain `cargo test`,
//! same rationale as `layout/split_tree.rs`/`terminal/broadcast.rs`.

/// Tilix's default profile title format is just the raw window title VTE
/// picked up via OSC 0/2 — Rutile matches that as the starting point,
/// falling back to "Terminal" (via `render_template`) when the shell
/// hasn't reported one yet.
pub const DEFAULT_TEMPLATE: &str = "${title}";

/// The pieces a title template can reference. All `Option` since a shell
/// may not have reported a title/cwd yet (no OSC 0/2/7 support), and
/// host/user lookups can fail (see `local_hostname`/`current_user`).
pub struct TitleContext<'a> {
    pub id: u64,
    pub title: Option<&'a str>,
    pub directory: Option<&'a str>,
    pub host: Option<&'a str>,
    pub user: Option<&'a str>,
}

/// Substitutes `${title}`/`${id}`/`${directory}`/`${host}`/`${user}` in
/// `template` with values from `ctx`. Plain literal substitution (no
/// escaping/nesting) — matches the scope of Tilix's own title format
/// strings.
pub fn render_template(template: &str, ctx: &TitleContext) -> String {
    let title = ctx.title.filter(|t| !t.is_empty()).unwrap_or("Terminal");
    template
        .replace("${title}", title)
        .replace("${id}", &ctx.id.to_string())
        .replace("${directory}", ctx.directory.unwrap_or(""))
        .replace("${host}", ctx.host.unwrap_or(""))
        .replace("${user}", ctx.user.unwrap_or(""))
}

/// Local hostname for `${host}`. Reads `/proc/sys/kernel/hostname`
/// directly (Linux-only, same tradeoff already made project-wide — see
/// `docs/ROADMAP.md` Phase 4 notification polling) rather than pull in a
/// `hostname`/`libc` dependency for one string.
pub fn local_hostname() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Local username for `${user}`.
pub fn current_user() -> Option<String> {
    std::env::var("USER").ok().filter(|s| !s.is_empty())
}

/// Extracts a filesystem path from the `file://[host]/path` URI VTE
/// reports via `current_directory_uri()` (OSC 7). Percent-decodes the
/// path portion; drops the URI entirely if it isn't `file://`.
pub fn directory_from_uri(uri: &str) -> Option<String> {
    let after_scheme = uri.strip_prefix("file://")?;
    let path_start = after_scheme.find('/')?;
    Some(percent_decode(&after_scheme[path_start..]))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&input[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_every_placeholder() {
        let ctx = TitleContext {
            id: 7,
            title: Some("vim"),
            directory: Some("/home/paul"),
            host: Some("workstation"),
            user: Some("paul"),
        };
        let rendered = render_template("${user}@${host}:${directory} [${id}] ${title}", &ctx);
        assert_eq!(rendered, "paul@workstation:/home/paul [7] vim");
    }

    #[test]
    fn missing_fields_render_as_empty_except_title_and_id() {
        let ctx = TitleContext {
            id: 3,
            title: None,
            directory: None,
            host: None,
            user: None,
        };
        let rendered = render_template("${title}(${id})[${directory}|${host}|${user}]", &ctx);
        assert_eq!(rendered, "Terminal(3)[||]");
    }

    #[test]
    fn empty_title_falls_back_like_missing_title() {
        let ctx = TitleContext {
            id: 1,
            title: Some(""),
            directory: None,
            host: None,
            user: None,
        };
        assert_eq!(render_template(DEFAULT_TEMPLATE, &ctx), "Terminal");
    }

    #[test]
    fn directory_from_uri_decodes_percent_escapes() {
        assert_eq!(
            directory_from_uri("file:///home/paul/My%20Docs"),
            Some("/home/paul/My Docs".to_string())
        );
    }

    #[test]
    fn directory_from_uri_handles_remote_host_authority() {
        assert_eq!(
            directory_from_uri("file://remote-box/var/www"),
            Some("/var/www".to_string())
        );
    }

    #[test]
    fn directory_from_uri_rejects_non_file_scheme() {
        assert_eq!(directory_from_uri("https://example.com/path"), None);
    }
}
