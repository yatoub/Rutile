use std::path::Path;

use gtk4::glib;

use super::Profile;
use crate::i18n::{tr, tr_args};

/// What to hand `vte4::Terminal::spawn_async` for a profile, resolved
/// without touching any widget — kept apart from `terminal::widget` so the
/// argv rules (custom command vs shell, login shell) are unit-testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnCommand {
    pub argv: Vec<String>,
    /// Look `argv[0]` up in `$PATH` (`glib::SpawnFlags::SEARCH_PATH`) —
    /// needed for a custom command like `htop`, not for `$SHELL`, which is
    /// already an absolute path.
    pub search_path: bool,
    /// `argv[0]` is the file to run and `argv[1..]` the real argument
    /// vector, starting with the `argv[0]` the child sees
    /// (`glib::SpawnFlags::FILE_AND_ARGV_ZERO`). How a login shell is
    /// started: running `/bin/zsh` with `argv[0] = "-zsh"`.
    pub argv_zero: bool,
}

/// The command a new terminal runs for `profile`. `shell` is the user's
/// shell (`$SHELL`), used unless the profile sets a non-blank custom
/// command. `Err` carries `glib::shell_parse_argv`'s message for a custom
/// command that doesn't parse (an unbalanced quote, typically).
pub fn command_for(profile: &Profile, shell: &str) -> Result<SpawnCommand, String> {
    if profile.use_custom_command && !profile.custom_command.trim().is_empty() {
        let argv = glib::shell_parse_argv(profile.custom_command.as_str())
            .map_err(|err| err.message().to_string())?;
        return Ok(SpawnCommand {
            argv: argv
                .into_iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
            search_path: true,
            argv_zero: false,
        });
    }

    if profile.login_shell {
        let name = Path::new(shell)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| shell.to_string());
        return Ok(SpawnCommand {
            argv: vec![shell.to_string(), format!("-{name}")],
            search_path: false,
            argv_zero: true,
        });
    }

    Ok(SpawnCommand {
        argv: vec![shell.to_string()],
        search_path: false,
        argv_zero: false,
    })
}

/// The directory a new terminal starts in: an explicit one (a restored
/// session's saved cwd) wins over the profile's `working_directory`, and a
/// leading `~` in the latter is expanded against `home`. `None` leaves it
/// to vte (Rutile's own working directory).
pub fn working_directory_for(
    profile: &Profile,
    explicit: Option<&str>,
    home: &Path,
) -> Option<String> {
    if let Some(explicit) = explicit {
        return Some(explicit.to_string());
    }
    let configured = profile.working_directory.as_deref()?.trim();
    if configured.is_empty() {
        return None;
    }
    let expanded = if configured == "~" {
        home.to_path_buf()
    } else if let Some(rest) = configured.strip_prefix("~/") {
        home.join(rest)
    } else {
        Path::new(configured).to_path_buf()
    };
    Some(expanded.to_string_lossy().into_owned())
}

