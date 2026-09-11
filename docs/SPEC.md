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

> **Amendement, entre M6 et M7 (2026-09-09) — Windows n'est plus exclu par
> construction, et n'est pas pour autant supporté.**
>
> Cette section excluait Windows en partie parce que le rendre possible aurait
> coûté des compromis d'architecture. Avec Tauri (§4) il n'en coûte plus aucun :
> le runtime y tourne. Deux choses restent distinctes, et les confondre serait
> exactement le genre de dette que cette section interdit :
>
> * **« Ça tournerait »** — vrai, gratuit, et une propriété du runtime.
> * **« C'est supporté »** — rouvre la variante Windows de la maquette 02 (les
>   boutons caption 46×48, la réserve de 138px, la zone `HTMAXBUTTON` pour les
>   Snap Layouts), le packaging, les tests, une troisième cible de CI.
>
> **Le second n'est pas décidé.** Tant qu'il ne l'est pas, la règle de cette
> section tient sous une forme plus faible : aucun compromis n'est fait *pour*
> Windows, rien n'est testé ni packagé pour lui, et un bug qui ne s'y produit que
> là n'est pas un bug de ce projet. Le jour où quelqu'un décide de le supporter,
> c'est un jalon, pas une case à cocher.

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

> **Amendement, après M4 (2026-09-08) — ni CI, ni PR, tant que le projet est
> jeune et développé par une seule personne.**
>
> La règle disait « un jalon = une PR », vérifiée par GitHub Actions sur les
> deux plateformes à chaque push. Les deux moitiés supposent un lecteur qui
> n'existe pas : sur un dépôt **privé** développé par **une seule personne**, une
> PR n'est relue par personne, et un gate ne vérifie le travail de personne
> d'autre. Un push qui casse la compilation est découvert par celui qui l'a
> écrit, qui le savait déjà. Le coût, lui, était réel : macOS est facturé ×10, et
> le seul run de la PR #1 a consommé environ 113 minutes.
>
> **Complément (2026-09-10) — une release n'est pas un gate.** Un workflow
> déclenché par un tag construit les paquets Linux et les attache à une release
> en brouillon (`.github/workflows/release.yml`). Il ne vérifie rien : il
> *fabrique* l'artefact que quelqu'un d'autre installera, depuis un checkout
> propre. La seule chose qu'il contrôle est celle qu'un clavier ne peut pas
> contrôler — que le tag et les manifestes disent la même version. C'est la
> partie Linux de M10, tirée en avant ; macOS attend la signature et la
> notarisation.
>
> Donc, jusqu'à nouvel ordre :
>
> - **Pas de CI.** Le workflow est supprimé (`.github/workflows/ci.yml`, dernier
>   état au commit `08193f5`, à restaurer tel quel le jour où il servira).
> - **Pas de PR.** On commite et on pousse sur `main`. Un jalon reste une unité
>   de travail cohérente, et son commit de tête le dit ; il n'a simplement plus
>   d'enveloppe de revue autour.
> - **Ce qui reste** : `scripts/check.sh`, lancé quand on le décide.
>
> Ce que ça coûte, écrit noir sur blanc plutôt que passé sous silence : plus
> personne ne compile l'autre plateforme. `scripts/check.sh` ne compile que la
> moitié de `omagit-app/src/platform/` correspondant à la machine hôte, et la
> moitié macOS de `status` (repli de casse APFS, noms décomposés) n'est plus
> exercée nulle part. Ces deux angles morts ont déjà produit deux bugs réels
> (`docs/ARCHITECTURE.md` §5). Le projet se développant sur les deux machines,
> ils seront vus au prochain passage sur l'autre — plus tard, et c'est le
> marché accepté ici.
>
> **À rétablir si un second développeur rejoint le projet, ou quand le projet
> sera à un stade avancé** : la règle redevient alors ce qu'elle dit, parce
> qu'il y a enfin du travail à relire et à vérifier qui n'est pas le sien.
9. **Écris les tests avec le code.** Chaque commande Git implémentée arrive avec au moins un
   test d'intégration sur un dépôt temporaire.

