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
- `serde`/`toml` ajoutés dès que la fenêtre de préférences a introduit un
  vrai besoin de config persistée (voir section Préférences plus bas) —
  jusque-là volontairement absents, comme prévu par le guideline. `zbus`
  toujours absent (D-Bus/quake mode = v0.2+).
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
├── preferences/
│   ├── config.rs         # struct Preferences (serde), load()/save() en TOML
│   └── window.rs         # AdwPreferencesWindow (sidebar de catégories façon Tilix)
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

## Préférences

Fenêtre `adw::PreferencesWindow` (`preferences/window.rs`), qui donne la
navigation par sidebar de catégories gratuitement (une `AdwPreferencesPage`
= une entrée de la sidebar), plutôt que de reconstruire à la main la
`ListBox` + pile de pages que Tilix (GTK3) devait faire lui-même.

- **Une seule page a du contenu réel pour l'instant : "General"** —
  `focus_follows_mouse` (focus au survol de la souris, sans clic —
  implémenté via un `gtk4::EventControllerMotion` sur chaque terminal dans
  `context_menu::attach`, même garde `try_borrow_mut()` que le
  `EventControllerFocus` existant pour la ré-entrance) et
  `close_window_on_last_session_closed` (ferme la fenêtre au lieu de la
  laisser vide — bug latent corrigé au passage : avant cette fenêtre, fermer
  toutes les sessions laissait une fenêtre vide sans jamais quitter).
  **Défauts alignés sur ceux de Tilix** (les deux cases correspondantes
  sont cochées par défaut dans la capture d'écran fournie par
  l'utilisateur), pas sur le comportement pré-existant de Rutile.
- **Toutes les autres pages** (Appearance, Bookmarks, Shortcuts, Encoding,
  Advanced, Profiles) sont des `AdwStatusPage` placeholder — décision
  explicite de l'utilisateur : reproduire la structure complète façon
  Tilix maintenant, remplir au fur et à mesure, plutôt que n'ajouter que
  "General". Ces concepts (profils, signets, encodage autre qu'UTF-8)
  n'existent pas du tout dans Rutile aujourd'hui.
- **Stockage** : TOML dans `glib::user_config_dir()/rutile/preferences.toml`
  (pas GSettings/dconf — choix explicite de l'utilisateur, plus simple/
  indépendant de GNOME). `Preferences::load()` retombe sur les défauts si
  le fichier n'existe pas ou ne parse pas (jamais d'erreur bloquante).
  `save()` est appelé à chaque toggle (`connect_active_notify` sur chaque
  `AdwSwitchRow`), pas de bouton "Appliquer" séparé.
- `Rc<RefCell<Preferences>>` créé une fois dans `window::build_window`,
  passé partout où c'est lu (`context_menu::attach` maintenant à 5
  paramètres : `session_view, prefs, session_id, pane_id, terminal` — tous
  les sites d'appel de `attach`/`split_and_wire`/`show_menu` dans
  `context_menu.rs` et `window.rs` ont dû être mis à jour en cascade).
- L'action `win.preferences` (item de menu "Préférences" dans le hamburger)
  utilise `window.downgrade()`/`WeakRef` pour construire la fenêtre de
  préférences à la demande, pas une fenêtre pré-construite gardée en vie.

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
- **Branche `main` protégée** (configurée via `gh api
  repos/yatoub/Rutile/branches/main/protection`, pas de fichier dans le
  repo) : PR obligatoire (`required_pull_request_reviews`,
  `required_approving_review_count: 0` — pas de review externe exigée,
  juste le passage par PR), 4 status checks requis (`Lint (fmt +
  clippy)`, `Tests`, `Security (cargo deny)`, `MegaLinter` — les noms
  doivent matcher exactement les noms de job dans `ci.yml`), pas de force
  push ni de suppression de branche, conversations de review à résoudre.
  `enforce_admins: false` — le propriétaire du repo peut encore pousser
  directement en cas de besoin (ex. `update-pkgbuild` dans `release.yml`,
  voir plus bas), ce qui est voulu.

## DevSecOps (posture au-delà du lint/scan de base)

Pour une app desktop réelle distribuée largement (AUR/deb/rpm/binaire
standalone, contrairement à un outil CLI), sécuriser juste le code source
ne suffit pas — la chaîne de distribution compte tout autant :

