//! Tilix's automatic profile switching, GTK-free so it's testable with
//! plain `cargo test`: a profile lists `user@host:directory` rules, and a
//! terminal whose current state matches one switches to that profile.

use std::path::{Component, Path, PathBuf};

use super::Profile;

/// Where a terminal currently is. `host` comes from the OSC 7 URI the shell
/// reports (the local hostname when the URI has none); `user` is the local
/// user — Tilix only learns a remote user through triggers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalState {
    pub user: Option<String>,
    pub host: Option<String>,
    pub directory: String,
}

/// One parsed rule. `None` means "any", like an empty part in Tilix.
#[derive(Debug, PartialEq, Eq)]
struct Rule {
    user: Option<String>,
    host: Option<String>,
    directory: Option<PathBuf>,
}

/// Splits `user@host:directory` like Tilix's `parsePromptParts`: `@` (if
/// not first) ends the user, the first `:` starts the directory, and
/// whatever is in between is the host. So `host`, `user@host`, `host:/dir`
/// and `:/dir` are all valid. A directory starting with `~` is expanded
/// against `home`.
fn parse_rule(rule: &str, home: &Path) -> Rule {
    let rule = rule.trim();
    let (head, directory) = match rule.find(':') {
        Some(colon) => (&rule[..colon], Some(&rule[colon + 1..])),
        None => (rule, None),
    };
    let (user, host) = match head.find('@') {
        Some(at) if at > 0 => (Some(&head[..at]), &head[at + 1..]),
        Some(at) => (None, &head[at + 1..]),
        None => (None, head),
    };
    let non_empty = |part: &str| (!part.is_empty()).then(|| part.to_string());
    let directory = directory.filter(|d| !d.is_empty()).map(|d| {
        if d == "~" {
            home.to_path_buf()
        } else if let Some(rest) = d.strip_prefix("~/") {
            home.join(rest)
        } else {
            PathBuf::from(d)
        }
    });
    Rule {
        user: user.and_then(non_empty),
        host: non_empty(host),
        directory,
    }
}

/// `directory` is `parent` or somewhere below it, compared component by
/// component (so `/home/u/project` doesn't match `/home/u/proj`).
fn is_within(directory: &Path, parent: &Path) -> bool {
    let (directory, parent) = (components(directory), components(parent));
    parent.len() <= directory.len() && directory[..parent.len()] == parent[..]
}

fn components(path: &Path) -> Vec<Component<'_>> {
    path.components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect()
}

fn matches(rule: &Rule, state: &TerminalState) -> bool {
    let part_matches = |wanted: &Option<String>, actual: &Option<String>| {
        wanted
            .as_ref()
            .is_none_or(|wanted| Some(wanted) == actual.as_ref())
    };
    // A rule with nothing in it would match everything — Tilix doesn't
    // treat that as a rule at all.
    let is_empty = rule.user.is_none() && rule.host.is_none() && rule.directory.is_none();
    !is_empty
        && part_matches(&rule.user, &state.user)
        && part_matches(&rule.host, &state.host)
        && rule
            .directory
            .as_ref()
            .is_none_or(|parent| is_within(Path::new(&state.directory), parent))
}

/// The first profile (in store order, like Tilix) with a rule matching
/// `state`, if any.
pub fn find_profile<'a>(
    profiles: impl IntoIterator<Item = &'a Profile>,
    state: &TerminalState,
    home: &Path,
) -> Option<&'a Profile> {
    profiles.into_iter().find(|profile| {
        profile
            .automatic_switch
            .iter()
            .any(|rule| matches(&parse_rule(rule, home), state))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/u";

    fn state(user: &str, host: &str, directory: &str) -> TerminalState {
        TerminalState {
            user: Some(user.to_string()),
            host: Some(host.to_string()),
            directory: directory.to_string(),
        }
    }

    fn profile(id: &str, rules: &[&str]) -> Profile {
        let mut profile = Profile::new(id, id, "catppuccin-mocha");
        profile.automatic_switch = rules.iter().map(|r| r.to_string()).collect();
        profile
    }

    #[test]
    fn parses_every_rule_shape_like_tilix() {
        let home = Path::new(HOME);
        assert_eq!(
            parse_rule("gnunn@macbook:/home/gnunn", home),
            Rule {
                user: Some("gnunn".into()),
                host: Some("macbook".into()),
                directory: Some("/home/gnunn".into()),
            }
        );
        assert_eq!(
            parse_rule("macbook", home),
            Rule {
                user: None,
                host: Some("macbook".into()),
                directory: None,
            }
        );
        assert_eq!(
            parse_rule(":~/projets", home),
            Rule {
                user: None,
                host: None,
                directory: Some("/home/u/projets".into()),
            }
        );
        assert_eq!(
            parse_rule("root@:/etc", home),
            Rule {
                user: Some("root".into()),
                host: None,
                directory: Some("/etc".into()),
            }
        );
    }

    #[test]
    fn directory_rule_matches_subdirectories_by_component() {
        let profiles = [profile("work", &[":/home/u/project"])];
        let home = Path::new(HOME);
        let found = |dir| find_profile(&profiles, &state("u", "box", dir), home).map(|p| &p.id);
        assert_eq!(found("/home/u/project"), Some(&"work".to_string()));
        assert_eq!(found("/home/u/project/src"), Some(&"work".to_string()));
        assert_eq!(found("/home/u/proj"), None);
        assert_eq!(found("/home/u/projects"), None);
    }

    #[test]
    fn every_given_part_must_match() {
        let profiles = [profile("prod", &["root@prod-db:/var"])];
        let home = Path::new(HOME);
        assert!(find_profile(&profiles, &state("root", "prod-db", "/var/log"), home).is_some());
        assert!(find_profile(&profiles, &state("u", "prod-db", "/var/log"), home).is_none());
        assert!(find_profile(&profiles, &state("root", "dev-db", "/var/log"), home).is_none());
    }

    #[test]
    fn first_matching_profile_wins_and_empty_rules_never_match() {
        let profiles = [
            profile("empty", &["", "  ", "@:"]),
            profile("first", &["box"]),
            profile("second", &["box"]),
        ];
        let found = find_profile(&profiles, &state("u", "box", "/"), Path::new(HOME));
        assert_eq!(found.map(|p| p.id.as_str()), Some("first"));
    }
}
