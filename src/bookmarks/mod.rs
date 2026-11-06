//! Tilix's bookmarks, limited for now to its "command" type (no path,
//! remote or password bookmarks — see `docs/ROADMAP.md` Phase 6): named
//! command lines, filed in nested folders, inserted into the focused
//! terminal (`dialogs::bookmark_picker`).
//!
//! GTK-free apart from the config directory lookup, so the filtering and
//! folder logic is testable with plain `cargo test`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub name: String,
    /// Inserted as typed text, not run in a new process — the shell in the
    /// focused terminal runs it.
    pub command: String,
    /// Nested folders, outermost first (`["prod", "db"]` is `prod/db`).
    /// Empty: top level.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folder: Vec<String>,
}

/// Every bookmark, persisted as TOML in
/// `$XDG_CONFIG_HOME/rutile/bookmarks.toml`. Bookmarks are addressed by
/// their index in this list: the UI rebuilds from scratch after every
/// change, so indices never outlive a modification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookmarkStore {
    #[serde(default)]
    bookmark: Vec<Bookmark>,
}

fn store_path() -> PathBuf {
    let mut path = gtk4::glib::user_config_dir();
    path.push("rutile");
    path.push("bookmarks.toml");
    path
}

impl BookmarkStore {
    /// Loads bookmarks from disk — empty if the file doesn't exist yet or
    /// doesn't parse.
    pub fn load() -> Self {
        std::fs::read_to_string(store_path())
            .map(|contents| Self::from_toml(&contents))
            .unwrap_or_default()
    }

    fn from_toml(contents: &str) -> Self {
        toml::from_str(contents).unwrap_or_default()
    }

