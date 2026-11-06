//! Command line options (Tilix's, see `docs/ROADMAP.md` Phase 3), parsed
//! with clap. GTK-free apart from `glib::shell_quote`, so it's testable
//! with plain `cargo test` like `layout/split_tree.rs`.
//!
//! The application runs with `HANDLES_COMMAND_LINE`: launching `rutile`
//! while it's already running hands the arguments — and the caller's
//! working directory — to the running instance (`app::command_line`), which
//! then opens a window, a session or a split depending on `--action` or the
//! "On new instance" preference.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::{ArgGroup, Parser};
use gtk4::glib;

use crate::preferences::NewInstanceMode;

#[derive(Debug, Parser)]
#[command(
    name = "rutile",
    version,
    about = "A tiling terminal emulator — Tilix rewritten in Rust/GTK4",
    group(ArgGroup::new("window_state").args(["maximize", "minimize", "full_screen"])),
    group(ArgGroup::new("target").args(["action", "new_window", "new_tab"])),
    group(ArgGroup::new("run").args(["command", "execute"])),
)]
pub struct Cli {
    /// Start the terminal in this directory (relative to where rutile is
    /// launched)
    #[arg(short = 'w', long, value_name = "DIR")]
    pub working_directory: Option<PathBuf>,

    /// Use this profile (id or name) instead of the default one
    #[arg(short = 'p', long, value_name = "PROFILE")]
    pub profile: Option<String>,

    /// Name the new session
    #[arg(short = 't', long, value_name = "TITLE")]
    pub title: Option<String>,

    /// Open a saved session file (.rutile-session.toml)
    #[arg(short = 's', long, value_name = "FILE")]
    pub session: Option<PathBuf>,

    /// Run this command line instead of the profile's shell or command
    #[arg(short = 'e', long, value_name = "COMMAND")]
    pub command: Option<String>,

    /// Run the rest of the command line instead of the shell (must come
    /// last)
    #[arg(
        short = 'x',
        long,
        value_name = "ARGS",
        num_args = 1..,
        trailing_var_arg = true,
        allow_hyphen_values = true
    )]
    pub execute: Vec<String>,

    /// What to do when rutile is already running (default: the "On new
    /// instance" preference)
    #[arg(short = 'a', long, value_enum, value_name = "ACTION")]
    pub action: Option<NewInstanceMode>,

    /// Same as --action new-window
    #[arg(long)]
    pub new_window: bool,

    /// Same as --action new-session
    #[arg(long)]
    pub new_tab: bool,

    /// Maximize the window
    #[arg(long)]
    pub maximize: bool,

    /// Minimize the window
    #[arg(long)]
    pub minimize: bool,

    /// Show the window full screen
    #[arg(long)]
    pub full_screen: bool,

    /// Window size in terminal cells, COLUMNSxROWS (a +X+Y position is
    /// ignored: Wayland doesn't let applications place their windows)
    #[arg(long, value_name = "GEOMETRY", value_parser = parse_geometry)]
    pub geometry: Option<Geometry>,

    /// Open the Preferences window
    #[arg(long)]
    pub preferences: bool,

    /// Run as a separate process instead of handing over to the running
    /// instance
    #[arg(long)]
    pub new_process: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub columns: u32,
    pub rows: u32,
    /// A `+X+Y` suffix was given — accepted for Tilix compatibility, but
    /// only reported back as ignored.
    pub has_position: bool,
}

