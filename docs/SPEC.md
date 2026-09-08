# Brief Claude Code — Client Git desktop en Rust / GPUI

> **Version 3** — Windows retiré du périmètre. Cibles : Linux/Omarchy et macOS.
>
> **Comment utiliser ce document** : place-le sous `docs/SPEC.md`, avec le design produit par
> Claude Design sous `docs/DESIGN.md` et le contrat de tokens sous `docs/DESIGN-TOKENS.md`.
> Lance ensuite Claude Code avec « Lis `docs/SPEC.md`, `docs/DESIGN-TOKENS.md` et
> `docs/DESIGN.md` en entier, puis attaque le jalon M0 et rien d'autre. »
> Ne demande jamais plusieurs jalons d'un coup.
>
> **Hiérarchie d'autorité en cas de divergence** : `DESIGN-TOKENS.md` sur les noms et formules
> de tokens · `DESIGN.md` sur l'usage visuel et les états d'interface · `SPEC.md` sur tout le
> reste.
> Remplace `layers` par le nom définitif du binaire.

---

## 1. Objectif

Construire `layers`, un client Git graphique **inspiré de Tower mais délibérément plus simple**,
écrit en Rust, rendu par GPUI, stylé avec le design system Omarchy.

**Deux plateformes, dans cet ordre :**

1. **Linux / Omarchy** — Wayland en primaire, X11 en secours. Cible de conception.
2. **macOS** — cible de distribution complète, signée et notarisée.

Trois écrans structurent le produit — **Repositories**, **Working Copy**, **History** — plus
Stashes, Branches et Settings. Les maquettes et le détail visuel sont dans `docs/DESIGN.md`,
qui fait autorité sur tout ce qui touche à l'apparence et aux états d'interface.

## 2. Ce qui n'est pas dans le périmètre

À traiter comme absent, pas comme différé-mais-préparé.

**Windows.** Aucune contrainte Windows ne doit peser sur ce projet : pas de code spécifique,
pas de test, pas de CI, pas de packaging, pas de compromis d'architecture pour le rendre
possible. La seule chose qui subsiste est la couche `platform/` derrière des traits — mais
elle existe parce que Linux et macOS divergent déjà, pas pour anticiper Windows. Si la
question se pose un jour, elle se posera à partir de cette couche, sans que rien n'ait été
payé d'avance.

**Fonctionnalités hors MVP** (§11) : point d'extension identifiable, mais **aucun code mort**,
aucun trait spéculatif, aucune abstraction « au cas où ». Une abstraction sans deuxième
implémentation réelle est une dette, pas une préparation.

## 3. Règles de travail non négociables

Lis cette section avant d'écrire une ligne de code, et relis-la avant chaque commit.

1. **Vérifie l'état réel de l'écosystème avant de coder.** Les versions de `gpui-kit`,
   `gpui-omarchy` et surtout de `gitoxide` bougent vite, et les capacités décrites plus bas
   peuvent être périmées. Avant chaque jalon : lis le `Cargo.toml` et la doc de la version
   effectivement résolue, et écris un court `docs/notes/<crate>-capabilities.md`.
   **Ne suppose jamais qu'une API existe parce qu'elle serait logique.** Si tu n'es pas sûr,
   écris un test qui prouve le comportement.
2. **Aucun blocage du thread UI.** GPUI rend sur un seul thread. Toute opération Git, tout accès
   disque, tout appel réseau part sur l'executor de fond et revient par un message. Une seule
   violation et l'app gèle sur un gros dépôt.
3. **Pas de `unwrap()` ni de `expect()` hors tests et initialisation.** Les erreurs Git sont
   normales, pas exceptionnelles : elles remontent à l'UI dans un état dédié, jamais en panique.
4. **Aucune couleur, taille ou espacement codé en dur dans les composants.** Tout passe par les
   tokens de `layers-theme`, **sous les noms canoniques de `DESIGN-TOKENS.md`**. Un
   `rgb(0x1a1b26)` dans un composant est un bug ; un `theme.fg` au lieu de `theme.text` aussi.
   Les maquettes HTML utilisent des abréviations CSS (`--fg`, `--ac`, `--raised`) : ce sont des
   alias de livraison, jamais les noms du code.
