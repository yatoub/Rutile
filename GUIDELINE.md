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
Nautilus, notifications, titre/couleur par terminal, packaging) reste
**secondaire et non bloquant** pour la v0.1. Ne pas commencer ces chantiers
tant que split + broadcast + sessions + thème ne sont pas stables et
utilisables au quotidien.

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
│   ├── main.rs
│   ├── app.rs               # gtk::Application / adw::Application
│   ├── window.rs            # AdwApplicationWindow
│   ├── terminal/
│   │   ├── mod.rs
│   │   ├── widget.rs         # wrapper autour de vte4::Terminal
│   │   └── broadcast.rs      # groupes de saisie synchronisée
│   ├── layout/
│   │   ├── mod.rs
│   │   ├── split_tree.rs     # modèle de l'arbre de splits (indépendant de GTK)
│   │   └── pane_view.rs      # rendu récursif via GtkPaned
│   ├── session/
│   │   ├── mod.rs
│   │   └── session_view.rs   # AdwTabView, une session = un arbre de splits
│   └── config/                # placeholder, vide tant que non nécessaire
├── resources/
│   ├── catppuccin-mocha.css
│   └── catppuccin-latte.css
├── tests/
│   ├── split_tree.rs          # tests unitaires du modèle, sans fenêtre GTK
│   └── broadcast.rs           # tests unitaires des groupes, sans event GTK
└── docs/
    └── ROADMAP.md
```

Pas de dossier `dbus/` ou `nautilus/` avant la Phase 2 — les créer en avance
ne fait qu'ajouter du bruit dans le repo pendant que le MVP n'est pas fini.

## Roadmap priorisée

### v0.1 — MVP split + broadcast + sessions + thème (objectif actuel)

- [ ] Fenêtre GTK4/libadwaita minimale, un seul `vte4::Terminal`
- [ ] Modèle `split_tree.rs` : arbre binaire de splits (Leaf(Terminal) / Split{Horizontal|Vertical, gauche, droite})
- [ ] Rendu `pane_view.rs` : traduction de l'arbre en `GtkPaned` imbriqués
- [ ] Split horizontal / split vertical au clavier sur le pane focus
- [ ] Fermeture d'un pane + rebalance de l'arbre (le pane voisin récupère l'espace)
- [ ] Navigation clavier entre panes (focus suivant/précédent, ou directionnel)
- [ ] `broadcast.rs` : groupes de diffusion (aucun / session / tous) + `feed_child()` synchronisé sur le groupe cible
- [ ] Raccourcis clavier configurables (même en dur au départ, un `HashMap` statique suffit pour la v0.1)
- [ ] Sessions multiples via `AdwTabView` — chaque onglet porte son propre arbre de splits, indépendant des autres
- [ ] Création / fermeture / navigation entre sessions au clavier
- [ ] Thème Catppuccin Mocha embarqué via `gtk::CssProvider`, appliqué par-dessus la structure libadwaita

**Definition of done v0.1** : utilisable au quotidien pour remplacer Tilix sur
GNOME — split, broadcast et sessions multiples fonctionnels, rendu Catppuccin,
sans crash sur split/fermeture/broadcast/changement de session répétés.

### v0.2+ — Reste des fonctionnalités Tilix (après stabilisation du MVP)

- Sauvegarde/restauration de session (TOML)
- Variante Catppuccin Latte pour le mode clair
- Intégration GNOME/Nautilus native (extension `nautilus-python` + D-Bus)
- Titre et couleur personnalisés par terminal
- Notifications de fin de process en arrière-plan
- Quake mode (service D-Bus + toggle, façon zoha4)
- Packaging AUR (PKGBUILD)

## Conventions de code

- Un module = une responsabilité. Pas de fichier fourre-tout type `utils.rs`.
- `split_tree.rs` et `broadcast.rs` doivent rester **indépendants de GTK** —
  testables en `cargo test` sans afficher de fenêtre. Le couplage GTK vit
  uniquement dans `pane_view.rs` et `widget.rs`.
- Pas de dépendance ajoutée au `Cargo.toml` qui ne sert pas le MVP en cours
  (pas de `zbus`, pas de `serde`/`toml` tant que la config n'est pas un
  besoin réel).

## Nom du projet

**Rutile** — dispo sur AUR et comme nom de repo GitHub. Pas publié sur
crates.io (nom déjà pris), sans conséquence pour un binaire GUI qui n'a pas
vocation à être une dépendance.
