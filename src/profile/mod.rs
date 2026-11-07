pub mod auto_switch;
pub mod model;
pub mod scheme;
pub mod spawn;

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

pub use auto_switch::TerminalState;
pub use model::{
    Badge, BadgePosition, BellMode, CjkWidth, ColorPair, CursorBlink, CursorShape, ExitAction,
    Profile, ProfileColors, ProfileId, ProfileStore, TerminalOptions, TextBlink,
};
pub use scheme::{ColorScheme, ColorSchemeStore};

use crate::preferences::Preferences;

/// Everything a new terminal takes from its profile, resolved once when
/// the pane is created: the profile itself (command, exit action, ...) and
/// its color scheme, already looked up in the `ColorSchemeStore`.
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    pub profile: Profile,
    pub scheme: ColorScheme,
    /// The resolver this config came from, so the terminal can look up
    /// another profile later (automatic profile switching) against the
    /// same — live — stores. `None` disables automatic switching.
    pub resolver: Option<ProfileResolver>,
    /// The global triggers followed by the profile's, and how many rows
    /// they may read per change (see `terminal::triggers`).
    pub triggers: Vec<crate::terminal::triggers::Trigger>,
    pub trigger_max_lines: Option<usize>,
}

/// Turns profile ids into `TerminalConfig`s against one window's profile
/// and color scheme stores, plus the preferences for the global trigger
/// settings. Cheap to clone (three `Rc`s).
#[derive(Clone)]
pub struct ProfileResolver {
    profiles: Rc<RefCell<ProfileStore>>,
    schemes: Rc<ColorSchemeStore>,
    prefs: Rc<RefCell<Preferences>>,
}

impl fmt::Debug for ProfileResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProfileResolver")
    }
}

impl ProfileResolver {
    pub fn new(
        profiles: Rc<RefCell<ProfileStore>>,
        schemes: Rc<ColorSchemeStore>,
        prefs: Rc<RefCell<Preferences>>,
    ) -> Self {
        Self {
            profiles,
            schemes,
            prefs,
        }
    }

    /// A profile that no longer exists (deleted since a session was saved)
    /// falls back to default settings with the Catppuccin Mocha scheme.
    pub fn resolve(&self, profile_id: &str) -> TerminalConfig {
        let profile = self.profiles.borrow().get(profile_id).cloned();
        self.config_for(profile, profile_id)
    }

    /// `resolve`, but `None` instead of panicking if the profile store is
    /// being modified at that moment — for callbacks fired by terminal
    /// signals, which can't rule that out (see CLAUDE.md pitfall 7).
    pub fn try_resolve(&self, profile_id: &str) -> Option<TerminalConfig> {
        let profile = self.profiles.try_borrow().ok()?.get(profile_id).cloned();
        Some(self.config_for(profile, profile_id))
    }

    /// The id of the profile `wanted` names — its id, or else its name,
    /// ignoring case (`rutile -p Work`).
    pub fn find_profile_id(&self, wanted: &str) -> Option<ProfileId> {
        let profiles = self.profiles.borrow();
        let found = profiles.get(wanted).or_else(|| {
            profiles
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(wanted))
        });
        found.map(|p| p.id.clone())
    }

    /// The profile whose automatic switch rules match `state`, if any.
    /// Same `try_borrow` caveat as `try_resolve`.
    pub fn resolve_automatic(&self, state: &TerminalState) -> Option<TerminalConfig> {
        let profile = {
            let profiles = self.profiles.try_borrow().ok()?;
            auto_switch::find_profile(profiles.iter(), state, &gtk4::glib::home_dir())?.clone()
        };
        let id = profile.id.clone();
        Some(self.config_for(Some(profile), &id))
    }

    fn config_for(&self, profile: Option<Profile>, profile_id: &str) -> TerminalConfig {
        let profile =
            profile.unwrap_or_else(|| Profile::new(profile_id, "Default", "catppuccin-mocha"));
        let scheme = self.schemes.get_or_default(&profile.scheme_id).clone();
        // `try_borrow`: called from terminal signals too (see
        // `try_resolve`); preferences being edited right then only means
        // no global triggers until the next resolve.
        let (mut triggers, trigger_max_lines) = match self.prefs.try_borrow() {
            Ok(prefs) => (prefs.triggers.clone(), prefs.trigger_max_lines()),
            Err(_) => (Vec::new(), Some(256)),
        };
        triggers.extend(profile.triggers.iter().cloned());
        TerminalConfig {
            profile,
            scheme,
            resolver: Some(self.clone()),
            triggers,
            trigger_max_lines,
        }
    }
}