5. **Le cœur Git ne connaît pas l'UI.** `layers-git` ne dépend ni de `gpui` ni de
   `gpui-omarchy`. Testable et utilisable en CLI.
6. **Aucun `#[cfg(target_os)]` dans la couche UI.** Ce qui diverge passe derrière un trait dans
   `layers-app/src/platform/`.
7. **Jamais d'opération destructive silencieuse.** `reset --hard`, `push --force`, suppression
   de branche, discard : confirmation explicite et journalisation.
8. **Un jalon = une PR**, avec `cargo fmt --check`, `cargo clippy -- -D warnings` et
   `cargo test` verts **sur Linux et macOS** avant de proposer la suite.
9. **Écris les tests avec le code.** Chaque commande Git implémentée arrive avec au moins un
   test d'intégration sur un dépôt temporaire.

## 4. Pile technique

| Couche | Choix | Note |
|---|---|---|
| Langage | Rust, édition 2024, toolchain épinglée dans `rust-toolchain.toml` | |
| UI | `gpui-kit` (embarque GPUI via les snapshots `gpui-pre-*`) | ne dépend jamais de `gpui` directement |
| Design system | `gpui-omarchy` | MIT — **à vendorer, voir §5** |
| Git (lecture) | `gix` (gitoxide) | rapide, pur Rust, pas de dépendance C |
| Git (écriture / réseau) | binaire `git` en sous-processus | choix délibéré, voir §8 |
| Surveillance FS | `notify` + debounce maison | |
| Coloration syntaxique | `tree-sitter` + grammaires | GPUI l'embarque déjà |
| Diff | `imara-diff` ou `gix-diff` | Myers + raffinement mot à mot |
| Couleur | `palette` ou équivalent, avec support OKLCH | indispensable pour §6 |
| Async | l'executor de GPUI (`background_spawn`) | **pas** de runtime Tokio global |
| Erreurs | `thiserror` dans les crates, `anyhow` dans le binaire | |
| Logs | `tracing` + `tracing-subscriber`, fichier tournant | |
| Config | `serde` + TOML | emplacement par OS, voir §9 |

## 5. Vendorer `gpui-omarchy` dès M0

`gpui-omarchy` est en 0.1.0, maintenu par une seule personne, avec `gpui-kit = "=0.6.0"`
épinglé en dur, et son README indique explicitement que le framework n'est pas complet.

**Action au jalon M0** : copier le crate dans `vendor/gpui-omarchy/` avec sa licence MIT et son
commit d'origine noté dans `vendor/README.md`, le référencer par `path`, et créer
`scripts/sync-vendor.sh` qui diffe l'upstream et signale les évolutions.

Rationale à consigner dans la PR : on ne veut pas qu'un upstream de 7 étoiles bloque un projet
de plusieurs mois. On contribue les correctifs génériques en amont, on garde les divergences
produit en local.

Vérifie aussi dès M0 qu'il se comporte correctement **sur macOS** : absence totale d'Omarchy →
repli sur thème embarqué, sans panique ni chemin Linux codé en dur.

## 6. Le système de thème — spécification complète

C'est le module le plus discuté du projet, il est donc entièrement spécifié. Il vit dans
`layers-theme` et n'a **aucune dépendance UI**, ce qui le rend testable seul.

### 6.1 Sources, par ordre de priorité

1. **Override utilisateur** : un thème choisi explicitement dans les préférences. Désactive
   tout suivi, persiste entre les sessions.
2. **Omarchy Quattro** (Linux uniquement) : lit
   `~/.local/state/omarchy/current/theme.name` et
   `~/.local/state/omarchy/current/theme/colors.toml`, surveille le répertoire, et met à jour
   les fenêtres **uniquement quand le thème change réellement**. Activé par défaut au premier
   lancement si un état Quattro valide est trouvé.
3. **Apparence système macOS** : suit le mode clair/sombre de l'OS, y compris le basculement
   automatique, et associe deux thèmes embarqués.
4. **Thème embarqué par défaut** : Tokyo Night en sombre, un thème clair en clair.

