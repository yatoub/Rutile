use std::path::PathBuf;

use gtk4::gdk;
use serde::{Deserialize, Serialize};

/// A named 16-color ANSI palette plus default fg/bg, matching Tilix's
/// notion of a color scheme. Stored as hex strings (not `gdk::RGBA`
/// directly) so it round-trips through TOML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColorScheme {
    pub id: String,
    pub name: String,
    pub foreground: String,
    pub background: String,
    /// black, red, green, yellow, blue, magenta, cyan, white, then the
    /// bright variants in the same order — the standard ANSI 16-color
    /// layout `vte4::Terminal::set_colors` expects.
    pub palette: [String; 16],
}

#[derive(Deserialize)]
struct BundledSchemes {
    scheme: Vec<ColorScheme>,
}

impl ColorScheme {
    pub fn foreground_rgba(&self) -> gdk::RGBA {
        parse_or_fallback(&self.foreground)
    }

    pub fn background_rgba(&self) -> gdk::RGBA {
        parse_or_fallback(&self.background)
    }

    pub fn palette_rgba(&self) -> [gdk::RGBA; 16] {
        std::array::from_fn(|i| parse_or_fallback(&self.palette[i]))
    }
}

/// Falls back to opaque black on a malformed hex string (e.g. a
/// hand-edited user override file) rather than panicking — a broken color
/// shouldn't be able to crash terminal creation.
fn parse_or_fallback(hex: &str) -> gdk::RGBA {
    gdk::RGBA::parse(hex).unwrap_or(gdk::RGBA::BLACK)
}

/// Holds every known color scheme: the ones bundled in the binary
/// (`resources/schemes/bundled.toml`) plus any user overrides dropped in
/// `$XDG_CONFIG_HOME/rutile/schemes/*.toml` — an override with the same
/// `id` as a bundled scheme replaces it, anything else is added.
pub struct ColorSchemeStore {
    schemes: Vec<ColorScheme>,
}

impl ColorSchemeStore {
    pub fn load() -> Self {
        let bundled: BundledSchemes =
            toml::from_str(include_str!("../../resources/schemes/bundled.toml"))
                .expect("bundled.toml must parse — it ships with the binary");
        let mut schemes = bundled.scheme;

        for user_scheme in load_user_schemes() {
            if let Some(existing) = schemes.iter_mut().find(|s| s.id == user_scheme.id) {
                *existing = user_scheme;
            } else {
                schemes.push(user_scheme);
            }
        }

        Self { schemes }
    }

    pub fn get(&self, id: &str) -> Option<&ColorScheme> {
        self.schemes.iter().find(|s| s.id == id)
    }

    /// The scheme to use when a profile references an id that no longer
    /// exists (e.g. a user scheme file was deleted after a profile started
    /// using it) — falls back to Catppuccin Mocha, Rutile's own default.
    pub fn get_or_default(&self, id: &str) -> &ColorScheme {
        self.get(id)
            .or_else(|| self.get("catppuccin-mocha"))
            .expect("catppuccin-mocha is always bundled")
    }

    pub fn iter(&self) -> impl Iterator<Item = &ColorScheme> {
        self.schemes.iter()
    }
}

fn schemes_dir() -> PathBuf {
    let mut path = gtk4::glib::user_config_dir();
    path.push("rutile");
    path.push("schemes");
    path
}

fn load_user_schemes() -> Vec<ColorScheme> {
    let Ok(entries) = std::fs::read_dir(schemes_dir()) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "toml"))
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|contents| toml::from_str::<ColorScheme>(&contents).ok())
        .collect()
}
