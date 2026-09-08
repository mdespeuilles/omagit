# Comment on travaille ici

Projet privé, un seul développeur. Tant qu'il n'est pas à un stade avancé, on
reste simple.

- **Pas de CI.** Pas de GitHub Actions, pas de gate automatique, pas de
  vérification déclenchée par un push. Ça a existé et ça a été retiré
  (SPEC §3 règle 8 et son amendement) : ça vérifiait le travail de personne
  d'autre, pour un coût réel — dépôt privé, runners macOS facturés ×10.
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

Le reste — architecture, décisions, risques — est dans `docs/SPEC.md`,
`docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/DESIGN-TOKENS.md`.
