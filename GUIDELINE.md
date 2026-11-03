# Rutile — Guideline projet (réécriture Tilix en Rust + GTK4)

## Vision & scope

Réécriture de Tilix visant la **parité fonctionnelle**, pas une réinvention UX.
Le cœur de valeur du projet, ce qui justifie de coder plutôt que de migrer vers
un autre terminal existant, c'est :

1. **Le tiling par split récursif** (horizontal/vertical, façon Tilix, pas une grille fixe)
2. **La saisie synchronisée (broadcast input)** — aucun / groupe / session
3. **Des sessions multiples**, chacune avec son propre arbre de splits indépendant
4. **Le thème Catppuccin**, GNOME étant maintenant le desktop utilisé au quotidien

Tout le reste (sauvegarde/restauration de session, quake mode, intégration
Nautilus, notifications, titre/couleur par terminal, packaging) était
**secondaire et non bloquant** pour le MVP — ce MVP (les 4 piliers ci-dessus)
est maintenant stable et utilisé au quotidien, et la plupart de ces chantiers
secondaires ont depuis été livrés (sessions, notifications, titres, couleurs
par schéma, packaging AUR/deb/rpm). Ce qui reste à construire pour la parité
Tilix est priorisé dans [`docs/ROADMAP.md`](docs/ROADMAP.md) (profondeur des
profils d'abord), avec les chantiers sensibles (quake, libsecret, Nautilus,
i18n) réservés à la Phase 6 sur demande explicite.

## Stack technique

- Rust 2024 edition
- `gtk4-rs` + `adw` (libadwaita) — look GNOME natif
- `vte4` — widget terminal (bindings mainteneurs Gtk-rs, gtk4/vte-2.91-gtk4)
- `serde` + `toml` — config (dès que la config devient nécessaire, pas avant)
- `gtk::CssProvider` — palette Catppuccin (Mocha par défaut, cohérent avec ta config WezTerm) surchargée par-dessus libadwaita ; libadwaita gère la structure (headerbar, suivi clair/sombre), le CSS surcharge les couleurs
- `zbus` — réservé aux phases ultérieures (quake mode, Nautilus)

> Note technique : libadwaita suit le thème système par défaut (Adwaita clair/sombre), ce qui n'est pas Catppuccin. Il faut donc fournir une feuille CSS Catppuccin embarquée (`resources/catppuccin-mocha.css`) chargée via `gtk::CssProvider` + `gtk::style_context_add_provider_for_display`, avec une variante Latte pour le mode clair si tu veux respecter `AdwStyleManager::is_dark()`.

## Structure du repo

```
rutile/
├── Cargo.toml
├── src/
│   ├── main.rs / lib.rs       # binaire mince + lib
│   ├── app.rs                 # gtk::Application / adw::Application
│   ├── window.rs              # AdwApplicationWindow, toolbar, raccourcis
│   ├── keymap.rs              # Action enum + Keymap TOML (défauts Tilix)
│   ├── context_menu.rs        # clic droit, focus souris, broadcast, fermeture
│   ├── pane_header.rs         # barre par-pane (sync/maximize/close/rename)
│   ├── preferences/
│   │   ├── mod.rs
│   │   ├── config.rs          # Preferences (serde, TOML)
│   │   └── window.rs          # AdwPreferencesWindow (General/Profiles/Shortcuts + placeholders)
│   ├── profile/
│   │   ├── mod.rs
│   │   ├── model.rs           # Profile + ProfileStore (TOML)
│   │   └── scheme.rs          # ColorScheme + ColorSchemeStore (bundled + user)
│   ├── layout/
│   │   ├── mod.rs
│   │   ├── split_tree.rs      # modèle de l'arbre de splits (indépendant de GTK)
│   │   └── pane_view.rs       # rendu récursif via GtkPaned
│   ├── session/
│   │   ├── mod.rs
│   │   ├── session_view.rs    # AdwTabView, une session = un arbre de splits
│   │   ├── sidebar.rs         # panneau latéral façon Tilix + vignettes live
│   │   └── persist.rs         # sauvegarde/restauration de session (TOML)
│   ├── terminal/
│   │   ├── mod.rs
│   │   ├── widget.rs          # wrapper autour de vte4::Terminal
│   │   ├── broadcast.rs       # groupes de saisie synchronisée (GTK-free)
│   │   ├── search.rs          # recherche in-terminal (PCRE2 VTE)
│   │   ├── hyperlinks.rs      # OSC 8 + URLs + chemins, Ctrl+Click
│   │   ├── title.rs           # templates de titres (GTK-free)
│   │   ├── monitor.rs         # notifications fond (bell + silence)
│   │   └── margin.rs          # guide de marge 80 cols
│   └── dialogs/
│       ├── mod.rs
│       └── confirm_close.rs   # confirmation de fermeture process
├── resources/
│   ├── catppuccin-mocha.css / catppuccin-latte.css
│   ├── rutile.desktop
│   └── schemes/bundled.toml   # 11 schémas (9 Tilix + Catppuccin Mocha/Latte)
├── tests/
│   ├── split_tree.rs          # tests unitaires du modèle, sans fenêtre GTK
│   └── broadcast.rs           # tests unitaires des groupes, sans event GTK
└── docs/
    └── ROADMAP.md
```