**Les dispositions Omarchy legacy et l'extraction de couleurs depuis des configs de terminal ne
sont pas supportées.** Si l'état Quattro est absent ou invalide, l'option n'est simplement pas
proposée à l'utilisateur — pas de support partiel, pas de devinette.

Sur macOS il n'y a pas d'Omarchy : **les thèmes embarqués y sont l'expérience par défaut**,
pas un secours dégradé. Leur qualité et la présence d'un thème clair de première classe
comptent autant que le mapping Omarchy.

### 6.2 Entrées lues

**Garanties, les seules obligatoires :**

```
background   foreground   accent   selection   muted   mode ("dark" | "light")
```

**Optionnelles, lues si présentes :** `red`, `green`, `yellow`, `blue`.

Toute autre clé est ignorée. Si une entrée garantie manque ou est invalide, **la palette entière
tombe en repli sur le thème embarqué** — jamais de mélange partiel entre deux sources, qui
produirait des combinaisons illisibles.

> **Amendement, jalon M3 (2026-09-08) — la cinquième clé est `muted`, pas `color8`.**
>
> Cette liste disait `color8`. **Aucun thème Omarchy n'en a jamais eu** : les 23 thèmes
> installés sur une machine Omarchy écrivent `muted`, et c'est la même donnée sous un autre
> nom — le `tokyo-night` d'Omarchy a `muted = "#414868"`, et le Tokyo Night embarqué de
> `omagit-theme` porte `bright_black: 0x414868`.
>
> Combinée à la règle du paragraphe ci-dessus, l'erreur était totale plutôt que partielle :
> une clé garantie manquante rejette la palette entière, donc **toutes** les palettes Omarchy
> étaient rejetées et la source que §6.1 rend prioritaire sur Linux retombait silencieusement
> sur un thème embarqué, à chaque lancement, depuis M1.
>
> Elle a survécu au jalon dont le sujet était le thème parce que M1 a été écrit et vérifié sur
> macOS, où le répertoire d'état n'existe pas : le lecteur n'a jamais vu qu'une fixture, et la
> fixture avait été transcrite depuis ce paragraphe. **Une fixture écrite d'après un document
> teste le document.** Les fixtures sont désormais des fichiers réels, copiés tels quels dans
> `tests/fixtures/omarchy/`.
>
> `color8` n'est pas accepté en alias : un repli pour une orthographe qui n'existe dans aucun
> fichier est du code sans appelant (§2). Détail dans `docs/ARCHITECTURE.md` §2.9.

### 6.3 Les tokens

**`docs/DESIGN-TOKENS.md` fait autorité** sur les noms canoniques, les formules de dérivation,
l'échelle de teintes, les lanes de graphe, la densité et la typographie. Ne duplique pas ces
tables ici : lis ce document et implémente-le tel quel.

Trois points en méritent le rappel, parce qu'ils sont contre-intuitifs :

1. **`mix(A, B, p)` signifie p % de A**, le reste de B. Équivalent CSS :
   `color-mix(in srgb, A p%, B)`.
2. **Les coefficients de mélange sont des champs du thème, pas des constantes du code.**
   `muted_mix` vaut 65 % en sombre et 90 % en clair ; `dim_mix` 45 % et 66 %. Un coefficient
   figé produit un thème clair délavé. Idem pour `lane_lightness` et `lane_chroma`.
3. **La correction de contraste s'applique aussi aux couleurs lues du thème**, pas seulement
   aux couleurs dérivées. Rien ne garantit qu'un thème Omarchy fournisse un `green` lisible
   sur son propre fond.

### 6.4 Tests obligatoires du module

Les six tests qui font foi sont listés en §10 de `DESIGN-TOKENS.md`. Ils vivent dans
`layers-theme` et échouent le build s'ils cassent. Le plus important : **pour chaque thème
embarqué, toute paire texte/fond utilisée dans l'UI atteint un contraste ≥ 4.5:1**. C'est le
seul moyen de tenir la promesse d'accessibilité sur des palettes qu'on ne contrôle pas.

## 7. Organisation du workspace

