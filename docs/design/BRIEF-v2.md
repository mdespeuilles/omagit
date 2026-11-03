# Brief Claude Design — omagit v2

## Le contexte

omagit est un client Git de bureau (Tauri 2, frontend Vue, Linux/Hyprland et
macOS). Une première passe de design existe : les neuf planches de ce dossier,
et les captures de `docs/screenshots/`. Elle est juste, mais l'app reste trop
**brute** : tout en monospace, angles droits partout, des listes denses et
aucune vue d'ensemble.

Le défaut principal est un défaut de **lecture**, pas seulement d'esthétique.
Quand on ouvre un dépôt, on n'arrive pas à répondre vite à ces questions :

1. Sur quelle branche suis-je ?
2. Où en suis-je par rapport au remote ? (à pousser, à tirer, divergé, pas de
   branche distante, et depuis quand ce n'est pas vérifié)
3. Qu'est-ce qui est en cours dans ma copie de travail ?
4. Quelles branches existent, qui a été mergé où, et dans quelle branche se
   trouve tel commit ?

## Ce qu'on demande

### 1. Une vue d'ensemble du dépôt (le nouvel écran d'accueil d'un dépôt)

- **L'état en une phrase et un visuel** : « Tu es sur `main` · 3 commits à
  pousser · 1 à récupérer · vérifié il y a 4 min ». Les états à dessiner : à
  jour, en avance, en retard, divergé, sans branche distante, HEAD détaché,
  opération en cours (rebase, merge, cherry-pick), remote injoignable.
- **Le lien local ↔ remote dessiné** : deux pastilles (`main` et
  `origin/main`) reliées, avec les commits d'écart de chaque côté. L'action
  logique (Pousser, Tirer, Rebaser) est proposée directement sur ce dessin.
- **Copie de travail résumée** : modifiés / indexés / non suivis / stash, avec
  un accès direct à l'écran de commit.
- **Branches actives** : les 5–8 branches récentes, chacune avec son écart par
  rapport à `main` et à son upstream, son statut (mergée, à jour, en retard),
  sa dernière activité et ses auteurs.

### 2. Une carte des branches simplifiée

En plus du graphe complet existant (planche 05), une vue « métro » : une ligne
horizontale par branche, avec un nom et une couleur stables. On y voit les
points de départ, les merges et les têtes de branche. Les suites linéaires
sont repliées (« 12 commits »). C'est fait pour comprendre la structure d'un
coup d'œil, pas pour lire chaque commit.

- **Chaque branche garde sa couleur partout** : dans la carte, le graphe, les
  pastilles, la barre du haut.
- **Le détail d'un commit dit où il se trouve** : « présent dans `main`,
  `release/0.2`, `origin/main` — pas encore dans `develop` », plus les tags
  qui le contiennent.

### 3. Un statut toujours visible

Une pastille de branche dans la barre du haut, à la place du chemin en petit
texte : nom de la branche, flèches ↑3 ↓1 colorées selon l'état, et un point
qui signale une copie de travail sale. Au clic, elle ouvre un sélecteur de
branche avec recherche.

### 4. Le tableau de bord des dépôts

L'écran Dépôts devient une liste ou une grille qui met en avant **ce qui
demande de l'attention** : dépôts sales, en retard, divergés, pas fetchés
depuis longtemps. On peut filtrer et trier par activité.

### 5. Cloner depuis GitHub (et plus tard GitLab auto-hébergé)

- **Un compte par fournisseur** : GitHub par jeton (réutiliser `gh auth
  token` s'il existe) ; GitLab par URL d'instance et jeton.
- **Une palette de clonage** : recherche floue instantanée sur ~200 dépôts
  (liste en cache local, rafraîchie en arrière-plan). Filtres : propriétaire
  ou organisation, forks, archivés, langage, activité récente.
- **Chaque ligne** montre le nom, le propriétaire, la visibilité, le langage,
  la date du dernier push, et un badge « déjà cloné » qui ouvre le dépôt au
  lieu de le recloner.
- **Entrée clone** dans le dossier par défaut. Le formulaire actuel (URL,
  destination, groupe) reste accessible en mode avancé.
- **États à dessiner** : non connecté, chargement, liste vide, erreur de
  jeton, clonage en cours.

## Direction visuelle

- **Typographie** : une vraie sans-serif pour l'interface (par exemple Inter,
  Geist ou IBM Plex Sans), embarquée dans l'app. La monospace (JetBrains Mono
  ou Geist Mono) est réservée au Git littéral : hashes, diff, chemins dans le
  diff.
- **Moins brut** : rayons légers (6–8 px), surfaces plus douces, plus d'air,
  avatars d'auteurs, couleurs de branches. L'interface doit rester dense et
  sobre : c'est un outil de développeur, pas un tableau de bord marketing.
- **À garder** :
  - la hiérarchie lisible sans couleur (le test en niveaux de gris de la
    planche 01) ;
  - les quatre palettes (Tokyo Night, Gruvbox, Matte Black, clair) et les deux
    densités ;
  - le clavier d'abord (planche 09), la différence entre sélection et focus,
    la palette de commandes ;
  - les contraintes de chrome Linux/macOS (planche 02).
- **Règles de `DESIGN.md` qui changent** : « Square, always » (§1) et l'usage
  étendu de la monospace (DESIGN-TOKENS §8). Le nouveau design devra les
  amender explicitement.

## Livrables attendus

Les planches, aux mêmes formats que la v1 :

- tokens et composants révisés ;
- vue d'ensemble du dépôt ;
- carte des branches ;
- pastille de statut et sélecteur de branche ;
- tableau de bord des dépôts ;
- palette de clonage GitHub/GitLab ;
- History et Working Copy restylés.

Chaque planche est livrée sur les quatre palettes.
