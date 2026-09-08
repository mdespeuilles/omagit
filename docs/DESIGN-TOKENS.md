# DESIGN-TOKENS.md — Contrat de nommage entre le design et le code

> Document de référence unique pour les tokens de thème. Il fait autorité sur les
> **noms** et les **formules**. `DESIGN.md` fait autorité sur l'usage visuel,
> `SPEC.md` sur l'implémentation.
>
> Toute divergence entre les trois documents se résout en faveur de celui-ci pour les tokens.

---

## 1. Règle de nommage

**Les noms canoniques sont les noms longs**, ceux qui apparaîtront dans le code Rust
(`theme.text_muted`). Les maquettes HTML livrées par Claude Design utilisent des abréviations
CSS (`--muted`) : elles sont un instantané de livraison, pas la source de vérité.

`layers-theme` expose une structure dont les champs portent les noms canoniques. Aucun
composant n'utilise d'autre nom.

## 2. Entrées

### 2.1 Garanties — les seules obligatoires

| Nom canonique | Clé Omarchy | CSS maquettes |
|---|---|---|
| `input.background` | `background` | `--bg` |
| `input.foreground` | `foreground` | `--fg` |
| `input.accent` | `accent` | `--ac` |
| `input.selection` | `selection` | `--sel` |
| `input.bright_black` | `color8` | `--c8` |
| `input.mode` | `mode` | — |

Si l'une manque ou est invalide, **la palette entière tombe en repli** sur un thème embarqué.
Jamais de mélange partiel entre deux sources.

### 2.2 Optionnelles — lues si présentes

`red`, `green`, `yellow`, `blue`. Absentes, elles sont dérivées (§4.2).

### 2.3 Paramètres de réglage, propres à chaque thème

C'est l'apport principal du design par rapport à la première version du `SPEC.md` : ces
coefficients **ne sont pas des constantes du code**, ce sont des champs du thème.

| Paramètre | Rôle | Sombre | Clair |
|---|---|---|---|
| `muted_mix` | proportion de `foreground` dans `text_muted` | 65 % | 90 % |
| `dim_mix` | proportion de `foreground` dans `text_dim` | 45 % | 66 % |
| `lane_lightness` | L des lanes de graphe | 0.74–0.76 | 0.52 |
| `lane_chroma` | C des lanes de graphe | 0.12–0.13 | 0.14 |

Raison : sur un fond clair il faut beaucoup plus de `foreground` pour qu'un texte secondaire
reste lisible. Un coefficient figé produit un thème clair délavé ou un thème sombre criard.

Valeurs de référence des thèmes livrés :

| Thème | `muted_mix` | `dim_mix` | `lane_lightness` | `lane_chroma` |
|---|---|---|---|---|
| Tokyo Night | 65 % | 45 % | 0.74 | 0.13 |
| Gruvbox | 65 % | 45 % | 0.75 | 0.12 |
| Matte Black | 65 % | 45 % | 0.76 | 0.13 |
| Clair (Rosé Pine Dawn) | 90 % | 66 % | 0.52 | 0.14 |

Un thème lu depuis Omarchy n'apporte pas ces paramètres. On applique alors les valeurs par
défaut de son `mode`, puis la correction de contraste de §4.3.

## 3. Notation

`mix(A, B, p)` signifie **p % de A, le reste de B**, en espace sRGB.
Équivalent CSS exact : `color-mix(in srgb, A p%, B)`.

Attention : c'est l'inverse de la formulation ambiguë de la première version du `SPEC.md`.
Ici `mix(foreground, background, 65%)` = 65 % de `foreground`.

## 4. Tokens dérivés

### 4.1 Structure — 10 tokens