```
layers/
├── Cargo.toml                    # workspace
├── rust-toolchain.toml
├── crates/
│   ├── layers-git/               # cœur Git — AUCUNE dépendance UI
│   │   ├── repo.rs               # ouverture, découverte, métadonnées
│   │   ├── status.rs             # working copy, index, untracked
│   │   ├── diff.rs               # hunks, lignes, raffinement intra-ligne
│   │   ├── history.rs            # parcours de commits, pagination
│   │   ├── graph.rs              # calcul des lanes (topologie, pas couleur)
│   │   ├── refs.rs               # branches, tags, remotes, ahead/behind
│   │   ├── stash.rs
│   │   ├── ops/                  # opérations mutantes
│   │   ├── credentials.rs        # délégation aux credential helpers
│   │   ├── paths.rs              # normalisation, encodage, casse (§9)
│   │   └── watch.rs              # notify + debounce + invalidation ciblée
│   ├── layers-theme/             # §6 — sources, tokens, dérivation, lanes
│   ├── layers-ui/                # composants applicatifs sur gpui-omarchy
│   │   ├── diff_view/            # le composant le plus lourd, isolé
│   │   ├── commit_graph/
│   │   ├── file_tree/
│   │   ├── palette/
│   │   └── primitives/
│   ├── layers-settings/          # config TOML, keymap, layout persisté
│   └── layers-app/
│       ├── platform/             # traits + impl linux / macos
│       └── ...                   # fenêtres, routage, état global, actions
├── vendor/gpui-omarchy/
├── assets/                       # icônes SVG, grammaires tree-sitter, thèmes embarqués
├── packaging/
│   ├── linux/                    # .desktop, icône hicolor, PKGBUILD AUR
│   └── macos/                    # Info.plist, bundle, signature, notarisation
├── docs/
│   ├── SPEC.md                   # ce document
│   ├── DESIGN.md                 # sortie de Claude Design — fait autorité sur l'UI
│   ├── DESIGN-TOKENS.md          # contrat de nommage — fait autorité sur les tokens
│   ├── ARCHITECTURE.md           # tu le maintiens au fil de l'eau
│   ├── KEYMAP.md
│   └── notes/                    # vérifications de capacités de crates
└── tests/fixtures/
```

## 8. Stratégie Git : hybride assumée

C'est la décision d'architecture la plus importante du projet.

**`gix` pour la lecture** : `status`, parcours d'historique, résolution de refs, lecture
d'objets, diff, blame. Rapide, pur Rust, sans dépendance C — ce qui simplifie considérablement
le packaging.

**Le binaire `git` en sous-processus pour les écritures complexes et le réseau** : `merge`,
`rebase`, `cherry-pick`, `revert`, `fetch`, `pull`, `push`, `clone`, résolution de conflits.
Trois raisons à documenter dans `ARCHITECTURE.md` :

- Ces opérations ont une sémantique subtile (hooks, stratégies de merge, `rerere`, config
  utilisateur) que réimplémenter serait une source de corruption de données.
- Les **hooks** de l'utilisateur (`pre-commit`, `commit-msg`) doivent s'exécuter. Un client Git
  qui les ignore casse les workflows d'équipe.
- L'authentification via les **credential helpers** existants (`libsecret`, `osxkeychain`)
  fonctionne alors sans rien réimplémenter.

**Implémentation** : un trait `GitBackend` dans `layers-git`, deux implémentations
(`GixBackend`, `CliBackend`), et un `HybridBackend` qui route. Ça permet de basculer une
opération d'un backend à l'autre si `gix` gagne en maturité, sans toucher à l'UI.

**À vérifier au jalon M2** avant de figer ce partage : l'état réel du support `gix` pour
`status`, le diff avec renommages, et le blame dans la version résolue. Documente le résultat.

**Sous-processus `git`, règles :**

- Toujours `--porcelain` / `-z` / `--no-color` quand un format machine existe.
- Toujours avec `GIT_TERMINAL_PROMPT=0`, `GIT_OPTIONAL_LOCKS=0`, environnement contrôlé.
- Toujours annulable : garder le `Child`, propager `SIGTERM` au groupe de processus.
- Toujours avec un timeout, `stderr` capturé et remonté tel quel à l'utilisateur.
- Détecter la version de `git` au démarrage, refuser sous 2.35 avec un message clair, et le
  dire explicitement si `git` est introuvable dans le `PATH`.

