# Roadmap

> **État réel au 2026-11-07 : série v0.9** (version exacte : badge Release
> du README / `CHANGELOG.md` / fenêtre « À propos », jamais en dur ici). Ce document a été réaligné sur le
> code livré — l'ancien décrivait encore un « MVP v0.1 » alors que le socle
> et une grosse partie de la parité étaient déjà codés, livrés et utilisés
> au quotidien. La numérotation a été **renumérotée** : ce qui est déjà
> livré est marqué ✅ sous des phases A/B/C, et ce qui reste à faire est
> priorisé en Phases 1-6 — voir `CHANGELOG.md` pour l'historique exact.
>
> Analyse comparative Tilix ↔ Rutile refaite le 2026-11-03 contre le repo D
> de Tilix (`/home/paul/Documents/PERSO/tilix`). Ce document part du
> principe que les 4 piliers (split + broadcast + sessions + thème) sont
> stables et utilisés au quotidien, et couvre tout ce qui manque pour la
> **parité fonctionnelle** avec Tilix.

## ✅ Déjà livré (v0.1 → v0.9)

Ci-dessous ce qui est codé et fonctionnel aujourd'hui — le socle sur
lequel la suite se construit.

### Piliers (v0.1) — stable et utilisé au quotidien

- [x] Fenêtre GTK4/libadwaita minimale, un seul `vte4::Terminal`
- [x] Modèle `split_tree.rs` : arbre binaire de splits
  (Leaf(Terminal) / Split{Horizontal|Vertical, gauche, droite})
- [x] Rendu `pane_view.rs` : traduction de l'arbre en `GtkPaned` imbriqués
- [x] Split horizontal / vertical au clavier sur le pane focus
- [x] Fermeture d'un pane + rebalance de l'arbre (le pane voisin récupère
  l'espace), **ratio de split conservé après drag** (`split_tree.rs` gagne
  `split_id`, `set_ratio`/`find_split_mut`, écriture ciblée depuis
  `paned.connect_notify_local("position", ...)`)
- [x] Navigation clavier entre panes (directionnelle, basée sur
  `leaf_rects()` / `neighbor`)
- [x] `broadcast.rs` : groupes de diffusion (aucun / session) + `feed_child()`
  synchronisé sur le groupe cible, avec exclusion par pane
- [x] Raccourcis clavier configurables (`keymap.rs` → `Keymap` TOML)
- [x] Sessions multiples via `AdwTabView` — chaque onglet porte son propre
  arbre de splits, indépendant des autres
- [x] Création / fermeture / navigation entre sessions au clavier
- [x] Thème Catppuccin Mocha embarqué via `gtk::CssProvider`

### Phase A — Profils + palettes + persistance (v0.3) ✅

- [x] Modèle `Profile` : `src/profile/{mod,model,scheme}.rs`, `ProfileId` =
  slug stable, persistance `$XDG_CONFIG_HOME/rutile/profiles.toml`,
  `ProfileStore` (create/clone/rename/delete/défaut)
- [x] `ColorScheme` chargé depuis `resources/schemes/bundled.toml` : les 9
  palettes Tilix (base16-twilight-dark, linux, material, monokai, orchis,
  solarized-dark/light, tango, yaru) + Catppuccin Mocha/Latte, surcharge
  utilisateur via `$XDG_CONFIG_HOME/rutile/schemes/*.toml`
- [x] `TerminalWidget::new(profile)` consomme le schéma ; page Préférences
  "Profils" réelle (CRUD + schéma + silence + défaut)
- [x] Persistance de session : `src/session/persist.rs` (TOML), auto-save
  vers `$XDG_STATE_HOME/rutile/last-session.toml` à la fermeture propre,
  auto-restore au lancement, actions `session-save-as`/`session-open` via
  `gtk4::FileDialog`. Restaure layout + cwd + profil (pas l'état du shell,
  comme Tilix)
- [x] Détacher une session dans une nouvelle fenêtre (`Action::DetachSession`,
  réutilise `build_window` + `restore_session`)

### Phase B — UX terminal : recherche, hyperliens, titres (v0.4-0.5) ✅

