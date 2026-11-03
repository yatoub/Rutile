use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub type ProfileId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub scheme_id: String,
    /// Seconds of no terminal output required, once a burst of CPU
    /// activity subsides, before `terminal::monitor` fires its "command
    /// finished" notification for a pane using this profile. `#[serde(default
    /// = ...)]` (rather than a container-level `#[serde(default)]` like
    /// `Preferences` has) since `Profile` itself has no meaningful
    /// `Default` impl — it's always constructed with an explicit
    /// id/name/scheme_id.
    #[serde(default = "default_silence_seconds")]
    pub silence_seconds: u32,
    /// Run `custom_command` instead of the user's shell. A separate flag
    /// (rather than "non-empty `custom_command` means use it") so turning
    /// it off in Preferences doesn't throw away what was typed — same
    /// split as Tilix's `use-custom-command`/`custom-command` keys.
    #[serde(default)]
    pub use_custom_command: bool,
    /// Parsed with shell quoting rules (`glib::shell_parse_argv`), looked
    /// up in `$PATH` — see `profile::spawn::command_for`.
    #[serde(default)]
    pub custom_command: String,
    /// Start the shell as a login shell (`argv[0]` prefixed with `-`).
    /// Ignored when `use_custom_command` is set, like Tilix.
    #[serde(default)]
    pub login_shell: bool,
    /// Where a brand new terminal starts when nothing more specific
    /// applies (a restored session's saved cwd always wins). `None` keeps
    /// vte's default — the directory Rutile itself was launched from.
    #[serde(default)]
    pub working_directory: Option<String>,
    /// What happens to the pane once its command exits.
    #[serde(default)]
    pub exit_action: ExitAction,
    /// Font, cursor, scrolling and bell settings. Flattened so they sit
    /// next to the other keys in `profiles.toml` rather than in a nested
    /// table.
    #[serde(flatten)]
    pub options: TerminalOptions,
}

impl Profile {
    /// A profile with every optional setting at its default.
    pub fn new(id: &str, name: &str, scheme_id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            scheme_id: scheme_id.to_string(),
            silence_seconds: default_silence_seconds(),
            use_custom_command: false,
            custom_command: String::new(),
            login_shell: false,
            working_directory: None,
            exit_action: ExitAction::default(),
            options: TerminalOptions::default(),
        }
    }
}

fn default_silence_seconds() -> u32 {
    10
}

/// How a terminal looks and behaves, as opposed to what it runs. Every
/// default matches Tilix's own profile defaults (its GSettings schema),
/// not vte's — e.g. 8192 scrollback lines instead of vte's 512.
/// Container-level `#[serde(default)]` so a `profiles.toml` written before
/// any of these existed still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalOptions {
    /// Use the desktop's monospace font (GNOME's
    /// `org.gnome.desktop.interface monospace-font-name`) instead of
    /// `font`.
    pub use_system_font: bool,
    /// A Pango font description, e.g. `"Monospace 12"`.
    pub font: String,
    /// Extra spacing between cells, as a multiplier of the font's own
    /// cell size — vte accepts 1.0 to 2.0.
    pub cell_width_scale: f64,
    pub cell_height_scale: f64,
    pub text_blink: TextBlink,
    pub cursor_shape: CursorShape,
    pub cursor_blink: CursorBlink,
    /// Characters that count as part of a word on double-click selection,
    /// on top of alphanumerics.
    pub word_chars: String,
    pub show_scrollbar: bool,
    pub scroll_on_output: bool,
    pub scroll_on_keystroke: bool,
    pub scrollback_unlimited: bool,
    pub scrollback_lines: u32,
    pub bell: BellMode,
}

impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            use_system_font: true,
            font: "Monospace 12".to_string(),
            cell_width_scale: 1.0,
            cell_height_scale: 1.0,
            text_blink: TextBlink::default(),
            cursor_shape: CursorShape::default(),
            cursor_blink: CursorBlink::default(),
            word_chars: "-,./?%&#:_".to_string(),
            show_scrollbar: true,
            scroll_on_output: false,
            scroll_on_keystroke: true,
            scrollback_unlimited: false,
            scrollback_lines: 8192,
            bell: BellMode::default(),
        }
    }
}

/// When text with the SGR "blink" attribute actually blinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextBlink {
    Never,
    Focused,
    Unfocused,
    #[default]
    Always,
}

