pub mod auto_switch;
pub mod model;
pub mod scheme;
pub mod spawn;

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

pub use auto_switch::TerminalState;
pub use model::{
    Badge, BadgePosition, BellMode, ColorPair, CursorBlink, CursorShape, ExitAction, Profile,
    ProfileColors, ProfileId, ProfileStore, TerminalOptions, TextBlink,
};
pub use scheme::{ColorScheme, ColorSchemeStore};

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
}

/// Turns profile ids into `TerminalConfig`s against one window's profile
/// and color scheme stores. Cheap to clone (two `Rc`s).
#[derive(Clone)]
pub struct ProfileResolver {
    profiles: Rc<RefCell<ProfileStore>>,
    schemes: Rc<ColorSchemeStore>,
}

impl fmt::Debug for ProfileResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProfileResolver")
    }
}

impl ProfileResolver {
    pub fn new(profiles: Rc<RefCell<ProfileStore>>, schemes: Rc<ColorSchemeStore>) -> Self {
        Self { profiles, schemes }
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
        TerminalConfig {
            profile,
            scheme,
            resolver: Some(self.clone()),
        }
    }
}