- [x] **Recherche** : `terminal/search.rs`, barre overlay (`gtk4::Overlay`),
  `search_set_regex`/`search_find_next|previous` (PCRE2 natif VTE),
  Ctrl+Shift+F
- [x] **Hyperliens** : `terminal/hyperlinks.rs` — OSC 8 + URLs nues
  http(s) + chemins fs, `match_add_regex` pour le hover (underline + curseur
  pointer), Ctrl+Click pour ouvrir, **whitelist de schemes**
  (http/https/mailto/file), toggle `enable_hyperlinks`
- [x] **Titres** : `terminal/title.rs::render_template()`
  (`${title}/${id}/${directory}/${host}/${user}`), `GtkEditableLabel`
  template-rendu avec rename pane/session

### Phase C — Notifications + clavier étendu + confirmation (v0.6) ✅

- [x] **Notifications** : `terminal/monitor.rs`, poll `/proc/<pid>` via
  `glib::timeout_add_seconds` (Linux-only), `gio::Notification` via
  `application.send_notification()`. Silence via `connect_contents_changed`
  + timeout par profil (`Profile::silence_seconds`). Cloche via
  `connect_bell`. Réglage global `enable_notifications`
- [x] **Clavier étendu** : `Action` gagne `SwitchToSessionN(u8)`,
  `ResizePane(Direction)`, `ToggleSyncCurrentPane`, `RenameSession`,
  `RenamePane`, `DetachSession`, `CopyAsHtml`, `PasteAdvanced`,
  `ToggleMargin`. Table statique → `Keymap` chargé/sauvé en TOML
  (`$XDG_CONFIG_HOME/rutile/keybindings.toml`, table statique = défauts),
  page Préférences "Raccourcis" réelle (rebind, collision)
- [x] **Confirmation de fermeture** : `dialogs/confirm_close.rs`, détecte un
  process actif au premier plan par pane (`tcgetpgrp` sur le fd du PTY via
  `vte4::Terminal::pty()`, comparé au pid du shell capturé par
  `TerminalWidget::child_pid()`), `adw::AlertDialog` avant fermeture —
  câblé sur les 5 points de fermeture (pane clavier/bouton header, session
  clavier/menu/sidebar, fenêtre entière via `close-request`). Réglage
  `Prefs::prompt_on_close_with_process` (défaut `true`, parité Tilix)
- [x] **Divers terminal livrés** : copier en HTML (`vte4::Format::Html`),
  paste avancé (normalisation CRLF→LF + trim espaces de fin), margin guide
  (ligne à 80 cols), Ctrl+Shift+C/V + middle-click paste, copy-on-select,
  context menu Tilix-parity

### Outillage / DevSecOps / packaging ✅

- [x] CI (fmt + clippy `-D warnings` + test + deny + megalinter), scorecard,
  coverage (codecov), MSRV (rust 1.92)
- [x] Conventional Commits + release-plz automatisé (bump mineur pour
  `feat:`), CHANGELOG
- [x] Packaging : AUR (`rutile` source + `rutile-bin` binaire), `.deb`
  (cargo-deb), `.rpm` (Fedora), binaire standalone, `SHA256SUMS.txt`,
  attestations de provenance SLSA
- [x] Sécurité : dependabot, secret scanning, pinning SHA des actions,
  megalinter sécurité, politique `SECURITY.md`

---

## 🗺️ Nouvelle roadmap — ce qui reste pour la parité Tilix

Les phases sont renumées par **ordre de dépendance et de valeur** ; chaque
phase est livrable et testable indépendamment. Les références `M#` renvoient
à l'écart fonctionnel identifié dans l'analyse comparative.

### Étape 0 — Réaligner le projet (faible effort, à faire immédiatement)

Corriger la documentation devenue fausse et les placeholders avant toute
nouvelle feature.

- [x] Mettre à jour `README.md` (features) et `GUIDELINE.md` pour refléter
  l'état réel (série v0.6, pas le MVP v0.1). Plus aucune version en dur dans
  la doc : le README affiche la dernière release via un badge shields.io
- [x] Reclasser les pages Préférences placeholder comme telles et lister
  leur contenu réel restant (Appearance/Encoding → Phase 2, Bookmarks →
  Phase 4, Advanced → Phase 5)