impl TextBlink {
    pub const ALL: [TextBlink; 4] = [
        TextBlink::Never,
        TextBlink::Focused,
        TextBlink::Unfocused,
        TextBlink::Always,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TextBlink::Never => "Never",
            TextBlink::Focused => "When focused",
            TextBlink::Unfocused => "When unfocused",
            TextBlink::Always => "Always",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorShape {
    #[default]
    Block,
    Ibeam,
    Underline,
}

impl CursorShape {
    pub const ALL: [CursorShape; 3] = [
        CursorShape::Block,
        CursorShape::Ibeam,
        CursorShape::Underline,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CursorShape::Block => "Block",
            CursorShape::Ibeam => "I-beam",
            CursorShape::Underline => "Underline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorBlink {
    /// Follow the desktop's cursor-blink setting.
    #[default]
    System,
    On,
    Off,
}

impl CursorBlink {
    pub const ALL: [CursorBlink; 3] = [CursorBlink::System, CursorBlink::On, CursorBlink::Off];

    pub fn label(self) -> &'static str {
        match self {
            CursorBlink::System => "Follow system",
            CursorBlink::On => "On",
            CursorBlink::Off => "Off",
        }
    }
}

/// Tilix's "Terminal bell" setting: an audible beep, a bell icon in the
/// pane header for a few seconds, both, or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BellMode {
    None,
    #[default]
    Sound,
    Icon,
    IconSound,
}

impl BellMode {
    pub const ALL: [BellMode; 4] = [
        BellMode::None,
        BellMode::Sound,
        BellMode::Icon,
        BellMode::IconSound,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BellMode::None => "None",
            BellMode::Sound => "Sound",
            BellMode::Icon => "Icon",
            BellMode::IconSound => "Icon and sound",
        }
    }

    pub fn plays_sound(self) -> bool {
        matches!(self, BellMode::Sound | BellMode::IconSound)
    }

    pub fn shows_icon(self) -> bool {
        matches!(self, BellMode::Icon | BellMode::IconSound)
    }
}

/// Tilix's "When command exits" setting, same three values (and same
/// lowercase spelling in the TOML file as Tilix's GSettings enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExitAction {
    /// Close the pane — Rutile's behavior before this setting existed.
    #[default]
    Close,
    /// Spawn the command again in the same pane.
    Restart,
    /// Keep the pane and its output, with a banner offering to relaunch.
    Hold,
}

impl ExitAction {
    pub const ALL: [ExitAction; 3] = [ExitAction::Close, ExitAction::Restart, ExitAction::Hold];

    pub fn label(self) -> &'static str {
        match self {
            ExitAction::Close => "Close the terminal",
            ExitAction::Restart => "Restart the command",
            ExitAction::Hold => "Hold the terminal open",
        }
    }
}

/// Every known profile, persisted as TOML in
/// `$XDG_CONFIG_HOME/rutile/profiles.toml`. Always has at least one profile
/// — `load()` seeds a "Default" profile on first run, and `delete()`
/// refuses to remove the last one, so callers never have to handle an
/// empty store.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileStore {
    profile: Vec<Profile>,
}

fn profiles_path() -> PathBuf {
    let mut path = gtk4::glib::user_config_dir();
    path.push("rutile");
    path.push("profiles.toml");
    path
}

impl ProfileStore {
    pub fn load() -> Self {
        match std::fs::read_to_string(profiles_path()) {
            Ok(contents) => match toml::from_str::<Self>(&contents) {
                Ok(store) if !store.profile.is_empty() => store,
                _ => Self::default_store(),
            },
            Err(_) => Self::default_store(),
        }
    }

    fn default_store() -> Self {
        Self {
            profile: vec![Profile::new("default", "Default", "catppuccin-mocha")],
        }
    }