/// Human-readable form of the raw `waitpid` status vte's `child-exited`
/// signal reports — shown in the "Hold the terminal open" banner, worded
/// like Tilix's own info bar.
pub fn describe_exit(status: i32) -> String {
    if libc::WIFEXITED(status) {
        tr_args(
            "The command exited with status {status}",
            &[("status", &libc::WEXITSTATUS(status))],
        )
    } else if libc::WIFSIGNALED(status) {
        tr_args(
            "The command was terminated by signal {signal}",
            &[("signal", &libc::WTERMSIG(status))],
        )
    } else {
        tr("The command was terminated").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::ExitAction;

    fn profile() -> Profile {
        Profile::new("test", "Test", "catppuccin-mocha")
    }

    #[test]
    fn default_profile_runs_the_shell() {
        let command = command_for(&profile(), "/bin/zsh").unwrap();
        assert_eq!(command.argv, ["/bin/zsh"]);
        assert!(!command.search_path);
        assert!(!command.argv_zero);
    }

    #[test]
    fn login_shell_prefixes_argv_zero_with_a_dash() {
        let mut profile = profile();
        profile.login_shell = true;
        let command = command_for(&profile, "/usr/bin/fish").unwrap();
        assert_eq!(command.argv, ["/usr/bin/fish", "-fish"]);
        assert!(command.argv_zero);
    }

    #[test]
    fn custom_command_is_shell_parsed_and_searched_in_path() {
        let mut profile = profile();
        profile.use_custom_command = true;
        profile.login_shell = true;
        profile.custom_command = r#"ssh -t host "tmux attach""#.to_string();
        let command = command_for(&profile, "/bin/zsh").unwrap();
        assert_eq!(command.argv, ["ssh", "-t", "host", "tmux attach"]);
        assert!(command.search_path);
        // Login shell only applies to the shell, not to a custom command.
        assert!(!command.argv_zero);
    }

    #[test]
    fn disabled_or_blank_custom_command_falls_back_to_the_shell() {
        let mut profile = profile();
        profile.custom_command = "htop".to_string();
        assert_eq!(command_for(&profile, "/bin/sh").unwrap().argv, ["/bin/sh"]);

        profile.use_custom_command = true;
        profile.custom_command = "   ".to_string();
        assert_eq!(command_for(&profile, "/bin/sh").unwrap().argv, ["/bin/sh"]);
    }

    #[test]
    fn unparsable_custom_command_is_an_error() {
        let mut profile = profile();
        profile.use_custom_command = true;
        profile.custom_command = "echo 'unterminated".to_string();
        assert!(command_for(&profile, "/bin/sh").is_err());
    }

    #[test]
    fn explicit_working_directory_wins_over_the_profile() {
        let mut profile = profile();
        profile.working_directory = Some("/srv".to_string());
        let home = Path::new("/home/u");
        assert_eq!(
            working_directory_for(&profile, Some("/tmp"), home).as_deref(),
            Some("/tmp")
        );
        assert_eq!(
            working_directory_for(&profile, None, home).as_deref(),
            Some("/srv")
        );
    }

    #[test]
    fn profile_working_directory_expands_tilde() {
        let mut profile = profile();
        let home = Path::new("/home/u");
        profile.working_directory = Some("~".to_string());
        assert_eq!(
            working_directory_for(&profile, None, home).as_deref(),
            Some("/home/u")
        );
        profile.working_directory = Some("~/projets".to_string());
        assert_eq!(
            working_directory_for(&profile, None, home).as_deref(),
            Some("/home/u/projets")
        );
        profile.working_directory = Some("  ".to_string());
        assert_eq!(working_directory_for(&profile, None, home), None);
    }

    #[test]
    fn describe_exit_decodes_waitpid_status() {
        // `waitpid` encoding: exit code in bits 8-15, signal in bits 0-6.
        assert_eq!(describe_exit(0), "The command exited with status 0");
        assert_eq!(describe_exit(2 << 8), "The command exited with status 2");
        assert_eq!(describe_exit(9), "The command was terminated by signal 9");
    }

    #[test]
    fn exit_action_round_trips_as_lowercase_toml() {
        let mut profile = profile();
        profile.exit_action = ExitAction::Hold;
        let toml = toml::to_string(&profile).unwrap();
        assert!(toml.contains(r#"exit_action = "hold""#));
        let back: Profile = toml::from_str(&toml).unwrap();
        assert_eq!(back.exit_action, ExitAction::Hold);
    }

    #[test]
    fn profiles_saved_before_these_settings_still_load() {
        let old = r#"
            id = "default"
            name = "Default"
            scheme_id = "catppuccin-mocha"
        "#;
        let profile: Profile = toml::from_str(old).unwrap();
        assert!(!profile.use_custom_command);
        assert!(!profile.login_shell);
        assert_eq!(profile.working_directory, None);
        assert_eq!(profile.exit_action, ExitAction::Close);
    }
}
