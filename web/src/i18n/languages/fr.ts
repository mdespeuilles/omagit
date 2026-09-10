// Français.
//
// Typé contre `en.ts` : une clé qui manque ou une clé en trop est une erreur de
// compilation. Les conventions sont décrites là-bas.

import type { strings as reference } from "./en";

export const name = "Français";

export const strings: Record<keyof typeof reference, string> = {
  // ── La topbar ───────────────────────────────────────────────────────────
  "topbar.back": "Retour aux dépôts",
  "topbar.repositories": "Dépôts",
  "topbar.addRepository": "Ajouter un dépôt local",
  "topbar.clone": "Cloner…",
  "topbar.cloneTitle": "Cloner un dépôt",
  "topbar.search": "Rechercher",
  "topbar.palette": "Palette de commandes",
  "topbar.gitUnusable": "git indisponible — {reason}",
  "topbar.noUpstream": "Cette branche ne suit aucune branche distante",
  "topbar.detached": "HEAD est détaché : il n'y a pas de branche à publier",
  "topbar.networkBusy": "une opération réseau est déjà en cours",

  // ── Les onglets ─────────────────────────────────────────────────────────
  "tabs.close": "Fermer {name}",

  // ── La sidebar ──────────────────────────────────────────────────────────
  "sidebar.workspace": "Workspace",
  "sidebar.workingCopy": "Copie de travail",
  "sidebar.history": "Historique",
  "sidebar.stashes": "Remises",
  "sidebar.settings": "Réglages",
  "sidebar.allRepositories": "Tous les dépôts",

  // ── La barre d'état ─────────────────────────────────────────────────────
  "statusbar.staged.one": "{n} indexé",
  "statusbar.staged.other": "{n} indexés",
  "statusbar.unstaged.one": "{n} non indexé",
  "statusbar.unstaged.other": "{n} non indexés",
  "statusbar.running": "{operation} en cours",
  "statusbar.continue": "Poursuivre",
  "statusbar.finish": "Terminer {operation}",
  "statusbar.conflictsLeft.one": "Il reste {n} conflit à résoudre",
  "statusbar.conflictsLeft.other": "Il reste {n} conflits à résoudre",
  "statusbar.abort": "Abandonner",
  "statusbar.conflicts.one": "{n} conflit",
  "statusbar.conflicts.other": "{n} conflits",
  "statusbar.journal": "Journal",

  // ── Dates ───────────────────────────────────────────────────────────────
  "date.today": "aujourd'hui {time}",
  "date.yesterday": "hier {time}",

  // ── Réglages ────────────────────────────────────────────────────────────
  "settings.language": "Langue",
  "settings.languageNote":
    "Celle du système, sauf si tu en décides autrement. Ajouter une langue, c'est ajouter un fichier dans `web/src/i18n/languages/`.",
  "settings.languageSystem": "Suivre le système",

  // ── L'arbre des branches ────────────────────────────────────────────────
  "branches.title": "Branches",
  "branches.tags": "Tags",
  "branches.remotes": "Remotes",
  "branches.empty": "Aucune branche : elle naîtra du premier commit.",
  "branches.reading": "Lecture des branches…",
  "branches.new": "Nouvelle branche",
  "branches.name": "Nom de la branche",
  "branches.create": "Créer",
  "branches.merge": "Fusionner {branch} dans la branche courante",
  "branches.mergeShort": "Fusionner",
  "branches.rebase": "Rejouer la branche courante sur {branch}",
  "branches.rebaseShort": "Rebaser",
  "branches.delete": "Supprimer cette branche",
  "branches.deleteShort": "Suppr.",
  "branches.merged": "Merged",
  "branches.row": "{branch} — clic : son historique, double-clic : basculer dessus",
  "branches.remoteRow": "{branch} — clic : son historique",
  "branches.months.one": "{n} mois",
  "branches.months.other": "{n} mois",
  "branches.years.one": "{n} an",
  "branches.years.other": "{n} ans",
};
