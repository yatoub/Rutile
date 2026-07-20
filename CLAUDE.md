# Rutile

Réécriture de Tilix en Rust + GTK4/libadwaita + vte4. Vise la parité
fonctionnelle sur 4 piliers : tiling par split récursif, saisie synchronisée
(broadcast input), sessions multiples indépendantes, thème Catppuccin Mocha.
Voir `GUIDELINE.md` pour la vision complète et la roadmap priorisée
(`docs/ROADMAP.md` en reprend l'état coché).

## Stack

- Rust 2024, `gtk4` 0.11 (feature `v4_14`), `libadwaita` 0.9 (feature `v1_4`),
  `vte4` 0.10 (feature `v0_70`) — **volontairement en dessous** de ce
  qu'installe `pkg-config` sur la machine de dev (GTK4 4.22 etc.) : la
  feature `vX_Y` fixe la version MINIMALE système exigée par pkg-config au
  moment du link, pas la version utilisée. Choisir la version installée
  localement casse la CI (Ubuntu 24.04 n'a que GTK4 4.14.5 dans ses dépôts
  apt) — vécu en prod (premier run CI en échec sur
  `Package 'gtk4' has version '4.14.5', required version is '>= 4.21'`).
  Les features sont cumulatives (`v4_14` inclut tout ce qui est gagné par
  `v4_2`..`v4_12`) donc pas de perte d'API réellement utilisée. Toujours
  choisir la feature en fonction de la plus VIEILLE distro cible, jamais
  de la machine de dev.
- Pas de `serde`/`toml`/`zbus` tant qu'ils ne servent pas un besoin réel
  (config, D-Bus/quake mode = v0.2+).
- `libadwaita` s'importe partout via `use libadwaita as adw;` (le crate
  s'appelle `libadwaita`, pas `adw`).

## Architecture

```
src/
├── main.rs / lib.rs      # binaire mince ; toute la logique vit dans la lib
├── app.rs                # activation GTK, chargement du thème CSS
├── window.rs             # fenêtre, barre d'outils globale, raccourcis clavier
├── keymap.rs             # table de raccourcis (parité Tilix), Action enum
├── context_menu.rs       # câblage par pane : clic droit, focus souris,
│                         #   commit→broadcast, child-exited→fermeture
├── pane_header.rs        # barre par-pane façon Tilix (sync/maximize/close)
├── layout/
│   ├── split_tree.rs     # arbre de splits, SANS GTK, testable en cargo test
│   └── pane_view.rs      # rendu GTK de l'arbre (GtkPaned imbriqués)
├── session/
│   ├── session_view.rs   # AdwTabView (headless, pas de tab bar), un PaneView + broadcast
│   └── sidebar.rs        # panneau latéral façon Tilix (liste de sessions, remplace AdwTabBar)
├── terminal/
│   ├── widget.rs         # wrapper vte4::Terminal (spawn, couleurs Mocha)
│   └── broadcast.rs      # BroadcastManager, SANS GTK, testable
└── resources/catppuccin-mocha.css
```

- `split_tree.rs` et `broadcast.rs` sont volontairement indépendants de GTK
  (contrainte du guideline) — testés par `cargo test` sans fenêtre.
- Chaque pane = `[header, terminal]` empilés verticalement dans un `gtk4::Box`
  ("wrapper"), c'est ce wrapper qu'on manipule dans l'arbre `GtkPaned`, pas le
  terminal brut — le header voyage avec lui.
- `PaneId`/`SessionId` sont des `u64` alloués par un compteur global dans
  `SessionView` (pas par session) : ça garde les ids uniques, nécessaire
  pour le broadcast qui doit pouvoir référencer n'importe quel pane.

## Pièges GTK4 rencontrés (déjà corrigés, à ne pas réintroduire)

1. **Ne jamais appeler `widget.unparent()` directement sur un enfant d'un
   `GtkPaned`.** Le `Paned` garde ses propres pointeurs internes
   `start-child`/`end-child` en plus du lien générique GTK ; `unparent()`
   direct ne nettoie que le lien générique. Le `Paned` (vidé mais pas
   informé) finit par se faire détruire et son dispose() va alors
   redétacher les widgets pointés par ses références obsolètes — même
   s'ils ont depuis rejoint un tout autre arbre. Toujours passer par
   `paned.set_start_child(None)` / `set_end_child(None)` (voir
   `detach_from_parent()` dans `pane_view.rs`).

2. **`grab_focus()` juste après un rebuild ne marche pas.** Le nouvel arbre
   de widgets n'est pas encore rattaché à la fenêtre affichée au moment où
   `PaneView::rebuild()` termine (le rattachement réel se fait après, dans
   `SessionView::resync_page_child`). Le focus est donc différé via
   `glib::idle_add_local_once`.

3. **`if let Some(x) = rc.borrow().méthode() { ... }` étend la durée de vie
   du `Ref`/`RefMut` sur tout le corps du bloc** (extension de portée des
   temporaires en position de scrutinée). Si le corps du bloc fait un
   `borrow_mut()` sur le même `RefCell` (même indirectement, via une
   fonction appelée), ça panique. Toujours faire `let x = rc.borrow()....;
   if let Some(x) = x { ... }` d'abord. Ce piège s'est produit à plusieurs
   endroits (`context_menu.rs`, `window.rs`) dès qu'on a ajouté du
   `borrow_mut()` dans le câblage des panes.

4. **`vte4::Terminal` doit avoir `hexpand`/`vexpand` à `true` explicitement**
   (dans `TerminalWidget::new()`), sinon un pane peut s'effondrer à une
   taille quasi nulle dans un arbre de splits imbriqué.

5. **Le broadcast (`feed_child()`) doit se protéger contre la cascade** :
   `feed_child()` sur une cible simule une vraie frappe, qui refait émettre
   le signal `"commit"` de CETTE cible → sans garde, tout le monde
   rebroadcasterait à l'infini. Voir le `Cell<bool> broadcasting` dans
   `SessionView::broadcast_from`.

6. **Icônes** : ne pas utiliser de noms d'icônes spécifiques à un thème
   (ex. `view-split-left-right-symbolic` n'existe que dans Breeze/Papirus,
   pas dans Adwaita). Vérifié avant usage via
   `find /usr/share/icons/Adwaita -iname "*-symbolic.svg"`.

7. **Un `RefCell<SessionView>` peut se re-réentrer via un simple setter
   GTK, pas seulement via nos propres callbacks.** `AdwTabView::
   set_selected_page()` émet `notify::selected-page` *de façon
   synchrone* ; si ce setter est appelé depuis une méthode déjà en train
   de tenir un `borrow_mut()` (ex. `SessionView::new_session()`), et
   qu'un listener de ce signal fait `session_view.borrow()` (la sidebar),
   ça panique — même piège que le focus-controller de `context_menu.rs`.
   Fix générique : tout callback déclenché par un signal GTK qui pourrait
   fire pendant qu'on modifie déjà le même `RefCell` doit utiliser
   `try_borrow()`/`try_borrow_mut()` et ignorer silencieusement l'échec
   (voir `sidebar.rs::highlight_current`), OU être différé via
   `glib::idle_add_local_once` si l'action ne PEUT PAS être ignorée (voir
   `SessionView::notify_session_listeners`, qui stocke les listeners en
   `Rc<dyn Fn()>` — pas `Box` — justement pour pouvoir les cloner et les
   rejouer plus tard sans avoir besoin d'un accès `'static` à `self`).

## Modèle de synchro (broadcast)

- `BroadcastGroup` = `None` ou `Session` (pas de mode "All" — jugé sans
  sens, une session est un contexte de travail indépendant).
- `SessionView::set_broadcast_group` change l'état global, mais chaque pane
  peut en plus être **exclu localement** (`toggle_pane_sync_exclusion`,
  bouton clavier de la barre par-pane) — un pane exclu n'émet ni ne reçoit,
  même si le groupe est actif.
- Le bouton de synchro par pane n'est **visible que si un groupe est actif**
  (`group != None`) ; cette visibilité est tenue à jour partout via un
  système de listeners (`SessionView::sync_listeners`,
  `register_sync_listener`) déclenché à chaque changement de groupe ou
  d'exclusion, pas juste sur le bouton cliqué.

## Sessions : panneau latéral (pas de barre d'onglets en haut)

Direction voulue (façon Tilix, pas la barre d'onglets `AdwTabBar` par
défaut) : `session/sidebar.rs` affiche la liste des sessions dans un
panneau à gauche (`gtk4::ListBox` dans un `gtk4::ScrolledWindow`, une ligne
par session avec titre + bouton fermer). `SessionView` garde `AdwTabView`
en interne (gère bien la sélection/le cycle de vie des pages), mais sans
`AdwTabBar` — c'est la sidebar qui pilote la sélection.

- Synchronisation bidirectionnelle : cliquer une ligne appelle
  `SessionView::select_session`; les changements de sélection venus
  d'ailleurs (clavier, fermeture) sont captés via
  `tab_view().connect_selected_page_notify`. Un `Cell<bool> syncing` évite
  la boucle de rétroaction entre les deux (même pattern que le guard
  anti-cascade du broadcast).
- `SessionView::session_ids()` donne l'ordre des onglets via
  `tab_view.pages()` (un `gtk::SelectionModel`/`GListModel` de
  `AdwTabPage`) — pas de `nth_page()` dans cette version de libadwaita,
  d'où le passage par `pages().item(i)` + downcast.
- La sidebar se reconstruit entièrement à chaque changement de sessions
  (même logique "tout reconstruire" que `PaneView`/`build_widget`), via un
  système de listeners générique côté `SessionView`
  (`register_session_listener`, appelé dans `new_session`/`close_session`)
  — évite d'avoir à se souvenir d'appeler `sidebar.rebuild()` à chaque site
  d'appel qui crée/ferme une session (clavier, menu, bouton close d'un
  pane qui ferme sa session...).
- Titres des sessions actuellement génériques ("Session N") — un vrai
  titre (cwd, titre du shell) serait un ajout naturel mais pas fait ici.
- **Miniatures live** (façon Tilix) : chaque ligne affiche un
  `gtk4::Picture` alimenté par un `gtk4::WidgetPaintable` créé sur
  `SessionView::container_for(session_id)` — le `gtk4::Box` stable de la
  session (celui qui survit aux rebuilds de `PaneView`, pas le widget de
  session lui-même). `WidgetPaintable` "reflète" le rendu du widget source
  en continu, aucun timer/snapshot manuel nécessaire. Recréé à chaque
  `rebuild()` de la sidebar (donc à chaque ajout/fermeture de session,
  pas à chaque split — le split ne déclenche pas de listener de session).
- **Caché par défaut** (`sidebar.widget().set_visible(false)` dans
  `window.rs`). Le bouton `sidebar-show-symbolic` de la barre d'outils
  (avant les boutons de split) est un simple `gtk4::ToggleButton` qui
  bascule la visibilité — rien d'autre (pas de création de session, tenté
  un temps, retiré : le bouton doit juste "toggle" le panneau). Ouvrir une
  nouvelle session reste séparé (menu hamburger "Nouvelle session",
  Ctrl+Shift+T). `build_toolbar()` prend `sidebar` en paramètre (créée
  avant le toolbar dans `build_window`, ordre important).

## Fermeture de pane

- Ctrl+D n'est PAS intercepté directement (casserait `cat`, un REPL, etc.).
  À la place, chaque terminal écoute le signal `"child-exited"` de vte4 et
  ferme son pane quand le process enfant se termine, peu importe la cause
  (Ctrl+D, `exit`, crash) — comportement standard façon Tilix/GNOME
  Terminal. Fermeture différée via `idle_add_local_once` (le signal fire
  pendant le teardown du terminal, pas question de démonter son propre
  pane de façon synchrone à ce moment-là).
- Fermer le dernier pane d'une session ferme la session entière
  (`ClosePaneOutcome::SessionClosed`).

## Tests

- `cargo test` couvre `layout/split_tree.rs` (arbre, navigation
  directionnelle géométrique via `leaf_rects`/`neighbor`) et
  `terminal/broadcast.rs` (ciblage, exclusion) — 18 tests, aucun ne
  nécessite d'affichage.
- Pas de test GTK automatisé (fenêtres/widgets) : vérifié par `cargo run`
  manuel, ou par des exemples jetables dans `examples/` pilotés par code
  (créés puis supprimés après usage) quand un bug nécessite d'inspecter
  l'état réel des widgets (parent, mapped, realized...) sans dépendre de
  souris/clavier simulés.

## CI

Calquée sur le projet `susshi` (même auteur), simplifiée pour un GTK4 app
Linux-only (pas de matrice cross-platform, pas de nix/msrv/release-plz/aur
pour l'instant — à ajouter plus tard si besoin) :

- `.github/workflows/ci.yml` : jobs `lint` (fmt + clippy `-D warnings`),
  `test` (`cargo test`), `deny` (`cargo-deny`), `megalinter`. `lint`/`test`
  installent `libgtk-4-dev libadwaita-1-dev libvte-2.91-gtk4-dev` via apt
  avant de compiler (nécessaire pour lier gtk4-sys/vte4-sys).
- `deny.toml` : licences permissives autorisées (MIT/Apache-2.0/BSD/...),
  advisories yanked=deny, sources inconnues refusées.
- `.mega-linter.yml` : liste blanche volontairement restreinte aux linters
  de sécurité (actionlint, shellcheck, yamllint, scan CVE Grype/Trivy,
  détection de secrets Gitleaks/Secretlint/TruffleHog, analyse statique
  Semgrep/Checkov, SBOM Syft) — `RUST_CLIPPY` n'est pas dans la liste
  puisque le job `lint` le fait déjà séparément (pas de double emploi).
- Avant de pousser, toujours vérifier localement :
  `cargo fmt --all -- --check && cargo clippy --all-targets --all-features
  -- -D warnings && cargo test`. `cargo fmt` a dû reformater 8 fichiers
  lors de la mise en place de la CI (jamais lancé jusque-là) ; clippy a
  relevé 4 lints réels (`let_and_return`, `collapsible_if` → let-chains
  Rust 2024, `new_without_default` sur `TerminalWidget`, closure
  redondante dans `main.rs`) — tous corrigés.
- `cargo-deny` n'est pas installé dans ce sandbox, donc `deny.toml` n'a été
  vérifié que par lecture/raisonnement (licences du graphe de dépendances
  inspectées via `cargo metadata`), pas exécuté localement — à surveiller
  au premier run CI.

## Environnement de dev (sandbox Claude Code)

- Le shell par défaut ici est **zsh**, pas fish (le fish de l'utilisateur
  est celui du bureau réel, pas de ce sandbox).
- Pas d'outil de capture d'écran fiable ici (ni X11 `import`/ImageMagick,
  ni le portail D-Bus GNOME Screenshot — `AccessDenied`). Pour "voir" l'UI,
  passer par des exemples pilotés par code qui logguent l'état des widgets
  plutôt que par une vraie capture visuelle.
- Ctrl+D/EOF simulé via `feed_child` peut ne pas fermer un shell interactif
  dans ce sandbox (zsh y a probablement `ignoreeof` ou équivalent) — ce
  n'est pas un bug de l'app, juste une limite de vérification ici. Le
  mécanisme `child-exited` lui-même est validé (testé avec un script qui
  se termine tout seul).
