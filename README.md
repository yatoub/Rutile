# 🦫 Rutile

[![CI](https://img.shields.io/github/actions/workflow/status/yatoub/Rutile/ci.yml?branch=main&style=flat-square)](https://github.com/yatoub/Rutile/actions/workflows/ci.yml)
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

Everything else Tilix does (session save/restore, Quake mode, Nautilus integration, notifications, per-terminal title/color) is explicitly secondary and deferred until the four pillars above are stable and usable daily — see [`GUIDELINE.md`](GUIDELINE.md) for the full vision and [`docs/ROADMAP.md`](docs/ROADMAP.md) for current status.

## Features (v0.1)

- Recursive pane splitting (horizontal/vertical), with directional keyboard navigation between panes
- Synchronized input per session, with a per-pane toggle to temporarily opt a pane out
- Multiple independent sessions, switched via a Tilix-style sidebar with live thumbnail previews (hidden by default, toggle in the header bar)
- Per-pane header bar: sync toggle, maximize/restore, close
- Right-click context menu on any pane (split, broadcast group)
- Catppuccin Mocha theme, embedded and applied over libadwaita's own chrome
- Standard terminal behavior: a pane closes automatically when its shell exits (`exit`, Ctrl+D, or a crash) — no keystroke is special-cased

## Keybindings

Tilix-parity, all `Ctrl+Shift+<key>`:

| Key            | Action                              |
| -------------- | ------------------------------------ |
| `O`            | Split horizontally                  |
| `E`            | Split vertically                    |
| `W`            | Close focused pane                  |
| `↑ ↓ ← →`      | Navigate to the pane in that direction |
| `T`            | New session                         |
| `Q`            | Close current session               |
| `Page Down`    | Next session                        |
| `Page Up`      | Previous session                    |

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