- **Sécurité native GitHub activée** (réglages du repo, pas de fichier) :
  Dependabot alerts + Dependabot security updates (activés via `gh api -X
  PUT repos/yatoub/Rutile/vulnerability-alerts` et
  `.../automated-security-fixes` — étaient désactivés par défaut), secret
  scanning + push protection (déjà actifs par défaut). Complète Gitleaks/
  Secretlint/TruffleHog déjà dans MegaLinter sans coût CI.
- **Toutes les actions GitHub sont épinglées par SHA de commit**, pas par
  tag (`actions/checkout@<sha>  # v7`, etc. — commentaire du tag gardé pour
  la lisibilité). Un tag peut être déplacé/compromis après coup ; un SHA
  non. Seuls `svenstaro/upload-release-action` et
  `KSXGitHub/github-actions-deploy-aur` l'étaient déjà (hérités de
  susshi) ; le reste (`actions/checkout`, `dtolnay/rust-toolchain`,
  `Swatinem/rust-cache`, `EmbarkStudios/cargo-deny-action`,
  `oxsecurity/megalinter`, `dependabot/fetch-metadata`, `actions/cache`)
  a été durci en plus — Rutile va plus loin que susshi sur ce point
  précis, choix assumé vu la surface de distribution plus large.
  **Piège** : `Swatinem/rust-cache@v2` et `EmbarkStudios/cargo-deny-action@v2`
  sont des tags ANNOTÉS — `gh api repos/OWNER/REPO/git/refs/tags/vX` donne
  le SHA de l'objet tag, pas celui du commit sous-jacent. Toujours résoudre
  via `gh api repos/OWNER/REPO/commits/vX` (résout correctement dans les
  deux cas), jamais via `git/refs/tags/`.
- **`.github/workflows/scorecard.yml`** : score OpenSSF Scorecard continu
  (pinning, permissions des workflows, protection de branche...), publié
  sur push vers `main` + hebdomadaire + sur changement de
  `branch_protection_rule`. Résultats en SARIF uploadés vers l'onglet
  Security de GitHub (`github/codeql-action/upload-sarif`).
- **`SHA256SUMS.txt` généré à chaque release** (`release.yml`, job
  `checksums`, après `build`/`build-deb`/`build-rpm`) : permet à qui
  installe le binaire standalone/`.deb`/`.rpm` de vérifier l'intégrité du
  téléchargement. Pas de signature GPG pour l'instant (contrairement aux
  dépôts APT/DNF hébergés de susshi, volontairement pas mis en place ici
  — voir section Packaging) — les checksums seuls n'authentifient pas la
  provenance, juste l'intégrité vis-à-vis de ce qui est affiché sur la
  page de release GitHub elle-même.