- [x] S'assurer que `CHANGELOG.md` et la page `About`/version suivent —
  dialogue « À propos de Rutile » (menu hamburger, `adw::AboutDialog`) ; la
  version vient de `CARGO_PKG_VERSION`, donc suit le bump de release-plz
  sans rien toucher

### Phase 1 — Profondeur des profils (M1) — le plus gros levier de parité

> Bloque tout ce qui dépend d'une configuration par terminal (encodage,
> badge, triggers, commande perso, action à la sortie) — c'est le goulot
> d'étranglement actuel : `Profile` ne porte que nom/schéma/silence.

Étendre `src/profile/model.rs` (TOML, `#[serde(default)]` pour rester
rétro-compatible) :

- [x] **Command / spawn** : `command` personnalisée (+ args), répertoire de
  travail par défaut, checkbox « login shell », `exit_action`
  (`close`/`restart`/`hold`) — à la sortie du shell. Logique pure dans
  `profile/spawn.rs` (testée), relance via un `adw::Banner` par pane, garde
  anti-boucle si la commande sort en moins d'1 s sous `restart`. Au passage,
  `SessionView` résout le profil par défaut à chaque nouvelle session (plus
  de snapshot figé au démarrage)
- [x] **Rendu** : cellule (cell width/height scale, bornée à 1.0–2.0 comme
  vte), custom font (avec toggle « system font », lue dans GSettings
  `org.gnome.desktop.interface monospace-font-name` ; les glyphes Powerline
  viennent de la police elle-même, rien à faire côté vte), `text_blink_mode`
  (never/focused/unfocused/always). Réglages regroupés dans
  `TerminalOptions` (`#[serde(flatten)]` dans `Profile`), défauts = ceux de
  Tilix (ex. 8192 lignes de scrollback au lieu des 512 de vte)
- [x] **Taille initiale** (cols/rows) : couverte par `--geometry
  COLONNESxLIGNES` (Phase 3) plutôt que par un réglage de profil — elle ne
  sert qu'à la taille de la fenêtre à l'ouverture et interférerait avec la
  taille restaurée par la persistance de session
- [x] **Curseur & sélection** : forme du curseur (block/ibeam/underline),
  mode de blink (system/on/off), `word_chars` (charset de sélection par
  mot)
- [x] **Scrolling** : show/hide scrollbar (terminal dans un
  `gtk4::ScrolledWindow`, `PolicyType::External` quand masquée pour garder
  le défilement), scroll on output, scroll on keystroke, limite de
  scrollback (ou illimité)
- [x] **Cloche** : terminal bell (none/sound/icon/icon+sound) — icône
  `alarm-symbolic` (pas de `bell` dans Adwaita) dans le header du pane
  pendant 5 s, comme Tilix
