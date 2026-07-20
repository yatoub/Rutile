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

## v0.2+ — Reste des fonctionnalités Tilix (après stabilisation du MVP)

- Sauvegarde/restauration de session (TOML)
- Variante Catppuccin Latte pour le mode clair
- Intégration GNOME/Nautilus native (extension `nautilus-python` + D-Bus)
- Titre et couleur personnalisés par terminal
- Notifications de fin de process en arrière-plan
- Quake mode (service D-Bus + toggle, façon zoha4)
- Packaging AUR (PKGBUILD)