    pub fn save(&self) {
        let path = store_path();
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

    pub fn get(&self, index: usize) -> Option<&Bookmark> {
        self.bookmark.get(index)
    }

    pub fn is_empty(&self) -> bool {
        self.bookmark.is_empty()
    }

    /// Adds and saves; returns the new bookmark's index.
    pub fn add(&mut self, bookmark: Bookmark) -> usize {
        self.bookmark.push(bookmark);
        self.save();
        self.bookmark.len() - 1
    }

    pub fn replace(&mut self, index: usize, bookmark: Bookmark) {
        if let Some(slot) = self.bookmark.get_mut(index) {
            *slot = bookmark;
            self.save();
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.bookmark.len() {
            self.bookmark.remove(index);
            self.save();
        }
    }

    /// Where `bookmark` is in the store, by value — for a list built from
    /// an earlier load (Preferences), which may since have been changed
    /// by another window.
    pub fn position(&self, bookmark: &Bookmark) -> Option<usize> {
        self.bookmark.iter().position(|b| b == bookmark)
    }

    /// Every bookmark with its index, folder by folder (folders in
    /// alphabetical order, the top level first), then by name — how both
    /// the Preferences page and the picker list them.
    pub fn sorted(&self) -> Vec<(usize, &Bookmark)> {
        let mut entries: Vec<(usize, &Bookmark)> = self.bookmark.iter().enumerate().collect();
        entries.sort_by_cached_key(|(_, b)| {
            (
                b.folder
                    .iter()
                    .map(|f| f.to_lowercase())
                    .collect::<Vec<_>>(),
                b.name.to_lowercase(),
            )
        });
        entries
    }

    /// `sorted`, keeping only the bookmarks matching every word of
    /// `query` (case-insensitive) in their name, command or folder path.
    pub fn matching(&self, query: &str) -> Vec<(usize, &Bookmark)> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.sorted()
            .into_iter()
            .filter(|(_, b)| {
                let haystack =
                    format!("{} {} {}", b.name, b.command, format_folder(&b.folder)).to_lowercase();
                words.iter().all(|word| haystack.contains(word.as_str()))
            })
            .collect()
    }

    /// Every folder in use, including the parents of nested ones
    /// (`prod/db` also yields `prod`), sorted — suggestions for the editor.
    pub fn folders(&self) -> Vec<Vec<String>> {
        let mut folders: Vec<Vec<String>> = self
            .bookmark
            .iter()
            .flat_map(|b| (1..=b.folder.len()).map(|depth| b.folder[..depth].to_vec()))
            .collect();
        folders.sort_by_key(|f| f.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>());
        folders.dedup();
        folders
    }
}

/// `"prod / db/"` → `["prod", "db"]`: `/`-separated, trimmed, empty
/// segments dropped.
pub fn parse_folder(text: &str) -> Vec<String> {
    text.split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

/// `["prod", "db"]` → `"prod/db"`.
pub fn format_folder(folder: &[String]) -> String {
    folder.join("/")
}

/// The text typed into the terminal for `command`: its own trailing line
/// breaks dropped, then a single Enter added when `include_return` (Tilix's
/// "Include return character with bookmark", on by default).
pub fn text_to_insert(command: &str, include_return: bool) -> String {
    let mut text = command.trim_end_matches(['\n', '\r']).to_string();
    if include_return {
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bookmark(name: &str, command: &str, folder: &str) -> Bookmark {
        Bookmark {
            name: name.to_string(),
            command: command.to_string(),
            folder: parse_folder(folder),
        }
    }

    fn store() -> BookmarkStore {
        BookmarkStore {
            bookmark: vec![
                bookmark("Logs", "journalctl -fu nginx", "prod/web"),
                bookmark("disk", "df -h", ""),
                bookmark("Backup", "pg_dump app > app.sql", "prod/db"),
                bookmark("build", "cargo build --release", "dev"),
            ],
        }
    }

    #[test]
    fn folders_parse_and_format_round_trip() {
        assert_eq!(parse_folder(" prod / db/ "), ["prod", "db"]);
        assert_eq!(parse_folder("//"), Vec::<String>::new());
        assert_eq!(format_folder(&parse_folder("a/b/c")), "a/b/c");
    }

    #[test]
    fn sorted_lists_top_level_first_then_folders_then_names() {
        let store = store();
        let names: Vec<&str> = store
            .sorted()
            .iter()
            .map(|(_, b)| b.name.as_str())
            .collect();
        assert_eq!(names, ["disk", "build", "Backup", "Logs"]);
    }

    #[test]
    fn matching_needs_every_word_anywhere() {
        let store = store();
        let names = |query| -> Vec<String> {
            store
                .matching(query)
                .iter()
                .map(|(_, b)| b.name.clone())
                .collect()
        };
        assert_eq!(names("prod"), ["Backup", "Logs"]);
        assert_eq!(names("PROD nginx"), ["Logs"]);
        assert_eq!(names("cargo"), ["build"]);
        assert_eq!(names(""), ["disk", "build", "Backup", "Logs"]);
        assert!(names("nothing-like-this").is_empty());
    }

    #[test]
    fn folders_include_parents_once() {
        let folders: Vec<String> = store().folders().iter().map(|f| format_folder(f)).collect();
        assert_eq!(folders, ["dev", "prod", "prod/db", "prod/web"]);
    }

    #[test]
    fn indices_from_sorted_address_the_store() {
        let store = store();
        let (index, first) = store.sorted()[0];
        assert_eq!(store.get(index), Some(first));
    }

    #[test]
    fn toml_round_trip_and_bad_files() {
        let store = store();
        let toml = toml::to_string_pretty(&store).unwrap();
        assert_eq!(BookmarkStore::from_toml(&toml), store);
        assert!(BookmarkStore::from_toml("not [[[ toml").is_empty());
    }

    #[test]
    fn inserted_text_ends_with_exactly_one_return_when_asked() {
        assert_eq!(text_to_insert("ls -la\n\n", true), "ls -la\n");
        assert_eq!(text_to_insert("ls -la\r\n", false), "ls -la");
        assert_eq!(text_to_insert("ls", true), "ls\n");
    }
}