- [x] **Couleurs étendues** : 16 couleurs + fg/bg déjà portées par schéma,
  ajouté curseur/highlight (paires texte/fond), bold, badge + `bold_is_bright`
  — `ProfileColors`, réglages du profil par-dessus le schéma (les schémas
  Tilix eux-mêmes n'ont que fg/bg/palette), onglet Color de l'éditeur
- [x] **Badge** : texte avec variables (`terminal::title`), position
  (NW/NE/SW/SE), police (2× celle du terminal par défaut, comme Tilix, ou
  perso), couleur (fg du schéma par défaut) — onglet Badge de l'éditeur.
  **Écart assumé avec Tilix** : pas de dessin dans le fond de vte (le spike
  `background-draw`/`EVENT_SCREEN` n'a pas été nécessaire), mais un
  `gtk4::Label` en overlay semi-transparent (`terminal/badge.rs`), qui
  ignore la souris — il passe donc *au-dessus* du texte au lieu d'en
  dessous
- [x] **Switch automatique de profil** : liste de règles
  `user@host:dir` (chaque partie optionnelle, `~` développé, un répertoire
  couvre ses sous-répertoires) dans l'onglet Advanced de l'éditeur,
  réévaluées à chaque OSC 7 comme Tilix — premier profil qui matche, retour
  au profil de la session quand plus rien ne matche. Logique pure dans
  `profile/auto_switch.rs` (testée) ; le terminal ré-applique couleurs,
  options et badge (`Appearance::apply` dans `terminal/widget.rs`) sans
  relancer la commande. L'utilisateur est toujours l'utilisateur local (Tilix
  ne le change que via les triggers, Phase 5)
- [x] Éditeur de profil réel à onglets dans la fenêtre Préférences au lieu
  du CRUD sommaire : la page Profils liste les profils (schéma, badge
  « Default »), chaque ligne ouvre une sous-page
  (`PreferencesWindow::push_subpage`) avec un `adw::ViewSwitcher` sur
  General / Command / Color / Scrolling (`src/preferences/profiles.rs`).
  Les onglets Compatibilité (encodage, Phase 2), Badge et Advanced
  (triggers, Phase 5) seront ajoutés avec leur contenu
- [x] Vérifier que `TerminalWidget::new(profile)` applique tous ces champs
  au spawn et au rendu — relu champ par champ (`Appearance::apply`,
  `apply_options`, `apply_colors`, `Spawner`). Seul écart trouvé : le délai
  de silence des notifications lisait le profil *de la session*, pas celui
  du terminal (split `-p`, switch automatique) — corrigé via
  `TerminalWidget::profile_id`

### Phase 2 — Appearance & encodage (M6, M5)

Dépend de Phase 1 pour les champs par profil ; remplit les placeholders.

- [ ] **Appearance** (page Préférences réalisée) :
  - [x] Variant de thème System/Light/Dark (`AdwStyleManager`) — Catppuccin
    Latte pour le clair. `resources/catppuccin-latte.css` était en fait
    **vide** : le CSS du chrome est maintenant séparé en structure
    (`resources/chrome.css`, tokens `@rutile_*` uniquement) et palettes
    (`catppuccin-{mocha,latte}.css`, des `@define-color`), rechargées en
    direct par `app::load_theme`. **Sombre par défaut** (pas System comme
    Tilix) : la DA de Rutile est Mocha, le clair est une option
  - [x] Transparence de fond, par profil comme Tilix
    (`ProfileColors::background_transparency`, onglet Color) : la fenêtre
    est transparente (`rutile-window` dans chrome.css), chaque élément du
    chrome peint son propre fond, le terminal applique l'alpha à son fond
  - [x] Image de fond + mode (scale/tile/center/stretch), réglage global
    comme Tilix : règle CSS `background-image` sur `rutile-window`, ajoutée
    à la feuille du chrome par `app::load_theme` — visible à travers les
    terminaux transparents uniquement. Fichier introuvable → ignoré
  - [x] Style de fenêtre : normal / barre d'outils masquée / sans bordure
    (`Preferences::window_style`, appliqué en direct à toutes les fenêtres
    par `window::apply_window_preferences`). Les variantes « disable-CSD »
    de Tilix n'ont pas d'équivalent : GTK4/libadwaita ne sait pas repasser
    en décorations serveur. Sans barre d'outils, les Préférences restent
    accessibles par **Ctrl+,** (nouvelle `Action::OpenPreferences`) et par
    le menu contextuel du terminal. Au passage, un `keybindings.toml`
    existant reçoit désormais les actions ajoutées depuis (sauf si leur
    raccourci est déjà pris)
  - [x] Position du panneau de sessions gauche/droite
    (`Preferences::sidebar_position`) — Rutile n'a pas de barre d'onglets,
    l'équivalent de la « position des onglets » de Tilix est le panneau
    latéral, d'où seulement gauche/droite
  - [x] Poignées larges, « show title even when single » (groupe
    « Panes ») — défauts de Tilix (poignées fines, titre affiché).
    `LayoutStyle` copié dans `SessionView`/`PaneView`, appliqué à chaque
    rebuild et en direct sans rebuild (`PaneView::set_layout`)
- [x] **Encodage** par profil (onglet Compatibility de l'éditeur) + largeur
  CJK narrow/wide. Le crate `vte4` sûr n'enveloppe plus `set_encoding`
  (déprécié par vte), appelé via `vte4::ffi` (`terminal/encoding.rs`) ; la
  liste vient de `vte_get_encodings` à l'exécution (221 encodages avec ICU,
  UTF-8 seul sinon) plutôt que des ~80 de Tilix codés en dur. Encodage
  invalide → message, le terminal reste en UTF-8
- [ ] Page Préférences « Encoding » globale de Tilix (choisir les encodages
  proposés dans le menu du terminal) et sélecteur par terminal
  (`encoding-select`)

### Phase 3 — CLI (M2)

> Débloque Nautilus (Phase 6) et facilite le quake. Ajout
> `clap` (derive) dans un `src/cli.rs`. Vérifier comment
> `gio::Application::run()` consomme déjà `argv` avant d'ajouter clap.

- [x] Options (`src/cli.rs`, clap derive, testé sans GTK) :
  `-w/--working-directory`, `-p/--profile` (id ou nom), `-t/--title`,
  `-s/--session <path>`, `-e/--command`, `-x/--execute` (reste de la
  ligne), `--maximize`/`--minimize`/`--full-screen`, `--geometry
  COLSxROWS` (position `+X+Y` acceptée mais ignorée, Wayland ne laisse pas
  placer une fenêtre), `--new-window`/`--new-tab`, `--preferences`,
  `--new-process`. L'application passe en `HANDLES_COMMAND_LINE` : un
  second `rutile …` remet ses arguments **et son répertoire courant** à
  l'instance en cours, les erreurs (profil inconnu, dossier ou fichier de
  session introuvable) s'affichent dans le terminal de l'appelant
- [x] Mode d'activation : New Window / New Session / Split Right / Split Down
  / Focus Window (parité Tilix `new-instance`) — réglage « On new
  instance » (General), surchargé par `-a/--action`. Au passage : seule la
  *première* fenêtre restaure la dernière session (une seconde fenêtre en
  ouvrait une copie)

### Phase 4 — Signets & DnD sidebar (M4, M3)

- [x] **Signets** (commandes uniquement, pas de mots de passe) :
  `src/bookmarks/` (`Bookmark{name, command, folder: Vec<String>}`, dossiers
  imbriqués, TOML dans `$XDG_CONFIG_HOME/rutile/bookmarks.toml`, logique
  testée sans GTK), page Préférences « Bookmarks » réelle (groupes par
  dossier, ajout/édition/suppression, « Press Enter after inserting »
  activé par défaut comme Tilix), `dialogs/bookmark_picker.rs` (dialogue
  avec recherche multi-mots) et `dialogs/bookmark_editor.rs`. Ctrl+Shift+B
  (Tilix) pour insérer, Ctrl+Alt+Shift+B pour ajouter (non lié chez Tilix,
  mais une action sans raccourci n'apparaît pas dans la page Raccourcis) ;
  les deux aussi dans le menu contextuel, « Ajouter » pré-rempli avec la
  sélection. L'insertion passe par `feed_child()` → le signal `commit` →
  les panes synchronisés la reçoivent aussi, sans code dédié. Les types
  PATH/REMOTE (ssh/telnet/ftp/sftp) et le DnD de réorganisation des signets
  restent en Phase 6
- [x] **Sidebar DnD** : `gtk4::DragSource`/`DropTarget` par ligne,
  `SessionView::move_session` → `tab_view.reorder_page()`, la liste se
  reconstruit depuis l'ordre des pages (donc aussi l'ordre sauvegardé).
  Lâché hors fenêtre (`DragCancelReason::NoTarget`) → détacher la session
  (réutilise `detach_session`). Un `DropTarget` « fourre-tout » sur le
  contenu de la fenêtre accepte les lâchers sur ses terminaux/sa barre,
  sinon eux aussi compteraient comme « hors fenêtre »

### Phase 5 — Collage avancé complet + triggers + insert number (M7, M8, M9)

- [x] **Paste avancé complet** : logique pure dans `terminal/paste.rs`
  (testée), dialogues dans `dialogs/paste.rs`. Collage simple : nettoyage
  (espaces de fin y compris le dernier saut de ligne, `#`/`$` en tête) puis
  alerte si le texte contient `sudo` **et** un saut de ligne (heuristique
  de Tilix) — « Paste Anyway » ne réavertit plus dans ce terminal, comme
  Tilix. Collage avancé (Ctrl+Shift+Alt+V) : dialogue avec le texte
  éditable, l'alerte si besoin, tabulations → espaces et CRLF/CR → LF
  (choix mémorisés), Ctrl+Entrée pour coller ; un texte d'une ligne est
  collé directement. Groupe « Paste » dans General (défauts de Tilix).
  Le texte part par `paste_text` (bracketed paste respecté) et ressort par
  `commit`, donc les panes synchronisés le reçoivent. Le collage au clic
  milieu reste celui de vte (pas d'alerte, comme Tilix)
- [x] **Insert number** (`Action::InsertNumber`, Ctrl+Shift+Alt+N — non lié
  chez Tilix) : insère le numéro du terminal, sa position dans la session
  en ordre de lecture (`SplitTree::number_of`, renuméroté à la fermeture,
  comme le `terminalID` de Tilix — pas le `PaneId` global). En saisie
  synchronisée, chaque pane atteint tape **son propre** numéro (garde
  `broadcasting` tenue pendant l'insertion, sinon le `commit` d'origine
  rediffuserait le sien)
- [ ] **Triggers** (regex → action) : `terminal/regex.rs`, CSV `pattern,
  action, parameter`, éditeur dédié (comme Tilix AdvDialog), `Profiles`
  globaux + par profil. **Point le plus risqué** : pas de signal natif
  « ligne matchée » identifiée en vte4 — à spiker avant d'estimer, probable
  diff via `connect_contents_changed` + `get_text_range`. Par défaut limité
  en lignes de traitement (comme Tilix) si coût trop élevé

### Phase 6 — Mode quake, Nautilus, signets étendus, i18n (M10, M11, M4bis) — détaillé

Cette phase regroupe les fonctionnalités **explicitement différées** lors de
l'analyse (décision utilisateur initiale) et **indépendantes** les unes des
autres : chacune peut être traitée séparément, dans n'importe quel ordre, sur
demande explicite. Elles n'ont **aucune dépendance croisée** entre elles,
mais dépendent en amont de la CLI (Phase 3, pour Nautilus/quake) et de la
profondeur des profils (Phase 1, pour les signets étendus / insert password).

#### 6.1 — Mode quake / dropdown (M10)

Tilix propose un terminal « quake » : une fenêtre déroulante (style
dropdown/console) déclenchée par un raccourci global, avec hide-on-lose-focus
et retour au focus précédent. **Tension majeure avec le Wayland-first de GTK4**
— Tilix lui-même le désactive sur Wayland (`isWayland` check, #1314). À
traiter en plan dédié séparé si réellement demandé :

- [ ] Service D-Bus (`zbus`, seul usage de D-Bus côté Rutile jusqu'ici) +
  raccourci global, pour basculer la fenêtre de n'importe où
- [ ] Configuration : hauteur % (et largeur % sur X11), alignement
  (gauche/centre/droite), position (haut/bas), tab position, show on all
  workspaces, hide-on-lose-focus + délai, keep-on-top, moniteur actif /
  spécifique (X11), hide headerbar
- [ ] **Limite assumée** : sur Wayland, pas de positionnement au-dessus /
  hide-on-lose-focus / keep-on-top fiables — le mode quake serait présent
  mais « best-effort », comme Tilix. Recommandation : réserver à X11, ou
  documenter clairement les limitations Wayland
- [ ] Dépend de : CLI (Phase 3) pour les options `--quake` et géométrie ;
  `zbus` nouveau

#### 6.2 — Intégration Nautilus (M11)

Extension **Nautilus-Python** (`data/nautilus/open-rutile.py`) ajoutant au
menu contextuel :

- [ ] « Open In Rutile », « Open Rutile Here », « Open Remote Rutile »
  (ftp/sftp URIs → `ssh -t user@host [-p port] [cd dir; $SHELL]`)
- [ ] `LocationWidgetProvider` (Nautilus < 43) : raccourci clavier nautilus
  pour ouvrir Rutile au répertoire consulté (réutilise la table de
  raccourcis étendue)
- [ ] Dépend de : CLI (Phase 3) — l'extension lance `rutile -w <dir>` /
  `rutile -e <cmd>`. À décider : distribuer le script dans les paquets
  (deb/rpm) ou en script séparé dans le repo

#### 6.3 — Signets étendus : types PATH/REMOTE + DnD + mots de passe (M4bis)

La Phase 4 livre le CRUD command-only. Cette sous-phase étend aux types
manquants de Tilix :

- [ ] Types **PATH** (avec file-chooser) et **REMOTE**
  (ProtocolType SSH/TELNET/FTP/SFTP, construction de la commande
  `ssh`/`telnet`/`ftp`/`sftp`)
- [ ] DnD de réorganisation des signets (before/after/into folder) —
  `BMTreeView` Tilix
- [ ] **Signets avec mots de passe** : dépend de **libsecret** (`secretc`/
  bindings Rust). Attention : Tilix charge `LIBRARY_SECRET` en runtime ;
  côté Rust, il faudra lier `libsecret-1`. Envisager **KeepassXC / SSH agent
  / commande externe** comme alternative plus simple et sans dépendance
  native. Réserver pour une décision utilisateur explicite (surface de
  sécurité + dépendance native)
- [ ] Dépend de : Phase 4 (CRUD), Phase 1 (badge/couleurs pour l'illustration
  dans l'arbre), libsecret (nouveau) ou alternative décidée

#### 6.4 — i18n / gettext complet (M12)

Tilix est traduit en 40+ langues via gettext. Rutile n'a aucun framework
i18n :

- [ ] Framework gettext Rust (`rust-embed` des `.mo` + le traditionnel
  `gettext`-`bindgen`, ou `tr` crate) ; marquer les chaînes UI (Preferences,
  toolbar, menu, dialogs, context menu, keymap labels)
- [ ] Porter les traductions françaises (au moins `fr`) depuis le repo
  Tilix `po/` pour démarrer
- [ ] Chantier lourd et transversal (touches chaque widget) : à traiter **en
  dernier**, idéalement quand les chaînes ont stabilisé (post-Phase 2 et
  post-Phase 4), sinon toute nouvelle feature impose de retraduire
- [ ] Aucune dépendance à la CLI ni au profils

---

## Ordre de dépendance

```
Étape 0 (docs) → Phase 1 (profondeur profils)
  → Phase 2 (appearance + encodage)     ← dépend de 1
  → Phase 3 (CLI)                        ← indépendante de 1 (peut démarrer en parallèle)
  → Phase 4 (signets CRUD + DnD sidebar) ← indépendante de 1
  → Phase 5 (paste + triggers + insert)  ← dépend de 1 (champs pastes par profil)
  → Phase 6 (différé, indépendantes entre elles) ← 6.2/6.3 dépendent de 3/4/1
```

Phases 3 et 4 sont **parallélisables** avec 1 (aucune dépendance blocante) :
si la profondeur des profils est le chantier long, la CLI et le DnD sidebar
peuvent avancer en même temps.

## Vérification par phase

Après chaque phase :

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

puis vérification manuelle via `cargo run` (pas de test GTK automatisé, cf.
CLAUDE.md) — aucun nouveau module GTK-free ne doit arriver sans tests
unitaires `cargo test` (contrainte `split_tree.rs`/`broadcast.rs`).

- Phase 1 : changer commande/font/scrollback/cursor d'un profil → le nouveau
  terminal applique tout ; `exit_action=restart` relance le shell à la sortie
- Phase 2 : passer en clair (Latte) sans casser le chrome ; choisir un
  encodage non-UTF-8 → l'affichage s'y plie
- Phase 3 : `rutile -w ~/projets -p dev -e 'nvim'` ouvre une fenêtre ciblée ;
  `rutile -s session.toml` restaure la session nommée
- Phase 4 : drag&drop réordonne la sidebar sans désynchroniser
  `AdwTabView` ; un signet insère sa commande dans le terminal focus
- Phase 5 : coller un bloc multiligne déclenche l'avertissement ; un trigger
  regex→action applique sa règle dans un terminal ciblé
- Phase 6 (sur demande) : quake bascule/affiche la fenêtre et rend le focus ;
  l'extension Nautilus ouvre Rutile au bon répertoire ; les traductions fr
  s'affichent sous `LANG=fr`