- **`rust-version = "1.92"` dans `Cargo.toml`** + job `msrv` requis dans
  `ci.yml`. **Piège découvert en le mettant en place** : le vrai plancher
  n'est PAS lié à notre propre code (les let-chains `if let ... && let
  ...` qu'on utilise ne demandent que Rust 1.88) mais à l'écosystème
  gtk4-rs 0.11.4 lui-même, qui exige rustc ≥ 1.92 (`cargo +1.88 check`
  échoue avec "gtk4@0.11.4 requires rustc 1.92" etc., même si NOTRE code
  compilerait très bien sous 1.88). Toujours vérifier le MSRV réel avec
  `cargo +X.Y.Z check --all-targets` sur une vraie toolchain installée
  (`rustup toolchain install X.Y.Z`), jamais en déduisant juste de la
  syntaxe Rust qu'on utilise soi-même. Important puisque `PKGBUILD`/
  `rutile.spec` compilent depuis les sources avec le `cargo`/`rustc` du
  dépôt de la distro cible, pas forcément la dernière stable.
- **`SECURITY.md`** — politique de signalement de vulnérabilité (contact
  via GitHub Security Advisories privées). Coche le check "Security-Policy"
  d'OpenSSF Scorecard.
- **Image conteneur `fedora:41` du job `build-rpm` épinglée par digest**
  (`fedora@sha256:...`), même raisonnement que le pinning SHA des actions
  — récupéré via `docker pull fedora:41 && docker inspect --format=
  '{{index .RepoDigests 0}}' fedora:41` (pas de `skopeo`/`crane` dispo,
  `docker manifest inspect` seul donne un index multi-arch pas directement
  utilisable comme référence `container:`).
- **Attestations de provenance SLSA** (`actions/attest-build-provenance`,
  natif GitHub) sur les 3 artefacts de release (binaire, `.deb`, `.rpm`)
  — après le smoke test, avant l'upload. Contrairement à `SHA256SUMS.txt`
  (intégrité du téléchargement uniquement), une attestation prouve
  cryptographiquement la provenance (ce repo, ce commit, ce run précis),
  vérifiable via `gh attestation verify`. Nécessite `id-token: write` +
  `attestations: write` en plus de `contents: write` sur ces 3 jobs.
- **Job `coverage` dans `ci.yml`** (`cargo-llvm-cov` → Codecov) —
  **volontairement pas dans `required_status_checks`** de la protection
  de branche (informatif, pas un gate). Exclut du rapport les fichiers
  couplés à GTK (`main.rs`, `app.rs`, `window.rs`, `context_menu.rs`,
  `pane_header.rs`, `preferences/window.rs`, `session/sidebar.rs`) qui ne
  sont testables que manuellement de toute façon — se concentre sur les
  modules qui ont ou pourraient avoir de vrais tests unitaires. Nécessite
  un secret `CODECOV_TOKEN` non configuré pour l'instant (`fail_ci_if_error:
  false`, donc le job ne casse rien tant qu'il n'est pas ajouté).
- **Explicitement pas fait** (Phase C du plan, à activer seulement sur
  demande explicite) : CodeQL pour Rust (risque de redondance avec Semgrep
  déjà dans MegaLinter), fuzzing continu (`cargo-fuzz` sur
  `split_tree.rs`/`broadcast.rs`, bons candidats car purs/sans GTK, mais
  chantier de maintenance à part entière), `cargo-geiger` (métrique de
  code `unsafe`), signature GPG des releases.

## Conventional Commits + release-plz (versioning/CHANGELOG automatisés)

Calqué sur `release-plz.yml`/`release-plz.toml` de `susshi`, avec une
différence assumée : **pas de publication crates.io** (`publish = false`
dans `release-plz.toml`) — Rutile est un binaire GUI, pas une lib à
consommer, et le nom est de toute façon déjà pris sur crates.io (cf
GUIDELINE.md). release-plz ne gère ici QUE version dans `Cargo.toml` +
`CHANGELOG.md` + tag Git + GitHub Release (notes = changelog) ; le tag
poussé déclenche ensuite `release.yml` (déjà en place) qui construit et
attache binaire/`.deb`/`.rpm`/`SHA256SUMS.txt`/attestations SLSA à cette
même Release.

- **Chaque commit individuel doit respecter Conventional Commits** (choix
  explicite de l'utilisateur, plus strict que juste lint le titre de PR) :
  `.github/workflows/commitlint.yml` tourne sur chaque PR
  (`wagoid/commitlint-github-action`), vérifie TOUS les commits de la
  branche, pas seulement le titre — attrape l'erreur au moment du commit,
  pas seulement à l'ouverture de la PR. Config dans `.commitlintrc.yml`
  (`@commitlint/config-conventional`, types standards `feat`/`fix`/
  `chore`/`docs`/`ci`/`test`/`refactor`/`perf`/`build`/...).
  `Commitlint` ajouté aux status checks requis par la protection de
  branche (6 checks requis au total maintenant).
- **`release-plz.toml`** : `release_commits = "^(feat|fix|perf|refactor)"`
  (seuls ces types déclenchent une PR de release — évite qu'un commit
  `chore(release): update PKGBUILD to vX.Y.Z` du job `update-pkgbuild` de
  `release.yml` ne redéclenche une release en boucle),
  `features_always_increment_minor = true` (garantit bump mineur, pas
  patch, pour `feat:` tant qu'on est en 0.x), `commit_parsers` mappe les
  types conventionnels vers les sections du changelog (`feat` → "Added",
  `fix` → "Fixed", `perf`/`refactor` → "Changed", le reste — `chore`/`ci`/
  `test`/`docs` — skip du changelog mais reste linté par commitlint).
- **`.github/workflows/release-plz.yml`** : sur chaque push vers `main`,
  ouvre/rafraîchit une PR de release (bump + changelog) si des commits
  releasables sont en attente, ou crée le tag + la GitHub Release une fois
  cette PR mergée (auto-merge squash dès que la CI passe). **Réutilise le
  secret `RUTILE_RELEASE_TOKEN`** déjà créé pour `update-pkgbuild` (même
  besoin : contourner `enforce_admins: false` et déclencher `release.yml`
  sur le tag poussé, ce que `GITHUB_TOKEN` ne peut pas faire) — **mais ce
  PAT doit avoir le scope `Pull requests: Read and write` ajouté en plus
  de `Contents: Read and write`** (pas modifiable via API, à faire dans
  GitHub Settings → Developer settings → Fine-grained tokens).
- **`CHANGELOG.md`** créé avec juste l'en-tête (aucune entrée encore,
  Rutile n'a pas de tag de version) — release-plz y ajoute une section à
  chaque release.
- **Piège vécu dès le premier run** : le commit qui a *introduit*
  commitlint/release-plz était typé `feat(ci): ...` — techniquement un
  type "feat" déclenche un bump MINEUR (`features_always_increment_minor
  = true`) même si c'est un changement d'outillage pur, pas une feature
  applicative. Résultat : passage direct 0.1.0 → 0.2.0 non voulu dès le
  premier commit conventionnel. **Toujours typer les changements de
  CI/outillage en `ci:` ou `chore(ci):`, jamais `feat:`**, même quand
  GitHub lui-même range ça sous "CI/CD" — le type doit refléter l'impact
  utilisateur final, pas la catégorie de fichiers touchés.
- **`release.yml` cassé au premier vrai run** (deux bugs indépendants de
  release-plz, découverts parce que release-plz a justement déclenché un
  vrai tag pour la première fois) :
  1. `rutile.spec` : il manquait `cargo fetch --locked` dans `%prep` avant
     `cargo build --frozen` dans `%build` — le registre Cargo local d'un
     conteneur Fedora fraîchement démarré est vide, donc `--frozen` ne
     trouve aucun paquet du tout (erreur trompeuse : `error: no matching
     package named 'libadwaita' found`, qui ressemble à un problème de
     dépendance mais est en fait un problème de cache jamais peuplé).
  2. Les jobs `build`/`build-deb` de `release.yml` tournaient sur
     `ubuntu-22.04` (choisi initialement pour la compat glibc, façon
     susshi) — mais `libvte-2.91-gtk4-dev` n'existe pas du tout dans les
     dépôts d'Ubuntu 22.04 (disponible seulement à partir de 23.10+).
     Passés à `ubuntu-latest` : contrairement à un binaire CLI statique,
     Rutile a de toute façon besoin d'un GTK4/libadwaita/vte4 système
     *récent* pour tourner chez l'utilisateur, donc viser une vieille
     glibc n'élargit pas réellement la compatibilité ici — une distro trop
     vieille pour une glibc récente n'aura de toute façon pas ces libs.
  Les deux bugs vérifiés en isolant chaque étape exactement comme dans le
  run CI cassé (`cargo fetch --locked && cargo build --frozen --release`
  avec un `CARGO_HOME` neuf, pour reproduire le conteneur frais).
- **Découverte critique après avoir corrigé les deux bugs ci-dessus** : en
  relançant `release.yml` sur le tag `v0.2.0` (via `workflow_dispatch`) le
  RPM échouait TOUJOURS de la même façon — parce qu'un tag Git est figé
  sur son commit d'origine ; relancer le workflow pour un tag existant
  rejoue le code TEL QU'IL ÉTAIT à ce tag, pas la version corrigée sur
  `main`. Pour tester un fix après coup, il faut soit un nouveau tag, soit
  laisser release-plz en créer un.

  Mais release-plz refusait de proposer un nouveau bump après le fix
  (`fix:` typé correctement pourtant) : les logs du job montraient
  `downloading packages from cargo registry crates.io` →
  `Downloaded rutile v0.1.1` → `local version (0.2.0) > registry version
  (0.1.1). Only changelog will be updated` → `Already published - Tag
  v0.2.0 already exists`. **Un crate `rutile` totalement indépendant est
  déjà publié sur crates.io** (exactement ce que GUIDELINE.md avait
  anticipé — "nom déjà pris"). Même avec `publish = false` dans
  `release-plz.toml`, release-plz interroge quand même crates.io pour sa
  logique de comparaison de version ; confronté à un paquet sans rapport,
  il abandonne silencieusement toute proposition de nouveau bump une fois
  que la version locale a dépassé la version (non pertinente) du registre.

  **Fix définitif** : renommer le `[package] name` dans `Cargo.toml` en
  `rutile-terminal` (vérifié libre sur crates.io via l'index sparse —
  `curl https://index.crates.io/ru/ti/rutile-terminal` → 404 — l'API
  REST `crates.io/api/v1/crates/...` refuse les requêtes automatisées,
  utiliser l'index sparse à la place). Pour que ça reste invisible
  partout ailleurs (binaire, `rutile.spec`, `PKGBUILD`, assets `.deb`,
  `resources/rutile.desktop`), deux sections ajoutées pour garder le nom
  interne `rutile` :
  ```toml
  [lib]
  name = "rutile"
  path = "src/lib.rs"

  [[bin]]
  name = "rutile"
  path = "src/main.rs"
  ```
  Sans le `[lib]`, tout le code (`use rutile::...` dans `main.rs` et les
  tests) casse puisque Cargo dérive le nom de la lib du nom du package
  par défaut. `[package.metadata.deb].name` reste `"rutile"` sans y
  toucher — c'est juste le nom du paquet `.deb`, indépendant de l'identité
  crates.io du crate.

