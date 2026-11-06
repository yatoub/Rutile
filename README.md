# 🦫 Rutile

[![CI](https://img.shields.io/github/actions/workflow/status/yatoub/Rutile/ci.yml?branch=main&style=flat-square)](https://github.com/yatoub/Rutile/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/yatoub/Rutile?style=flat-square)](https://github.com/yatoub/Rutile/releases/latest)
[![License](https://img.shields.io/github/license/yatoub/Rutile?style=flat-square)](LICENSE)
[![Maintenance](https://img.shields.io/badge/maintenance-active-brightgreen?style=flat-square)](https://github.com/yatoub/Rutile)

**Rutile** is a from-scratch Rust/GTK4 rewrite of [Tilix](https://gnunn1.github.io/tilix-web/) — recursive split tiling, synchronized input across panes, independent multi-sessions, and a Catppuccin theme, built natively on GTK4/libadwaita/vte4.

## Why Rutile?

Tilix is a great tiling terminal, but it's built on GTK3/Vte3 and is effectively unmaintained. Rather than migrate to a different terminal paradigm entirely, Rutile aims for **functional parity, not reinvention**: the goal is to keep Tilix's core workflow alive on a modern, native GNOME stack.

The value Rutile is built around — and the reason it's worth writing rather than just switching terminals — comes down to four pillars:

1. **Recursive split tiling** (horizontal/vertical, Tilix-style — not a fixed grid)
2. **Synchronized input (broadcast)** across a session, with per-pane opt-out
3. **Multiple independent sessions**, each with its own split tree
4. **The Catppuccin Mocha theme**, since GNOME is the daily desktop this targets

The four pillars above are stable and usable daily; everything else (session save/restore, notifications, per-terminal title/color, search, hyperlinks) has been layered on since. The remaining Tilix-parity work (deep profiles, appearance options, CLI, bookmarks, quake mode, Nautilus) is tracked in [`docs/ROADMAP.md`](docs/ROADMAP.md) — see [`GUIDELINE.md`](GUIDELINE.md) for the full vision.

## Features

### Core (stable, daily-driver)

- Recursive pane splitting (horizontal/vertical), with directional keyboard navigation between panes and split ratios that survive dragging/rebuilds
- Synchronized input per session, with a per-pane toggle to temporarily opt a pane out
- Multiple independent sessions, switched via a Tilix-style sidebar with live thumbnail previews (hidden by default, toggle in the header bar), or detached into their own window
- Per-pane header bar: sync toggle, maximize/restore, close, rename
- Right-click context menu on any pane (split, broadcast group), copy-on-select
- Catppuccin theme, embedded and applied over libadwaita's own chrome
- Standard terminal behavior: a pane closes automatically when its shell exits (`exit`, Ctrl+D, or a crash) — no keystroke is special-cased
- Configurable profiles with 11 bundled color schemes, and a real Preferences window (General / Profiles / Shortcuts)

### UX

- In-terminal search overlay (Ctrl+Shift+F, PCRE2-backed)
- Hyperlinks: OSC 8 + bare `http(s)` URLs + filesystem paths, opened on Ctrl+Click with a strict scheme whitelist
- Live pane/session titles with rename support and `${title}`/`${directory}` templates
- Background notifications: terminal bell + "command finished after silence" (`/proc` polling)
- Copy as HTML, advanced paste (CRLF/whitespace normalization), an 80-column margin guide
- Confirm-before-close dialog when a pane/session/window still has a foreground process
- Session save/restore (layout + cwd + profile, TOML) and detach-session-into-new-window

### Yet to come (see [`docs/ROADMAP.md`](docs/ROADMAP.md))

Deep profiles (custom command, font, cursor, scrollback, badge, triggers), appearance options (theme variant/transparency/background image), non-UTF-8 encodings, a CLI, bookmarks, sidebar drag-and-drop, advanced paste warnings, and (deferred, on request) quake mode, Nautilus integration, libsecret passwords, and i18n.

## Command line

```sh
rutile -w ~/projets -p dev -e nvim       # a session in ~/projets, "dev" profile, running nvim
rutile -s work.rutile-session.toml       # open a saved session
rutile --new-tab                         # while running: new session in the open window
rutile -a split-down -x htop -d 10       # while running: split the focused pane, run the rest of the line
rutile --geometry 120x40 --maximize      # window size in cells / state
```

A second `rutile` hands its arguments (and its working directory) over to the running instance, which opens a new window, a new session, a split or just focuses itself — `--action`, or the "On new instance" preference. `--new-process` starts a separate instance instead. See `rutile --help`.

## Keybindings

Tilix-parity, all `Ctrl+Shift+<key>` (shortcuts are fully configurable via the Preferences → Shortcuts page, persisted as TOML — the defaults below):

| Key            | Action                              |
| -------------- | ------------------------------------ |
| `O` / `E`      | Split horizontally / vertically      |
| `W`            | Close focused pane                  |
| `C` / `V`      | Copy / paste                        |
| `Alt+Shift+C/V`| Copy as HTML / paste (advanced)     |
| `↑ ↓ ← →`      | Navigate to the pane in that direction |
| `Alt+Shift+↑↓←→` | Resize the focused split         |
| `M`            | Toggle the 80-column margin guide  |
| `F`            | Toggle in-terminal search          |
| `T`            | New session                         |
| `Q`            | Close current session               |
| `D`            | Detach current session into a new window |
| `S`            | Toggle sync for current pane       |
| `F2` / `F6`    | Rename session / pane              |
| `Page Down`    | Next session                        |
| `Page Up`      | Previous session                    |
| `1`–`9`        | Switch directly to that session     |

All of the above are also reachable from the toolbar or a pane's right-click menu / header bar.

## Installation

```bash
# Arch Linux (AUR, builds from source)
paru -S rutile

# Arch Linux (AUR, pre-built binary)
paru -S rutile-bin
```

Pre-built `.deb` and `.rpm` packages are attached to each [release](https://github.com/yatoub/Rutile/releases/latest):

```bash
# Debian / Ubuntu
wget https://github.com/yatoub/Rutile/releases/latest/download/rutile_<version>_amd64.deb
sudo apt install ./rutile_<version>_amd64.deb

# Fedora
sudo dnf install https://github.com/yatoub/Rutile/releases/latest/download/rutile-<version>-1.x86_64.rpm
```

A standalone Linux x86_64 binary is also available on the releases page (requires GTK4, libadwaita, and vte-2.91-gtk4 already installed on the system).

## Building from source

Requires Rust (2024 edition) and the GTK4/libadwaita/vte4 development headers:

```bash
# Arch
sudo pacman -S gtk4 libadwaita vte4

# Debian / Ubuntu
sudo apt install libgtk-4-dev libadwaita-1-dev libvte-2.91-gtk4-dev

# Fedora
sudo dnf install gtk4-devel libadwaita-devel vte291-devel
```

```bash
git clone https://github.com/yatoub/Rutile.git
cd Rutile
cargo build --release
./target/release/rutile
```

Run the test suite (no display required — the split-tree and broadcast models are GTK-free by design):

```bash
cargo test
```

## Stack

Rust 2024 · [gtk4-rs](https://gtk-rs.org/) · [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/) · [vte4](https://gitlab.gnome.org/GNOME/vte)

## License

[MIT](LICENSE)