> **Amendement, entre M6 et M7 (2026-09-09) — les deux règles qui nomment GPUI.**
>
> La **règle 1** cite `gpui-kit` et `gpui-omarchy` comme les dépendances dont les
> versions bougent vite. Elles sortent du projet (§4) ; la règle vaut désormais
> pour **Tauri, son webview et `gitoxide`**. Elle n'a rien perdu de sa force :
> c'est en l'appliquant qu'on a découvert que `gpui-omarchy` était vieux de deux
> jours et maintenu par une personne, et qu'on l'a vendorisé avant qu'il ne
> coûte cher.
>
> La **règle 5** dit que le cœur Git ne dépend ni de `gpui` ni de
> `gpui-omarchy`. Elle devient : il ne dépend **d'aucune couche d'interface**,
> Tauri compris. C'est la règle qui a rendu ce changement de pile envisageable —
> 8 300 lignes et 3 959 lignes de tests traversent intactes. Le prix de la tenir
> était nul ; le prix de ne pas l'avoir tenue aurait été le projet.
>
> Les règles 2, 3, 4, 6, 7, 8 et 9 sont inchangées. La règle 2 (aucun blocage du
> thread UI) change seulement de thread à ne pas bloquer.

## 4. Pile technique

| Couche | Choix | Note |
|---|---|---|
| Langage | Rust, édition 2024, toolchain épinglée dans `rust-toolchain.toml` | |
| UI | `gpui-kit` (embarque GPUI via les snapshots `gpui-pre-*`) | ne dépend jamais de `gpui` directement |
| Design system | `gpui-omarchy` | MIT — **à vendorer, voir §5** |
| Git (lecture) | `gix` (gitoxide) | rapide, pur Rust, pas de dépendance C |
| Git (écriture / réseau) | binaire `git` en sous-processus | choix délibéré, voir §8 |
| Surveillance FS | `notify` + debounce maison | |
| Coloration syntaxique | `tree-sitter` + grammaires | grammaires embarquées par omagit — voir l'amendement §4 |
| Diff | `imara-diff` ou `gix-diff` | Myers + raffinement mot à mot |
| Couleur | `palette` ou équivalent, avec support OKLCH | indispensable pour §6 |
| Async | l'executor de GPUI (`background_spawn`) | **pas** de runtime Tokio global |
| Erreurs | `thiserror` dans les crates, `anyhow` dans le binaire | |
| Logs | `tracing` + `tracing-subscriber`, fichier tournant | |
| Config | `serde` + TOML | emplacement par OS, voir §9 |

> **Amendement, jalon M4 (2026-09-08) — « GPUI l'embarque déjà » est faux ici.**
>
> La note de la ligne « Coloration syntaxique » disait que GPUI embarque
> tree-sitter. `gpui-kit` le propose bien, mais **derrière `gpui-component`**,
> que ce workspace n'active pas : `gpui-omarchy` en dépend avec
> `default-features = false`, et l'activer tirerait une seconde bibliothèque de
> composants concurrente de celle qu'on a vendorée (§5). La coloration a donc
> ses propres grammaires : `tree-sitter` plus une grammaire par langage
> réellement affiché — Rust, TOML, JSON, Markdown — et rien de plus, chacune
> étant une compilation C. Un fichier sans grammaire s'affiche en clair, ce qui
> reste lisible : la hiérarchie ne repose jamais sur la teinte (DESIGN §1).
>
> Détail dans `docs/ARCHITECTURE.md` §2.18 et `docs/notes/gpui-kit-capabilities.md`.

