pub mod model;
pub mod scheme;
pub mod spawn;

pub use model::{
    BellMode, CursorBlink, CursorShape, ExitAction, Profile, ProfileId, ProfileStore,
    TerminalOptions, TextBlink,
};
pub use scheme::{ColorScheme, ColorSchemeStore};

/// Everything a new terminal takes from its profile, resolved once when
/// the pane is created: the profile itself (command, exit action, ...) and
/// its color scheme, already looked up in the `ColorSchemeStore`.
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    pub profile: Profile,
    pub scheme: ColorScheme,
}