| Nom canonique | CSS maquettes | Formule |
|---|---|---|
| `bg` | `--bg` | `input.background` |
| `surface` | `--surface` | `mix(foreground, background, 4%)` |
| `surface_raised` | `--raised` | `input.selection` |
| `surface_hover` | `--hover` | `mix(foreground, background, 6%)` |
| `border` | `--border` | `input.bright_black` |
| `border_focus` | `--focus` | `input.accent` |
| `text` | `--fg` | `input.foreground` |
| `text_muted` | `--muted` | `mix(foreground, background, muted_mix)` |
| `text_dim` | `--dim` | `mix(foreground, background, dim_mix)` |
| `accent` | `--ac` | `input.accent` |

`surface_hover` ne figurait pas dans la première version du `SPEC.md`. Il est nécessaire :
sans lui, le survol d'une ligne de liste devrait réutiliser `surface_raised`, qui signifie
« sélectionné ». Survol et sélection doivent rester distincts.

### 4.2 Statut — 4 tokens

| Nom canonique | Si le thème fournit | Sinon, OKLCH |
|---|---|---|
| `danger` | `red` | teinte 25° |
| `success` | `green` | teinte 145° |
| `warning` | `yellow` | teinte 85° |
| `info` | `blue` | teinte 250° |

Luminance et chroma de repli : sombre `L 0.68–0.80, C 0.13–0.17` ; clair `L ≈ 0.52, C ≈ 0.14`.
Valeurs exactes du thème Matte Black, qui exerce ce chemin de repli en entier :

```
danger  = oklch(0.68 0.17 25)
success = oklch(0.74 0.15 145)
warning = oklch(0.80 0.14 85)
info    = oklch(0.70 0.13 250)
```

### 4.3 Correction de contraste — obligatoire

Après dérivation **et** après lecture d'une couleur nommée du thème, ajuster la luminance
jusqu'à atteindre un contraste ≥ 4.5:1 contre `bg`.

Cette correction s'applique aux couleurs **lues** autant qu'aux couleurs dérivées : rien ne
garantit qu'un thème Omarchy fournisse un `green` lisible sur son propre fond.

## 5. Teintes de statut appliquées à un fond

Toute surface teintée par une couleur de statut passe par cette échelle. Aucun composant
n'invente son propre pourcentage.

| Niveau | Valeur | Usage |
|---|---|---|
| `tint_subtle` | 12 % | fonds de ligne de diff (ajout, suppression) |
| `tint_medium` | 16 % | badges, pastilles, chips de statut |
| `tint_strong` | 24 % | surlignage intra-ligne mot à mot |

Tokens diff nommés, dérivés de cette échelle :

| Nom canonique | CSS maquettes | Formule |
|---|---|---|
| `diff_added` | `--add` | `mix(success, bg, 12%)` |
| `diff_added_word` | `--addw` | `mix(success, bg, 24%)` |
| `diff_deleted` | `--del` | `mix(danger, bg, 12%)` |
| `diff_deleted_word` | `--delw` | `mix(danger, bg, 24%)` |

> **Écart relevé dans les maquettes, à corriger.** Le design utilise à la fois `danger 16%`
> (badges) et `danger 18%` (chips carrés). Deux points d'écart sont perceptuellement
> indistinguables et créent une valeur magique de plus à maintenir. **Les deux sont ramenés à
> `tint_medium` = 16 %.** Si une distinction visuelle est réellement voulue entre badge et
> chip, elle passe par la bordure ou la taille, pas par 2 % d'opacité.

## 6. Lanes du graphe de commits

**Ne lisent jamais le thème.** Huit teintes équiréparties, générées :

```
lane[i] = oklch(lane_lightness, lane_chroma, 20° + i × 45°)
```

soit les teintes 20, 65, 110, 155, 200, 245, 290, 335. Cycliques au-delà de huit lanes.

Seuls `lane_lightness` et `lane_chroma` varient selon le thème (§2.3), pour rester lisibles
sur le fond. La répartition des teintes ne varie jamais.

Signature attendue :

```rust
fn lane_colors(count: usize, lightness: f32, chroma: f32) -> Vec<Color>
```