## 9. Spécificités par plateforme

Ce qui diverge passe derrière un trait dans `layers-app/src/platform/`.

| | Linux | macOS |
|---|---|---|
| Config | `$XDG_CONFIG_HOME/layers/` | `~/Library/Application Support/layers/` |
| Credential helper | `libsecret` | `osxkeychain` |
| Modificateur primaire | `Ctrl` | `Cmd` |
| Menus | aucun (tout dans l'app) | **barre de menus native obligatoire** |
| Décoration | CSD, Hyprland/Wayland | sans titre, feux tricolores incrustés (≈ 78px à gauche) |
| Surveillance FS | inotify | FSEvents |
| Ouvrir un fichier | `xdg-open` ou commande configurée | `open` |
| Révéler dans l'explorateur | selon le gestionnaire | Finder |
| Packaging | tar.gz + PKGBUILD AUR + `.desktop` | `.app` signé et notarisé + DMG |

**Linux** : Wayland en cible primaire, X11 en secours. Détecter l'absence de `libsecret` et le
signaler clairement plutôt que d'échouer silencieusement à l'authentification.

**macOS, cible de distribution — exigences fermes :**

- **Barre de menus native minimale** : App, Fichier, Édition, Affichage, Dépôt, Fenêtre, Aide.
  Sans elle, l'app paraît cassée.
- **Conventions d'édition de texte** : `Cmd+A`, `Cmd+Z`, `Option+←/→` par mot, `Cmd+←/→` en
  début et fin de ligne.
- **Défilement élastique** et scrollbars qui s'effacent.
- **Suivi de l'apparence système** claire/sombre, basculement automatique inclus.
- **Réglages d'accessibilité** : « Augmenter le contraste » et « Réduire les animations ».
- **APFS est insensible à la casse par défaut** alors que Git est sensible : un renommage
  `Readme.md` → `README.md` doit fonctionner. Centralise toute manipulation de chemin dans
  `layers-git/paths.rs`, jamais ad hoc ailleurs. Même vigilance sur les noms de fichiers
  non-UTF-8 et la normalisation Unicode (macOS décompose, Git non).
- **Distribution** : bundle `.app`, `Info.plist`, signature Developer ID, **notarisation**,
  stapling, DMG. Compte développeur Apple requis. Traité au jalon M10, mais le build macOS
  doit passer en CI dès M0.

## 10. Modèle de données et flux

```
┌─────────────┐   commande    ┌──────────────┐  spawn fond  ┌────────────┐
│  Composant  │──────────────▶│  RepoStore   │─────────────▶│ layers-git │
│    GPUI     │               │ Entity<Repo> │              │            │
│             │◀──────────────│              │◀─────────────│            │
└─────────────┘   cx.notify() └──────────────┘   résultat   └────────────┘
                                     ▲
                                     │ invalidation ciblée
                              ┌──────────────┐
                              │  FS watcher  │
                              └──────────────┘
```

- **`Entity<RepoStore>`** est la source de vérité par dépôt. Les composants l'observent via
  `cx.observe` et ne détiennent jamais d'état Git dupliqué — uniquement de l'état de vue
  (défilement, sélection, repliage).
- Chaque sous-état (`status`, `history`, `refs`, `stashes`) est un `AsyncState<T>` :
  `Idle | Loading | Ready(T, generation) | Failed(GitError)`. L'UI rend explicitement les
  quatre cas — pas de spinner global, pas d'écran blanc.
- Les résultats obsolètes sont **rejetés par numéro de génération**. Sans ça, tu auras des
  affichages incohérents dès que l'utilisateur enchaîne les actions.
- Le **watcher FS** debounce à 150ms, ignore `.git/index.lock` et les répertoires ignorés, et
  invalide **de façon ciblée** : une écriture dans `.git/refs/` n'invalide pas le statut de la
  copie de travail. Vérifie le comportement réel de `notify` sur inotify et sur FSEvents, ils
  ne rapportent pas les mêmes événements.
- Toutes les opérations mutantes passent par une **file sérialisée par dépôt**. Deux commandes
  Git écrivantes ne doivent jamais tourner en parallèle sur le même dépôt.

## 11. Périmètre fonctionnel

### MVP — à livrer intégralement

**Repositories** — ajouter un dépôt local, cloner avec progression, retirer. Groupes
utilisateur repliables, réordonnables par drag & drop, persistés. Fiche dépôt complète.
Détection du dépôt introuvable ou déplacé.

**Working Copy** — statut complet (modifié, ajouté, supprimé, renommé, non suivi, ignoré, en
conflit). Indexation **par fichier, par hunk et par ligne** — la sélection de lignes est un
vrai différenciateur, ne la reporte pas. Discard aux trois granularités avec confirmation.
Commit, amend, sign-off, `--no-verify` en option explicite. Détection de l'identité du
committer avec avertissement si absente. Support de `commit.template`.

**History** — parcours paginé et virtualisé, 100 000 commits sans ralentissement. Graphe
multi-lanes calculé en incrémental sur le thread de fond. Filtres par branche, auteur, chemin,
plage de dates, texte. Détail de commit avec parents cliquables, fichiers à diff dépliable,
vue arborescence. Comparaison entre deux commits.

**Diff** — unifié et côte à côte, raffinement intra-ligne mot à mot, coloration syntaxique
tree-sitter, contexte réglable, affichage des espaces, détection des fins de ligne. Cas
particuliers : binaire, image (avant/après), renommage, fichier volumineux (affichage dégradé
au-delà d'un seuil plutôt qu'un gel).

**Branches, remotes, tags, stashes** — créer, basculer, renommer, supprimer, fusionner,
rebaser. Regroupement automatique par `/`. Ahead/behind rafraîchi. Fetch, pull (merge et
rebase), push, **push force-with-lease uniquement, jamais `--force` nu**. Tags légers et
annotés. Stash avec ou sans untracked, apply, pop, drop, aperçu du diff.

**Conflits** — détection, liste, affichage des trois versions, résolution « ours / theirs » par
fichier et par hunk, ouverture dans l'éditeur configuré, poursuite ou abandon de l'opération.

**Transverse** — command palette, navigation clavier intégrale, keymap réassignable, système de
thème complet (§6), mode compact/confortable, journal des opérations Git avec la commande exacte.

### Hors MVP

Pull requests GitHub/GitLab · rebase interactif graphique · blame · submodules · Git LFS ·
worktrees · bisect · reflog · génération de message de commit par IA · signature GPG/SSH.

Point d'extension identifiable, **aucun code mort** (§2).

## 12. Performance — objectifs mesurés

Benchmarks reproductibles dans `benches/`, sur dépôts générés, mesurés sur Linux et macOS.

| Scénario | Cible |
|---|---|
| Ouverture d'un dépôt de 50 000 fichiers, premier rendu | < 400 ms |
| `status` sur 50 000 fichiers | < 300 ms |
| 1 000 premiers commits sur un historique de 100 000 | < 250 ms |
| Diff d'un fichier de 5 000 lignes, rendu complet | < 100 ms |
| Défilement de l'historique | 120 fps constants, aucune image sautée |
| Mémoire au repos, dépôt moyen | < 250 Mo |
| Démarrage à froid jusqu'à l'écran Repositories | < 250 ms |

**Techniques imposées** : virtualisation de toutes les listes longues, diff calculé hunk par
hunk à la demande, cache LRU des objets Git, coloration syntaxique sur la plage visible
uniquement, graphe recalculé en différentiel.

## 13. Tests

- **Unitaires** dans `layers-git` : parsing de sortie `git`, calcul de lanes, découpage en
  hunks, raffinement intra-ligne, normalisation de chemins. Les fonctions les plus faciles à
  casser silencieusement.
- **Unitaires** dans `layers-theme` : voir §6.5.
- **Intégration** : un helper `TestRepo` qui construit un dépôt temporaire avec une histoire
  scriptée — merges, conflits, renommages, binaires, sous-modules, noms non-UTF-8, espaces,
  accents, et un renommage qui ne change que la casse.
- **Cas limites obligatoires** : dépôt vide sans commit, HEAD détaché, rebase en cours, merge en
  cours, dépôt bare, montage réseau lent, fichier de 100 Mo, ligne de 1 Mo sans retour,
  historique à 50 racines.
- **UI** : tests d'interaction sur les composants critiques (indexation par ligne, navigation
  clavier de la palette) avec le support de test de GPUI.