    pub fn save(&self) {
        let path = profiles_path();
        let Some(parent) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        if let Ok(contents) = toml::to_string_pretty(self) {
            let _ = std::fs::write(path, contents);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Profile> {
        self.profile.iter()
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profile.iter().find(|p| p.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Profile> {
        self.profile.iter_mut().find(|p| p.id == id)
    }

    /// Creates a profile with a fresh slug id derived from `name`
    /// (`"My Profile"` -> `"my-profile"`, de-duplicated with a numeric
    /// suffix if that slug is already taken), returning the new id.
    pub fn create(&mut self, name: &str, scheme_id: &str) -> ProfileId {
        let id = self.unique_slug(name);
        self.profile.push(Profile::new(&id, name, scheme_id));
        self.save();
        id
    }

    /// Duplicates an existing profile under a new id/name ("Foo copy",
    /// "Foo copy 2", ...), every other setting included. Returns `None` if
    /// `id` doesn't exist.
    pub fn clone_profile(&mut self, id: &str) -> Option<ProfileId> {
        let mut copy = self.get(id)?.clone();
        copy.name = format!("{} copy", copy.name);
        copy.id = self.unique_slug(&copy.name);
        let new_id = copy.id.clone();
        self.profile.push(copy);
        self.save();
        Some(new_id)
    }

    /// Applies `edit` to a profile and saves — the generic setter for
    /// fields that don't need any validation of their own, so each new
    /// profile setting doesn't need its own `set_*` method.
    pub fn update(&mut self, id: &str, edit: impl FnOnce(&mut Profile)) {
        if let Some(profile) = self.get_mut(id) {
            edit(profile);
            self.save();
        }
    }

    pub fn rename(&mut self, id: &str, new_name: &str) {
        if let Some(profile) = self.get_mut(id) {
            profile.name = new_name.to_string();
            self.save();
        }
    }

    pub fn set_scheme(&mut self, id: &str, scheme_id: &str) {
        if let Some(profile) = self.get_mut(id) {
            profile.scheme_id = scheme_id.to_string();
            self.save();
        }
    }

    pub fn set_silence_seconds(&mut self, id: &str, silence_seconds: u32) {
        if let Some(profile) = self.get_mut(id) {
            profile.silence_seconds = silence_seconds;
            self.save();
        }
    }

    /// Removes a profile. No-op (returns `false`) if it's the last one —
    /// there must always be at least one profile to spawn terminals with.
    pub fn delete(&mut self, id: &str) -> bool {
        if self.profile.len() <= 1 {
            return false;
        }
        let before = self.profile.len();
        self.profile.retain(|p| p.id != id);
        let removed = self.profile.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    fn unique_slug(&self, name: &str) -> ProfileId {
        let base = slugify(name);
        if self.get(&base).is_none() {
            return base;
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|candidate| self.get(candidate).is_none())
            .expect("infinite suffix range")
    }
}

fn slugify(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "profile".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_options_round_trip_as_flat_toml_keys() {
        let mut profile = Profile::new("t", "T", "catppuccin-mocha");
        profile.options.bell = BellMode::IconSound;
        profile.options.cursor_shape = CursorShape::Ibeam;
        profile.options.cell_width_scale = 1.5;
        profile.options.scrollback_lines = 100;
        profile.options.use_system_font = false;

        let toml = toml::to_string(&profile).unwrap();
        assert!(toml.contains(r#"bell = "icon-sound""#), "{toml}");
        assert!(toml.contains(r#"cursor_shape = "ibeam""#), "{toml}");
        assert!(toml.contains("scrollback_lines = 100"), "{toml}");
        assert!(!toml.contains("[options]"), "{toml}");

        let back: Profile = toml::from_str(&toml).unwrap();
        assert_eq!(back.options, profile.options);
    }

    #[test]
    fn missing_terminal_options_fall_back_to_tilix_defaults() {
        let old = r#"
            id = "default"
            name = "Default"
            scheme_id = "catppuccin-mocha"
            scrollback_lines = 500
        "#;
        let profile: Profile = toml::from_str(old).unwrap();
        assert_eq!(profile.options.scrollback_lines, 500);
        assert!(profile.options.use_system_font);
        assert!(profile.options.show_scrollbar);
        assert_eq!(profile.options.bell, BellMode::Sound);
        assert_eq!(profile.options.cell_height_scale, 1.0);
    }

    #[test]
    fn store_with_several_profiles_round_trips() {
        let mut a = Profile::new("a", "A", "catppuccin-mocha");
        a.options.text_blink = TextBlink::Never;
        let b = Profile::new("b", "B", "tango");
        let store = ProfileStore {
            profile: vec![a, b],
        };
        let toml = toml::to_string_pretty(&store).unwrap();
        let back: ProfileStore = toml::from_str(&toml).unwrap();
        assert_eq!(back.profile.len(), 2);
        assert_eq!(back.profile[0].options.text_blink, TextBlink::Never);
        assert_eq!(back.profile[1].scheme_id, "tango");
    }
}