Justification à conserver dans `ARCHITECTURE.md` : la couleur d'une lane est arbitraire et ne
porte aucun sens ; elle doit seulement se distinguer de sa voisine. Aucune palette de thème ne
peut garantir huit teintes séparables — Matte Black en a zéro.

État « lane sélectionnée » : la lane du commit sélectionné garde son opacité pleine, les
autres tombent à 40 %. Le nœud sélectionné reste net. La position, pas la teinte, porte la
lecture.

## 7. Densité

Deux modes. Les valeurs sont des tokens, pas des constantes de composant.

| Token | CSS | Compact | Confortable |
|---|---|---|---|
| `row_height` | `--row` | 26px | 32px |
| `row_padding` | `--rowpad` | 6px | 8px |
| `gap` | `--gap` | 4px | 8px |
| `pad` | `--pad` | 6px | 10px |
| `control_height` | `--ctl` | 22px | 26px |
| `header_height` | `--hdr` | 20px | 24px |

Contrainte d'accessibilité maintenue dans les deux modes : cible cliquable ≥ 24×24px. En
compact, `control_height` vaut 22px — la zone de clic est donc étendue au-delà du visuel.

## 8. Typographie

| Token | Valeur |
|---|---|
| `font_ui` | `system-ui, -apple-system, 'Inter', sans-serif` |
| `font_mono` | `'JetBrains Mono', ui-monospace, monospace` |

Les maquettes incluent `'Segoe UI Variable Text'` dans la pile UI. Windows étant hors
périmètre, c'est inoffensif mais inutile — à retirer côté code.

Échelle : 13px de base, 11px pour les labels secondaires, 15–16px pour les titres de panneau.
Graisses 400 / 500 / 600 uniquement.

**Usage de la monospace** : tout le Git littéral — hashes, noms de branches, chemins de
fichiers, contenu de diff, messages de commit en vue détail, URLs de remotes.

## 9. Note de périmètre sur les maquettes livrées

Les maquettes `02 Topbars` contiennent trois variantes : Linux, macOS et **Windows 11**
(réserve droite de 138px). Windows est sorti du périmètre : cette variante est conservée
comme documentation, **elle n'est pas implémentée**.

En revanche, la variante **Linux avec boutons caption dessinés** (réserve droite 115px, soit
3 × 38px plus un séparateur) est à implémenter. Elle ne figurait pas dans le brief initial et
c'est un ajout pertinent : sur un bureau non tuilant ou pour une fenêtre flottante, Hyprland
ne décore pas à la place de l'app. La réserve est un spacer flex, jamais un padding
conditionnel.

Rappel de la contrainte macOS : les 78 premiers pixels de la topbar sont **interdits**, les
feux tricolores y sont dessinés par le système. Aucun contrôle ne doit y entrer.

## 10. Tests qui font foi

Ces tests vivent dans `layers-theme` et échouent le build s'ils cassent.

1. **Contraste** : pour chaque thème embarqué, toute paire texte/fond effectivement utilisée
   dans l'UI atteint ≥ 4.5:1. Échec en dessous.
2. **Repli** : un `colors.toml` complet, minimal à 5 clés, corrompu, vide, illisible, ou avec
   des couleurs hors gamut donne dans les cinq cas une palette valide.
3. **Lanes** : distance perceptuelle minimale entre lanes voisines respectée sur un fond quasi
   monochrome (Matte Black) et sur un fond pastel.
4. **Changement de thème Omarchy** : un remplacement de symlink émet exactement une mise à
   jour ; une écriture sans changement réel n'en émet aucune.
5. **Catalogue** : au moins 6 thèmes sombres et 2 clairs, tous passant les tests ci-dessus.
6. **Mode clair** : `text_muted` et `text_dim` restent lisibles — c'est le cas que des
   coefficients figés cassent, et c'est pourquoi ils sont paramétrables (§2.3).
