# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

---

## [0.6.2] — 2026-08-10

### Fixed

- Stop double-pasting on middle-click (fixes #39)


## [0.6.1] — 2026-08-10

### Added

- Background notifications (bell + silence-after-activity)

- Extended keyboard actions, configurable TOML keymap, shortcuts prefs page

- Ctrl+shift+c/v and middle-click paste beyond the context menu

- Add a toggle for hyperlink handling, cursor on path hover

- Open OSC 8 hyperlinks on ctrl+click

- Live pane/session titles with rename support

- In-terminal search overlay (Ctrl+Shift+F)

- Save/restore session layout, cwd, and profile

- Profile model, bundled color schemes, real Profiles page

- Tilix-parity context menu with copy-on-select

- Enforce Conventional Commits and automate releases with release-plz


### Fixed

- Stop crashing AdwPreferencesGroup on Shortcuts rebuild

- Underline plain http(s) URLs on hover, Tilix-parity

- Restrict hyperlink schemes and open system paths on ctrl+click

- Persist split ratio across drag and rebuild

- Rename crates.io package to avoid name collision

- Repair broken RPM build and drop unavailable Ubuntu 22.04 target


## [0.6.0] — 2026-07-29

### Added

- Background notifications (bell + silence-after-activity)

- Extended keyboard actions, configurable TOML keymap, shortcuts prefs page


### Fixed

- Stop crashing AdwPreferencesGroup on Shortcuts rebuild


## [0.5.0] — 2026-07-29

### Added

- Ctrl+shift+c/v and middle-click paste beyond the context menu

- Add a toggle for hyperlink handling, cursor on path hover

- Open OSC 8 hyperlinks on ctrl+click

- Live pane/session titles with rename support


### Fixed

- Underline plain http(s) URLs on hover, Tilix-parity

- Restrict hyperlink schemes and open system paths on ctrl+click


## [0.4.0] — 2026-07-28

### Added

- In-terminal search overlay (Ctrl+Shift+F)


## [0.3.0] — 2026-07-28

### Added

- Save/restore session layout, cwd, and profile

- Profile model, bundled color schemes, real Profiles page

- Tilix-parity context menu with copy-on-select


### Fixed

- Persist split ratio across drag and rebuild


## [0.2.2] — 2026-07-22

### Added

- Enforce Conventional Commits and automate releases with release-plz


### Fixed

- Bump version to 0.2.2 to ship the libglvnd-gles RPM fix

- Explicitly trigger aur-publish.yml on manual re-publish

- Correct RPM vte4 devel package name to vte291-gtk4-devel

- Bump version to unstick release-plz after package rename

- Rename crates.io package to avoid name collision

- Repair broken RPM build and drop unavailable Ubuntu 22.04 target


## [0.2.1] — 2026-07-21

### Added

- Enforce Conventional Commits and automate releases with release-plz


### Fixed

- Bump version to unstick release-plz after package rename

- Rename crates.io package to avoid name collision

- Repair broken RPM build and drop unavailable Ubuntu 22.04 target


## [0.2.0] — 2026-07-21

### Added

- Enforce Conventional Commits and automate releases with release-plz