- **Snapshots** de rendu de diff, pour détecter les régressions visuelles.
- **CI Linux et macOS** : `fmt` + `clippy -D warnings` + `test` + `cargo deny` + build release.

## 14. Jalons

Un jalon à la fois, chacun terminé par une PR validée.

- **M0 — Squelette.** Workspace, vendoring de `gpui-omarchy`, fenêtre décorée qui démarre et
  applique un thème sur Linux et macOS, CI verte sur les deux, `docs/ARCHITECTURE.md` initial
  et `docs/notes/` avec la vérification des capacités des crates.
- **M1 — Thème.** `layers-theme` complet, implémentant `DESIGN-TOKENS.md` intégralement : les
  4 sources, les tokens sous leurs noms canoniques, les coefficients paramétrables par thème,
  la dérivation OKLCH avec correction de contraste, l'échelle de teintes, les lanes, la
  densité, le catalogue embarqué (6 sombres + 2 clairs), le suivi live d'Omarchy, le suivi de
  l'apparence macOS, et les six tests qui font foi.
- **M2 — Cœur Git en lecture.** `layers-git` : ouverture, statut, refs, historique paginé, diff
  avec raffinement. Aucune UI. Une CLI de debug `layers-git-cli` pour tout valider. Tests
  d'intégration complets.
