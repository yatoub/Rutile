# Roadmap

## v0.1 — MVP split + broadcast + sessions + thème (objectif actuel)

- [x] Fenêtre GTK4/libadwaita minimale, un seul `vte4::Terminal`
- [x] Modèle `split_tree.rs` : arbre binaire de splits (Leaf(Terminal) / Split{Horizontal|Vertical, gauche, droite})
- [x] Rendu `pane_view.rs` : traduction de l'arbre en `GtkPaned` imbriqués
- [x] Split horizontal / split vertical au clavier sur le pane focus
- [x] Fermeture d'un pane + rebalance de l'arbre (le pane voisin récupère l'espace)
- [x] Navigation clavier entre panes (directionnelle, basée sur `leaf_rects()`)
- [x] `broadcast.rs` : groupes de diffusion (aucun / session / tous) + `feed_child()` synchronisé sur le groupe cible
- [x] Raccourcis clavier configurables (parité Tilix, table statique dans `keymap.rs`)
- [x] Sessions multiples via `AdwTabView` — chaque onglet porte son propre arbre de splits, indépendant des autres
- [x] Création / fermeture / navigation entre sessions au clavier
- [x] Thème Catppuccin Mocha embarqué via `gtk::CssProvider`, appliqué par-dessus la structure libadwaita

**Definition of done v0.1** : utilisable au quotidien pour remplacer Tilix sur
GNOME — split, broadcast et sessions multiples fonctionnels, rendu Catppuccin,
sans crash sur split/fermeture/broadcast/changement de session répétés.

## v0.2+ — Parité Tilix (après stabilisation du MVP)

Analyse comparative faite le 2026-07-22 contre le repo D de Tilix
(`/home/paul/Documents/dev/tilix`). Phasé par dépendances techniques, chaque
phase livrable/testable indépendamment. Exclus explicitement pour l'instant
(décision utilisateur) : mode quake/dropdown (D-Bus + X11), intégration
Nautilus, gestionnaire de mots de passe (libsecret), framework i18n/gettext
— regroupés en Phase 6, à reprendre en plans séparés sur demande explicite.

Deux prérequis transverses à traiter en premier, tout le reste en dépend :
- **Bug de ratio de split** : `SplitTree::Split.ratio` est figé à `0.5` et
  jamais mis à jour après un drag de `GtkPaned` (`pane_view.rs` reconstruit
  tout l'arbre à chaque mutation) — bloque une sauvegarde de session fidèle.
- **Modèle `Profile`** : aucun concept de profil aujourd'hui, un seul
  terminal partagé pour tous (palette, commande de spawn) — bloque couleurs
  multiples, encodage, titre/badge, triggers/hyperliens par profil.

### Phase 1 — Correctness + Profils + palettes

- Nouveau module `src/profile/` (`mod.rs`, `model.rs`, `scheme.rs`) : miroir
  de `src/preferences/`, `Profile` séparé de `Preferences` (qui reste un
  singleton de réglages globaux, gagne juste `default_profile_id`).
  `ProfileId` = slug stable, persistance TOML dans
  `$XDG_CONFIG_HOME/rutile/profiles.toml`.
- `ColorScheme` chargé depuis `resources/schemes/bundled.toml` (9 palettes
  Tilix — base16-twilight-dark, linux, material, monokai, orchis,
  solarized-dark/light, tango, yaru — + Catppuccin Mocha/Latte portées
  depuis les CSS existants), surchargeable par l'utilisateur via
  `$XDG_CONFIG_HOME/rutile/schemes/*.toml`.
- `TerminalWidget::new(profile: &Profile)` remplace la palette Mocha codée
  en dur ; `app.rs` cesse de charger le CSS terminal inconditionnellement
  (le CSS ne sert plus qu'au chrome GTK).
- `layout/split_tree.rs` gagne `SplitId`, `set_ratio`/`find_split_mut`
  (GTK-free, testé) ; `layout/pane_view.rs` connecte
  `paned.connect_notify_local("position", ...)` avec écriture ciblée dans
  l'arbre (pas de `rebuild()`) et garde de réentrance (`Cell<bool>`
  self-initiated, même pattern que `SessionView::broadcasting`) ;
  positionnement initial différé via `glib::idle_add_local_once`.
- Page Préférences "Profils" réelle (remplace le placeholder `AdwStatusPage`) :
  CRUD complet (créer/cloner/supprimer/renommer/défaut).
- Aucune nouvelle dépendance.

### Phase 2 — Sauvegarde/restauration de session

Dépend de Phase 1 (ratio fiable, `ProfileId`).

- `SplitTree` dérive `Serialize`/`Deserialize` directement (déjà GTK-free).
  `PaneId` étant un compteur local au process, restauration via
  `SplitTree::remap_ids` (nouvelle méthode GTK-free, testable).
- Nouveau `src/session/persist.rs` : `SavedSession{name, profile_id,
  working_directory, tree, pane_meta}`, `SavedWindow{sessions,
  active_session_index, window_width, window_height, broadcast_group}`
  (`BroadcastGroup` gagne `Serialize`/`Deserialize`). Format TOML, extension
  `.rutile-session.toml`.
- Auto-save vers `$XDG_STATE_HOME/rutile/last-session.toml` à la fermeture
  propre, auto-restore au lancement si présent (sauf argument CLI `-s`, cf.
  Phase 5). Actions `session-save`/`session-open`/`session-save-as` via
  `gtk4::FileDialog` (déjà dans gtk4 0.11).
- Limite assumée (comme Tilix) : seuls layout + cwd + profil sont restaurés,
  pas l'état du shell (un nouveau shell est spawné).

### Phase 3 — UX terminal : recherche, hyperliens/triggers, titre & badge

Dépend de Phase 1 (champs `Profile`), indépendante de Phase 2.

- **Recherche** : `terminal/search.rs`, barre overlay (le `wrapper` de pane
  passe de `gtk4::Box` à `gtk4::Overlay` — changement structurel, toujours
  `set_start_child(None)`/`set_end_child(None)` avant reparenting). Utilise
  `vte4::Terminal::search_set_regex`/`search_find_next`/`search_find_previous`
  (PCRE2 natif VTE, pas le crate `regex`). Raccourci Ctrl+Shift+F.
- **Hyperliens/triggers** : `terminal/hyperlinks.rs` (`match_add_regex`
  URL/email/custom du profil, ouverture via `check_hyperlink_at` +
  `AppInfo::launch_default_for_uri` sous Ctrl+Click). Triggers : à spiker
  avant d'estimer — pas de signal natif "ligne matchée" identifié dans
  vte4, probable diff via `connect_contents_changed` + `get_text_range`.
  **Point le plus risqué du plan.**
- **Titre/badge** : `terminal/title.rs::render_template()` pour
  `${title}`/`${id}`/`${directory}`/`${host}`/`${user}` (VTE expose déjà
  `window_title()`/`current_directory_uri()` via OSC 0/2/7). `pane_header.rs`
  et `session/sidebar.rs` passent du label fixe à un `GtkEditableLabel`
  template-rendu.
- Vérifier les signatures exactes vte4 0.10 (context7/`cargo doc`) avant de
  coder cette phase.

### Phase 4 — Notifications, clavier étendu, confirmation de fermeture ✅

- [x] **Notifications** : `terminal/monitor.rs`, poll `/proc/<pid>` via
  `glib::timeout_add_seconds` (Linux-only), `gio::Notification` via
  `application.send_notification()` (pas de dépendance `notify-rust`).
  Silence via `connect_contents_changed` + timeout par profil. Cloche via
  `connect_bell`.
- [x] **Clavier étendu** : `Action` gagne `SwitchToSessionN(u8)`,
  `ResizePane(Direction)`, `ToggleSyncCurrentPane`, `RenameSession`,
  `RenamePane`, `DetachSession`, `CopyAsHtml`, `PasteAdvanced`,
  `ToggleMargin`. Table statique → `Keymap` chargé/sauvé en TOML
  (`$XDG_CONFIG_HOME/rutile/keybindings.toml`, table statique = défauts) —
  remplit enfin la page Préférences "Raccourcis".
- [x] **Confirmation de fermeture** : `dialogs/confirm_close.rs`, détecte un
  process actif au premier plan par pane (`tcgetpgrp` sur le fd du PTY via
  `vte4::Terminal::pty()`, comparé au pid du shell capturé par
  `TerminalWidget::child_pid()`), `adw::AlertDialog` avant fermeture —
  câblé sur les 5 points de fermeture (pane clavier/bouton header, session
  clavier/menu/sidebar, fenêtre entière via `close-request`). Nouvelle
  dépendance `libc` (pas `rustix`) pour `tcgetpgrp`. A nécessité de monter
  la feature `libadwaita` de `v1_4` à `v1_5` (`AlertDialog` n'existe qu'à
  partir de 1.5 ; toujours dans le plancher Ubuntu 24.04/GNOME 46). Réglage
  `Preferences::prompt_on_close_with_process` (défaut `true`, parité
  Tilix) pour désactiver l'invite.

### Phase 5 — Drag&drop sidebar, CLI, collage avancé, signets

- **Sidebar DnD** : `gtk4::DragSource`/`DropTarget` par ligne (garder
  `ListBox` et `tab_view.reorder_page()` synchronisés), détection de drag
  hors fenêtre pour détacher une session dans une nouvelle fenêtre (réutilise
  `snapshot()` de Phase 2).
- **CLI** : nouveau `src/cli.rs`, dépendance `clap` (derive). Options :
  `-w/--working-directory`, `-p/--profile`, `-t/--title`, `-s/--session
  <path>` (Phase 2), `-e/--command`, `--maximize`/`--minimize`,
  `--geometry` (best-effort Wayland), `--new-process`, `--preferences`.
  Vérifier comment `gio::Application::run()` consomme déjà `argv` avant
  d'ajouter clap.
- **Collage avancé** : `dialogs/advanced_paste.rs` (strip premier
  caractère/espaces finaux, remplace tabs/CRLF) + avertissement "paste non
  sûr" heuristique (multi-ligne), activable en préférences.
- **Signets** (commandes uniquement, pas de mots de passe) :
  `bookmarks/model.rs` (`Bookmark{name, command, folder: Vec<String>}`,
  TOML), page Préférences "Signets" réelle, `dialogs/bookmark_picker.rs`
  (popover, insère via `terminal.feed_child()` comme le broadcast existant).
  Dépend de l'extension de `Action` de la Phase 4.

### Phase 6 — Différé, non détaillé

Mode quake/dropdown (D-Bus + X11, tension avec le Wayland-first de GTK4),
intégration Nautilus, signets avec mots de passe (libsecret), i18n/gettext
complet (40+ langues chez Tilix). Plans dédiés séparés sur demande explicite.

### Ordre de dépendance

Phase 1 (ratio + profils) → Phase 2 (persistance session) → Phase 3
(recherche/hyperliens/titre) → Phase 4 (notifications/clavier/confirmation)
→ Phase 5 (DnD sidebar/CLI/paste/signets) → Phase 6 (différé).

### Vérification par phase

Après chaque phase : `cargo fmt --all -- --check && cargo clippy
--all-targets --all-features -- -D warnings && cargo test`, puis
vérification manuelle via `cargo run` (pas de test GTK automatisé, cf.
CLAUDE.md).

- Phase 1 : split + drag de poignée + nouveau split → ratio conservé (pas
  de retour à 0.5) ; changer de profil change la palette du nouveau terminal.
- Phase 2 : sauvegarder une session avec splits imbriqués, relancer,
  vérifier arbre et ratios identiques.
- Phase 3 : recherche trouve/surligne dans un terminal avec scrollback ;
  Ctrl+Click sur une URL l'ouvre ; renommer un titre persiste à l'écran.
- Phase 4 : notification après process long terminé en arrière-plan ;
  fermer une session avec process actif déclenche la boîte de dialogue.
- Phase 5 : drag&drop réordonne la sidebar sans désynchroniser
  `AdwTabView` ; `rutile -s session.toml` restaure la session donnée.