## Packaging (AUR / deb / rpm)

Calqué sur `release.yml`/`aur-publish.yml` de `susshi`, mais **scope réduit
à Linux uniquement** (contrairement à susshi qui est un outil CLI portable
avec matrice cross-platform macOS/Windows/musl) : Rutile dépend de
GTK4/libadwaita/vte4 système, donc un seul target `x86_64-unknown-linux-gnu`
a du sens pour l'instant.

- `.github/workflows/release.yml` (déclenché par un tag `vX.Y.Z`) :
  - `build` : binaire Linux x86_64 sur `ubuntu-22.04` (glibc plus ancienne
    = compatible avec plus de distros que `ubuntu-latest`), publié en
    asset de la GitHub Release.
  - `build-deb` : `cargo-deb` (métadonnées dans `[package.metadata.deb]`
    de `Cargo.toml`), smoke test = installation + `ldd` (vérifie que les
    libs partagées se résolvent, pas d'exécution réelle du binaire GTK
    puisqu'il n'y a pas de display dans le runner CI).
  - `build-rpm` : `rpmbuild` dans un conteneur `fedora:41`, à partir de
    `rutile.spec`, même style de smoke test.
  - `update-pkgbuild` : régénère `PKGBUILD`/`PKGBUILD.bin`/`.SRCINFO`/
    `rutile.spec` (version + b2sums) via `scripts/update-pkgbuild.sh`,
    commit direct sur `main` — déclenche ensuite `aur-publish.yml`.