/// `COLUMNSxROWS[+X+Y]`, like X11's `-geometry`.
fn parse_geometry(value: &str) -> Result<Geometry, String> {
    let (size, position) = match value.find(['+', '-']) {
        Some(index) => (&value[..index], Some(&value[index..])),
        None => (value, None),
    };
    let (columns, rows) = size
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("expected COLUMNSxROWS, got {value:?}"))?;
    let parse = |part: &str| {
        part.parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("invalid size in {value:?}"))
    };
    if let Some(position) = position {
        let valid = position
            .split(['+', '-'])
            .skip(1)
            .all(|n| n.parse::<i32>().is_ok());
        if !valid || position.matches(['+', '-']).count() != 2 {
            return Err(format!("invalid position in {value:?}"));
        }
    }
    Ok(Geometry {
        columns: parse(columns)?,
        rows: parse(rows)?,
        has_position: position.is_some(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    Maximized,
    Minimized,
    FullScreen,
}

/// A launch, resolved against the invoking process: relative paths made
/// absolute against *its* working directory (which, for a launch handed
/// over to an already running instance, isn't the running instance's).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchRequest {
    /// Where `rutile` was launched from — the new terminal's directory when
    /// neither `-w` nor the profile's own working directory says otherwise.
    pub invocation_dir: Option<PathBuf>,
    pub working_directory: Option<PathBuf>,
    pub profile: Option<String>,
    pub title: Option<String>,
    pub session_file: Option<PathBuf>,
    /// A shell command line (`-e` as given, `-x` arguments shell-quoted).
    pub command: Option<String>,
    pub action: Option<NewInstanceMode>,
    pub window_state: Option<WindowState>,
    pub geometry: Option<Geometry>,
    pub preferences: bool,
    /// Accepted but ignored options, to report back to the caller.
    pub warnings: Vec<String>,
}

impl LaunchRequest {
    /// Parses `args` (including the program name, as `argv`). `Err` is
    /// clap's own error, which also covers `--help`/`--version` — the
    /// caller prints it and exits with its `exit_code()`.
    pub fn parse(args: &[OsString], cwd: Option<&Path>) -> Result<Self, clap::Error> {
        Ok(Cli::try_parse_from(args)?.resolve(cwd))
    }

    /// Whether this launch says anything about the session to open — if
    /// not, a first window may restore the last session instead.
    pub fn shapes_session(&self) -> bool {
        self.working_directory.is_some()
            || self.profile.is_some()
            || self.title.is_some()
            || self.session_file.is_some()
            || self.command.is_some()
    }
}

impl Cli {
    fn resolve(self, cwd: Option<&Path>) -> LaunchRequest {
        let absolute = |path: PathBuf| match cwd {
            Some(cwd) if path.is_relative() => cwd.join(path),
            _ => path,
        };
        let command = self.command.or_else(|| {
            (!self.execute.is_empty()).then(|| {
                self.execute
                    .iter()
                    .map(|arg| glib::shell_quote(arg).to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        });
        let action = if self.new_window {
            Some(NewInstanceMode::NewWindow)
        } else if self.new_tab {
            Some(NewInstanceMode::NewSession)
        } else {
            self.action
        };
        let window_state = if self.maximize {
            Some(WindowState::Maximized)
        } else if self.minimize {
            Some(WindowState::Minimized)
        } else if self.full_screen {
            Some(WindowState::FullScreen)
        } else {
            None
        };
        let mut warnings = Vec::new();
        if self.geometry.is_some_and(|g| g.has_position) {
            warnings.push(
                "rutile: --geometry position ignored, Wayland doesn't let applications place their windows"
                    .to_string(),
            );
        }
        LaunchRequest {
            invocation_dir: cwd.map(Path::to_path_buf),
            working_directory: self.working_directory.map(&absolute),
            profile: self.profile,
            title: self.title,
            session_file: self.session.map(&absolute),
            command,
            action,
            window_state,
            geometry: self.geometry,
            preferences: self.preferences,
            warnings,
        }
    }
}

/// Whether `--new-process` is among the raw arguments — checked before
/// the application starts, since it decides whether to hand over to a
/// running instance at all (`gio::ApplicationFlags::NON_UNIQUE`).
pub fn wants_new_process(args: impl IntoIterator<Item = OsString>) -> bool {
    args.into_iter()
        .skip(1)
        .take_while(|arg| arg != "-x" && arg != "--execute" && arg != "--")
        .any(|arg| arg == "--new-process")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<LaunchRequest, clap::Error> {
        let args: Vec<OsString> = std::iter::once("rutile")
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect();
        LaunchRequest::parse(&args, Some(Path::new("/home/u/src")))
    }

    #[test]
    fn no_arguments_is_a_plain_launch() {
        let request = parse(&[]).unwrap();
        assert!(!request.shapes_session());
        assert_eq!(
            request.invocation_dir.as_deref(),
            Some(Path::new("/home/u/src"))
        );
        assert_eq!(request.action, None);
    }

    #[test]
    fn relative_paths_resolve_against_the_invoking_directory() {
        let request = parse(&["-w", "rutile", "-s", "/tmp/a.rutile-session.toml"]).unwrap();
        assert_eq!(
            request.working_directory.as_deref(),
            Some(Path::new("/home/u/src/rutile"))
        );
        assert_eq!(
            request.session_file.as_deref(),
            Some(Path::new("/tmp/a.rutile-session.toml"))
        );
        assert!(request.shapes_session());
    }

    #[test]
    fn execute_takes_the_rest_of_the_line_shell_quoted() {
        let request = parse(&["-p", "dev", "-x", "nvim", "-c", "set nu", "my file"]).unwrap();
        assert_eq!(request.profile.as_deref(), Some("dev"));
        // What matters is the round trip through the shell parsing the
        // spawn does (`profile::spawn::command_for`), not the quoting style.
        let argv: Vec<String> = glib::shell_parse_argv(request.command.unwrap().as_str())
            .unwrap()
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(argv, ["nvim", "-c", "set nu", "my file"]);
    }

    #[test]
    fn command_and_execute_conflict() {
        assert!(parse(&["-e", "htop", "-x", "top"]).is_err());
    }

    #[test]
    fn action_shortcuts_and_conflicts() {
        assert_eq!(
            parse(&["--new-tab"]).unwrap().action,
            Some(NewInstanceMode::NewSession)
        );
        assert_eq!(
            parse(&["-a", "split-down"]).unwrap().action,
            Some(NewInstanceMode::SplitDown)
        );
        assert!(parse(&["--new-window", "--new-tab"]).is_err());
        assert!(parse(&["--maximize", "--full-screen"]).is_err());
    }

    #[test]
    fn geometry_accepts_an_ignored_position() {
        let request = parse(&["--geometry", "120x40"]).unwrap();
        assert_eq!(
            request.geometry,
            Some(Geometry {
                columns: 120,
                rows: 40,
                has_position: false
            })
        );
        assert!(request.warnings.is_empty());

        let request = parse(&["--geometry", "80x24+10-20"]).unwrap();
        assert!(request.geometry.unwrap().has_position);
        assert_eq!(request.warnings.len(), 1);

        for bad in ["80", "0x24", "80x", "axb", "80x24+10", "80x24+a+b"] {
            assert!(
                parse(&["--geometry", bad]).is_err(),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn help_and_version_are_clap_errors_with_a_success_exit_code() {
        let help = parse(&["--help"]).unwrap_err();
        assert_eq!(help.exit_code(), 0);
        assert!(help.to_string().contains("--working-directory"));
        assert_eq!(parse(&["--version"]).unwrap_err().exit_code(), 0);
        assert_ne!(parse(&["--bogus"]).unwrap_err().exit_code(), 0);
    }

    #[test]
    fn new_process_is_spotted_before_parsing_but_not_inside_execute() {
        let args = |list: &[&str]| -> Vec<OsString> {
            std::iter::once("rutile")
                .chain(list.iter().copied())
                .map(OsString::from)
                .collect()
        };
        assert!(wants_new_process(args(&["--new-process", "-w", "/tmp"])));
        assert!(!wants_new_process(args(&["-w", "/tmp"])));
        assert!(!wants_new_process(args(&["-x", "foo", "--new-process"])));
    }
}