> **Amendement, entre M6 et M7 (2026-09-09) — l'UI passe de GPUI à Tauri.**
>
> La ligne « UI » de ce tableau devient :
>
> | Couche | Choix | Note |
> |---|---|---|
> | UI | **Tauri 2** — backend Rust, interface web | remplace `gpui-kit` et `gpui-omarchy` |
> | Design system | **le nôtre, en CSS** | les maquettes livrées sont déjà du HTML |
> | Async | **l'executor de Tauri** | toujours **pas** de runtime Tokio global exposé aux crates métier |
>
> Tout le reste du tableau est inchangé : `gix`, le binaire `git`, `notify`,
> `imara-diff`, `palette`, `thiserror`, `tracing`, `serde`. Le cœur Git ne
> dépendait d'aucun d'entre eux (§3 règle 5), et c'est précisément ce qui rend
> ce changement supportable.
>
> **La raison, et elle est unique.** GPUI n'est pas un produit destiné à des
> tiers : il existe pour servir Zed. Son API bouge quand Zed a besoin qu'elle
> bouge — le préfixe `gpui-pre-*` le dit à voix haute — et chaque refactor de
> Zed serait notre migration, sans qu'ils nous doivent rien. Tauri existe pour
> que des tiers construisent dessus ; ses ruptures sont annoncées et
> documentées. Sur un projet de plusieurs mois, c'est la différence entre une
> dépendance et un pari.
>
> **Ce qui n'a *pas* motivé la décision**, parce qu'on l'a mesuré et que c'était
> faux : le coût de la frontière IPC. Sur un dépôt réel de 739 commits, une page
> d'historique complète avec son graphe pèse 0,55 Mo et traverse en 17 ms, les
> 60 lignes visibles en 1,5 ms, le plus gros diff du dépôt en 4 ms. Face aux
> budgets de §12 c'est moins de 10 %. L'argument performance ne tenait pas.
>
> **Tauri plutôt qu'Electron**, malgré les trois moteurs de rendu : le backend
> d'une app Tauri *est* du Rust, donc `omagit-git` — 8 300 lignes sans
> dépendance UI et 3 959 lignes de tests — reste tel quel. Electron obligerait à
> le réécrire ou à ajouter une frontière de processus.
>
> **Ce que ça coûte, sans l'enjoliver.** Environ 10 000 lignes d'interface à
> réécrire, et surtout la perte du harnais de tests d'interaction de GPUI
> (`TestAppContext`, `VisualTestContext`), qui pilote le vrai arbre de widgets
> et a attrapé quatre bugs réels en une session. Il faudra le remplacer, et rien
> côté web n'est aussi direct.
>
> **Ce qui reste inconnu et que le spike doit trancher** : Tauri utilise le
> webview du système, donc **WebKitGTK sur Linux** — la cible de conception
> (§1), et le plus faible des trois moteurs. Le spike rend 1 000 lignes de diff
> virtualisées avec coloration syntaxique et la gouttière du graphe, **sur
> Linux**. Si ça tient là, ça tient partout.

## 5. ~~Vendorer `gpui-omarchy` dès M0~~ *(caduc — voir l'amendement en fin de section)*

`gpui-omarchy` est en 0.1.0, maintenu par une seule personne, avec `gpui-kit = "=0.6.0"`
épinglé en dur, et son README indique explicitement que le framework n'est pas complet.

**Action au jalon M0** : copier le crate dans `vendor/gpui-omarchy/` avec sa licence MIT et son
commit d'origine noté dans `vendor/README.md`, le référencer par `path`, et créer
`scripts/sync-vendor.sh` qui diffe l'upstream et signale les évolutions.

Rationale à consigner dans le message de commit : on ne veut pas qu'un upstream de 7 étoiles bloque un projet
de plusieurs mois. On contribue les correctifs génériques en amont, on garde les divergences
produit en local.

Vérifie aussi dès M0 qu'il se comporte correctement **sur macOS** : absence totale d'Omarchy →
repli sur thème embarqué, sans panique ni chemin Linux codé en dur.