- **M3 — Écran Repositories.** Liste, groupes, ajout, fiche, persistance. Premier écran
  navigable au clavier de bout en bout, conforme à `docs/DESIGN.md`.
- **M4 — Working Copy en lecture.** Liste des fichiers, visionneuse de diff complète (unifié +
  split, syntaxe, raffinement, virtualisation). Pas encore d'écriture.
- **M5 — Écriture.** Indexation fichier/hunk/ligne, commit, amend, discard, file sérialisée,
  journal des commandes.
- **M6 — History.** Liste virtualisée, graphe de lanes, détail de commit, filtres.
- **M7 — Branches et réseau.** Sidebar complète, checkout, création, merge, rebase, fetch,
  pull, push force-with-lease, credential helpers, overlay de progression annulable.
- **M8 — Stashes et conflits.** Résolution, poursuite ou abandon d'opération.
- **M9 — Finition.** Command palette, keymap réassignable, préférences, densités, feuille de
  raccourcis, états vides et d'erreur partout, menus natifs macOS.
- **M10 — Distribution.** Bundle macOS signé et notarisé + DMG, archive Linux + PKGBUILD AUR,
  mise à jour in-app optionnelle, pipeline de release GitHub Actions avec attestation.

## 15. Risques à surveiller

Consigne-les dans `docs/ARCHITECTURE.md` et réévalue-les à chaque jalon.

1. **Le thread UI qui bloque.** Mode de défaillance le plus probable. Ajoute dès M0 une
   assertion en debug qui panique si une opération Git est appelée depuis le thread UI.
2. **Le partage `gix` / CLI mal placé.** Si `gix` ne couvre pas un cas de lecture (renommages,
   sous-modules, `.gitattributes`), bascule sur la CLI plutôt que de bricoler, et documente-le.
3. **`gpui-omarchy` incomplet.** Certains composants n'existent pas encore. Compose directement
   avec `gpui_kit::base` et remonte le manque en amont. Le vendoring (§5) existe pour ça.
4. **Le graphe de commits.** L'algorithme le plus difficile du projet. Isole-le, teste-le sur
   des historiques pathologiques, ne le couple jamais au rendu.
5. **La perte de données.** Toute commande destructive est journalisée avec sa ligne de commande
   exacte **avant** exécution. En cas de doute sur une sémantique Git, délègue à la CLI `git`.
6. **Le coût de distribution macOS.** Compte développeur Apple, signature, notarisation, runner
   CI macOS. Coût récurrent à budgéter dès maintenant, pas au jalon M10.