Pas de dossier `dbus/` ou `nautilus/` avant la Phase 6 (sur demande
explicite) — les créer en avance n'ajoute que du bruit tant qu'ils ne sont
pas des chantiers actifs.

## Roadmap

**État réel (série v0.6, version exacte : badge Release du README /
`CHANGELOG.md`).** Les 4 piliers (split, broadcast, sessions, thème)
sont stables et utilisés au quotidien, et la parité Tilix a déjà été
étendue bien au-delà du MVP : profils + schémas de couleurs, persistance
de session, recherche, hyperliens, titres live, notifications de fond,
confirmation de fermeture, clavier étendu configurable, détacher une
session, copy-as-HTML, paste avancé, margin guide.

Voir [`docs/ROADMAP.md`](docs/ROADMAP.md) pour la liste coordonnée (ce qui
est livré) et la feuille de route vers la parité complète (profondeur des
profils, appearance/encodage, CLI, signets, DnD sidebar, triggers, puis —
sur demande explicite — quake, Nautilus, libsecret, i18n).

Règles de planification rappelées ici :

- La **profondeur des profils** est le goulot d'étranglement actuel : toute
  feature « par terminal » (encodage, badge, triggers, commande perso,
  action à la sortie, curseur, scrollback, cloche) en dépend. À traiter en
  premier (Phase 1 de la roadmap).
- **CLI** et **DnD sidebar** sont parallélisables en même temps
  (indépendantes des profils) — ils débloquent ensuite Nautilus et le quake.
- Les chantiers à **forte tension avec le Wayland-first** de GTK4 (quake,
  keep-on-top, hide-on-lose-focus) et ceux à **dépendance native** (libsecret)
  restent différés et ne démarreront que sur demande explicite.

## Conventions de code

- Un module = une responsabilité. Pas de fichier fourre-tout type `utils.rs`.
- `split_tree.rs` et `broadcast.rs` doivent rester **indépendants de GTK** —
  testables en `cargo test` sans afficher de fenêtre. Le couplage GTK vit
  uniquement dans `pane_view.rs` et `widget.rs`.
- Pas de dépendance ajoutée au `Cargo.toml` qui ne sert pas le chantier en
  cours. `serde`/`toml` sont déjà là (config persistée) ; `zbus`, `clap`
  (CLI), et les bindings `libsecret` restent **réservés** tant qu'un chantier
  de la roadmap ne les requiert pas explicitement.

## Nom du projet

**Rutile** — dispo sur AUR et comme nom de repo GitHub. Pas publié sur
crates.io (nom déjà pris), sans conséquence pour un binaire GUI qui n'a pas
vocation à être une dépendance.