> **Amendement, entre M6 et M7 (2026-09-09) — cette section est caduque.**
>
> `gpui-omarchy` disparaît avec GPUI (§4), et avec lui `vendor/` et
> `scripts/sync-vendor.sh`. La section est conservée pour ce qu'elle a montré :
> le vendoring a fonctionné exactement comme prévu — un upstream d'une personne
> et de deux jours n'a jamais bloqué le projet, et son remplacement complet du
> système de thème a coûté une session, pas un jalon. Le raisonnement reste
> valable pour la prochaine dépendance fragile qu'on rencontrera.

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

> **Amendement, entre M6 et M7 (2026-09-09) — le système de thème survit intact.**
>
> `omagit-theme` n'a aucune dépendance UI (§3 règle 5), donc rien de cette
> section ne change : les quatre sources, la dérivation OKLCH, la correction de
> contraste, les lanes, le catalogue et les six tests qui font foi restent tels
> quels. Seule sa *projection* change de cible : au lieu de remplir la structure
> de thème de `gpui-omarchy`, elle émettra des variables CSS.
>
> C'est même le sens de la marche : `DESIGN-TOKENS.md` décrit ses tokens avec une
> colonne « CSS maquettes » (`--fg`, `--ac`, `--raised`) présentée comme un
> instantané de livraison. Elle redevient la représentation réelle.

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

> **Amendement, entre M6 et M7 (2026-09-09) — ce que devient le workspace.**
>
> Survivent tels quels, parce qu'aucun ne dépend de l'UI : `omagit-git`,
> `omagit-theme`, `omagit-settings`, et `omagit-git-cli`.
>
> Disparaissent : `omagit-ui` et les écrans de `omagit-app`, soit environ 10 000
> lignes. `vendor/` disparaît avec eux.
>
> Apparaissent : une interface web sous `web/` — les maquettes livrées sont déjà
> du HTML — et `omagit-app` devient le binaire Tauri, qui expose le cœur Git aux
> commandes et garde la couche `platform/`, dont la raison d'être (Linux et
> macOS divergent) n'a pas changé.
>
> Ce qui vaut d'être noté : cette liste est courte parce que la règle 5 de §3 a
> été tenue. Un cœur Git qui aurait connu l'UI aurait rendu ce changement
> impossible à envisager.

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

### 9.1 Langue de l'interface

> **Amendement (2026-09-10).** La langue n'était spécifiée nulle part, et
> l'interface était écrite en français en dur. Elle est bilingue à partir
> d'ici : **anglais par défaut**, français à côté.

- La langue est celle du système au premier lancement, et se change dans
  Réglages. Le choix est stocké dans `settings.toml` ; `None` veut dire « suivre
  le système ».
- **Ajouter une langue, c'est ajouter un fichier** dans
  `web/src/i18n/languages/`. Rien ne les importe par leur nom.
- L'anglais est la langue de référence : les autres catalogues sont typés contre
  lui, et une clé absente d'une traduction est une erreur de compilation.
- **Le backend ne formule pas.** Ce qu'une commande renvoie est soit les mots de
  `git`, montrés tels quels (§3 règle 3), soit une clé marquée `omagit:` que la
  fenêtre habille. `omagit-git` n'a pas de locale, par règle : il envoie des
  secondes et des codes.
- Les mots de `git` restent dans la langue de `git`. Les traduire serait
  inventer.

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
worktrees · bisect · ~~génération de message de commit par IA~~ *(amendé — voir ci-dessous)* ·
reflog · signature GPG/SSH.

Point d'extension identifiable, **aucun code mort** (§2).

#### Amendement (2026-09-11) — la génération de message de commit entre

Elle était hors MVP parce qu'elle était chère : une clef d'API par fournisseur,
saisie dans les réglages, donc un secret à stocker — trousseau système ou
fichier en clair, les deux avec leur coût — un client HTTP et une pile TLS dans
l'arbre de dépendances, et une API par fournisseur à suivre.