- `.github/workflows/aur-publish.yml` : publie `rutile` (source) et
  `rutile-bin` (binaire pré-compilé) sur l'AUR via
  `KSXGitHub/github-actions-deploy-aur`.
- **Volontairement PAS fait** (à ajouter plus tard si besoin, façon
  susshi) : hébergement de vrais dépôts APT/DNF (gh-pages + aptly +
  createrepo_c + signature GPG) — juste des `.deb`/`.rpm` attachés à la
  Release GitHub pour l'instant. Pas de fichier icône custom pour
  `resources/rutile.desktop` (utilise l'icône générique
  `utilities-terminal` du thème système).

### Secrets à configurer avant que `release.yml`/`aur-publish.yml` marchent

Ces workflows sont écrits et syntaxiquement valides, mais **non
exécutables tant que ces secrets repo ne sont pas ajoutés** (je ne peux
pas les créer moi-même) :

- `RUTILE_RELEASE_TOKEN` — PAT (fine-grained, contents:write) d'un compte
  admin du repo. Nécessaire pour que `update-pkgbuild` pousse directement
  sur `main` (bloqué pour le token `GITHUB_TOKEN` par défaut à cause de la
  protection de branche — `enforce_admins: false` exempte seulement les
  vrais comptes admin authentifiés, pas le bot Actions) ET pour que ce
  push déclenche bien `aur-publish.yml` (les push faits avec
  `GITHUB_TOKEN` ne déclenchent jamais d'autre workflow, limitation
  GitHub volontaire anti-boucle infinie).
- `AUR_USERNAME`, `AUR_EMAIL`, `AUR_SSH_PRIVATE_KEY` — identifiants + clé
  SSH pour pousser vers `aur.archlinux.org` (paquets `rutile` et
  `rutile-bin` doivent déjà exister/être réclamés sur l'AUR au préalable).

Sans ces secrets : `build`/`build-deb`/`build-rpm` fonctionneront quand
même (accrochent les artefacts à la Release GitHub) ; seuls
`update-pkgbuild` et `aur-publish.yml` échoueront.

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
