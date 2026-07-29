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
}

fn default_silence_seconds() -> u32 {
    10
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
            profile: vec![Profile {
                id: "default".to_string(),
                name: "Default".to_string(),
                scheme_id: "catppuccin-mocha".to_string(),
                silence_seconds: default_silence_seconds(),
            }],
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
        self.profile.push(Profile {
            id: id.clone(),
            name: name.to_string(),
            scheme_id: scheme_id.to_string(),
            silence_seconds: default_silence_seconds(),
        });
        self.save();
        id
    }

    /// Duplicates an existing profile under a new id/name ("Foo copy",
    /// "Foo copy 2", ...). Returns `None` if `id` doesn't exist.
    pub fn clone_profile(&mut self, id: &str) -> Option<ProfileId> {
        let source = self.get(id)?.clone();
        let new_name = format!("{} copy", source.name);
        let new_id = self.create(&new_name, &source.scheme_id);
        self.set_silence_seconds(&new_id, source.silence_seconds);
        Some(new_id)
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