Aucun de ces coûts n'existe dans la forme retenue. omagit **s'adresse à un agent
de code déjà installé sur la machine** — `claude`, `codex`, `gemini`, ou une
commande que l'utilisateur nomme lui-même. Il n'y a pas de secret à stocker :
l'agent porte déjà les identifiants de son utilisateur. Pas de client HTTP :
c'est un processus, lancé par le lanceur qui lance déjà `git`, sous les règles 2
à 5 de §8 — environnement contrôlé, groupe de processus signalable, délai,
`stderr` verbatim. Ce qui est configuré est le **nom d'un programme**, et
`settings.toml` reste un fichier ordinaire que personne n'a besoin de protéger.

Ce qui part : le diff indexé, tronqué au-delà de 120 Ko, et la commande exacte
apparaît dans le journal comme toute commande qu'omagit lance. L'agent est
démarré **dans le dépôt**, donc il lit le `CLAUDE.md` ou l'`AGENTS.md` du projet
et suit les conventions qui y sont écrites — ce qu'une clef d'API avec un diff
nu ne peut pas faire.

Le reste de la liste ci-dessus est inchangé.

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

> **Amendement, entre M6 et M7 (2026-09-09) — les cibles tiennent, deux sont à
> re-mesurer.**
>
> Les sept objectifs restent les objectifs : ils décrivent le produit voulu, pas
> la technologie. La mesure IPC (§4) montre que la frontière en consomme moins de
> 10 %. Deux lignes sont à re-mesurer une fois le spike fait, parce qu'un webview
> a un coût de base que GPUI n'avait pas :
>
> * **Mémoire au repos < 250 Mo** — WebKitGTK et WKWebView partent plus haut que zéro.
> * **Démarrage à froid < 250 ms** — l'initialisation du webview s'ajoute.
>
> Si l'une des deux ne tient pas, c'est la cible qu'on rediscute en la nommant,
> pas le chiffre qu'on oublie.

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

> **Amendement, entre M6 et M7 (2026-09-09) — le harnais d'interaction est à
> refaire.**
>
> Les tests unitaires, d'intégration et les cas limites de cette section ne
> bougent pas : ils vivent dans `omagit-git` et `omagit-theme`, qui ne changent
> pas.
>
> La ligne « **UI** : tests d'interaction sur les composants critiques avec le
> support de test de GPUI » perd son outil. `TestAppContext` et
> `VisualTestContext` pilotaient le vrai arbre de widgets — frappes, clics,
> bornes mesurées — et ont attrapé quatre bugs réels en une session. Rien côté
> web n'est aussi direct ; il faudra choisir un remplaçant (WebDriver, Playwright)
> et le choisir tôt, parce que c'est par ce trou que sont passés tous les bugs
> d'interface de ce projet.
>
> Les **snapshots de rendu de diff** deviennent plus faciles, en revanche : du
> DOM se compare mieux qu'une liste d'éléments.

## 14. Jalons

Un jalon à la fois, chacun terminé par `scripts/check.sh` vert et poussé sur `main`
(§3 règle 8 et son amendement : pas de PR à ce stade).

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
> **Amendement (2026-09-11) — le modèle de distribution est arrêté.**
>
> omagit est **open source sous GPL-3.0-only**. La licence n'est pas un détail
> administratif ici : le `Cargo.toml` annonçait MIT, qui autorise un fork fermé —
> exactement ce qu'il ne faut pas quand une plateforme est vendue.
>
> - **Linux est gratuit** : `.deb` et `.AppImage` sur les releases GitHub,
>   construits par `.github/workflows/release.yml`.
> - **macOS est payant**, et ce qui est vendu est le *build* — signé, notarisé,
>   qui se met à jour —, jamais le droit d'usage. La GPL le permet, et c'est ce
>   qui rend l'offre honnête : compiler soi-même reste possible, redistribuer sa
>   copie reste permis, et personne ne peut refermer le code.
> - Le **Mac App Store est exclu** : son bac à sable interdit de lancer un binaire
>   externe, et §8 fait de `git` le seul outil d'écriture. Vente directe et
>   notarisation, donc, avec le *Merchant of Record* qui porte la TVA.
> - Le build macOS n'est jamais attaché à une release publique. Les certificats
>   vivent dans les secrets du dépôt, que GitHub ne donne pas aux forks.

