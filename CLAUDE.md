# Comment on travaille ici

Projet privé, un seul développeur. Tant qu'il n'est pas à un stade avancé, on
reste simple.

- **Pas de CI *de vérification*.** Pas de gate automatique, pas de contrôle
  déclenché par un push. Ça a existé et ça a été retiré (SPEC §3 règle 8 et son
  amendement) : ça vérifiait le travail de personne d'autre, pour un coût réel —
  dépôt privé, runners macOS facturés ×10.

  Ce qui existe depuis (2026-09-10), et qui n'est pas la même chose :
  `.github/workflows/release.yml`, déclenché **par un tag**, qui construit les
  paquets Linux et les attache à une release en brouillon. Il ne vérifie
  personne : il fabrique la chose que d'autres installent, depuis un checkout
  propre, sur une machine que personne n'a éditée. Le gate, lui, reste
  `scripts/check.sh` lancé à la main avant de couper le tag.
- **Pas de PR.** On commite et on pousse sur `main`. Pas de branche de
  fonctionnalité, pas d'enveloppe de revue : il n'y a pas de relecteur.
- **La seule porte, c'est `scripts/check.sh`**, lancé à la main quand on le
  décide : `fmt`, `clippy -D warnings`, `test`, build release. Il ne compile que
  la moitié de `omagit-app/src/platform/` de la machine hôte — le projet vit sur
  Linux et macOS, chaque machine trouvera ce que l'autre a cassé au prochain
  passage.
- **Un push qui casse quelque chose n'est pas un incident** à ce stade. On
  répare quand on le voit.

On remettra CI, PR et process le jour où un second développeur rejoint le
projet, ou quand le projet sera assez avancé pour que ça protège quelque chose.
D'ici là, proposer ce genre d'outillage est du bruit : avant de construire de
l'infrastructure, nommer qui elle protège.

## Changement de pile en cours (2026-09-09)

L'interface passe de GPUI à **Tauri 2** : backend Rust, frontend web. La raison
est dans `docs/SPEC.md` §4 (amendement) et `docs/ARCHITECTURE.md` §2.20 — GPUI
existe pour servir Zed, pas des tiers.

Ce qui ne change pas : `omagit-git`, `omagit-theme`, `omagit-settings`,
`omagit-git-cli`. Ce qui disparaît : `omagit-ui`, les écrans, `vendor/`.

M6 est **interrompu** : ses filtres et sa comparaison A ↔ B ne seront pas écrits
en GPUI. Le prochain travail est le spike de M6b, sur Linux et WebKitGTK.

Le reste — architecture, décisions, risques — est dans `docs/SPEC.md`,
`docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/DESIGN-TOKENS.md`.