- **M10 — Distribution.** Bundle macOS signé et notarisé + DMG, archive Linux + PKGBUILD AUR,
  mise à jour in-app optionnelle, pipeline de release GitHub Actions avec attestation.

> **Amendement, entre M6 et M7 (2026-09-09) — un jalon s'insère, M6 est
> interrompu.**
>
> **M6 s'arrête où il en est** : le graphe, la liste virtualisée et le détail de
> commit sont faits ; les filtres et la comparaison A ↔ B ne seront **pas**
> écrits en GPUI. Les écrire pour les jeter n'a aucun sens, et le travail déjà
> fait ne doit pas peser dans la balance — c'est ce qui a fondé la décision.
>
> **M6b — Portage.** Un spike Tauri d'abord, sur Linux et WebKitGTK, qui répond
> à la seule question ouverte : 1 000 lignes de diff virtualisées avec
> coloration, et la gouttière du graphe. S'il tient, le portage de M3 à M6 ;
> sinon on rouvre la décision avec une mesure en main plutôt qu'un avis.
>
> M6 se termine ensuite dans la nouvelle interface — les filtres et A ↔ B — et
> M7 à M10 sont inchangés dans leur contenu. La seule ligne qui bouge est celle
> de M10 : le packaging devient celui de Tauri, et Windows y entrerait *si* et
> seulement si quelqu'un décide de le supporter (§2).

## 15. Risques à surveiller

Consigne-les dans `docs/ARCHITECTURE.md` et réévalue-les à chaque jalon.

1. **Le thread UI qui bloque.** Mode de défaillance le plus probable. Ajoute dès M0 une
   assertion en debug qui panique si une opération Git est appelée depuis le thread UI.
2. **Le partage `gix` / CLI mal placé.** Si `gix` ne couvre pas un cas de lecture (renommages,
   sous-modules, `.gitattributes`), bascule sur la CLI plutôt que de bricoler, et documente-le.
3. **~~`gpui-omarchy` incomplet~~ — remplacé (2026-09-09).** Ce risque s'est réalisé et a été
   traité comme prévu : vendoring, puis remplacement complet du système de thème. Il est
   remplacé par **le webview de Tauri** : trois moteurs au lieu d'un, dont WebKitGTK sur la
   cible de conception. C'est le risque que le spike de M6b existe pour chiffrer. À sa suite,
   deux autres à surveiller : la frontière IPC sur les gros diffs — mesurée à moins de 10 % du
   budget, à re-mesurer sur un vrai rendu — et la disparition du harnais de tests d'interaction,
   par lequel sont passés tous les bugs d'interface de ce projet.

   *(Texte d'origine, conservé.)* Certains composants n'existent pas encore. Compose directement
   avec `gpui_kit::base` et remonte le manque en amont. Le vendoring (§5) existe pour ça.
4. **Le graphe de commits.** L'algorithme le plus difficile du projet. Isole-le, teste-le sur
   des historiques pathologiques, ne le couple jamais au rendu.
5. **La perte de données.** Toute commande destructive est journalisée avec sa ligne de commande
   exacte **avant** exécution. En cas de doute sur une sémantique Git, délègue à la CLI `git`.
6. **Le coût de distribution macOS.** Compte développeur Apple, signature, notarisation, runner
   CI macOS. Coût récurrent à budgéter dès maintenant, pas au jalon M10.
