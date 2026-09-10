// Board 07's command palette: what it searches, and how it ranks what it finds.
//
// Kept apart from the component on purpose. The ranking is the part with rules
// in it — which of two rows comes first, which characters are marked — and a
// test that had to mount an overlay and type into it to check them would be a
// test nobody reads when one changes.
//
// Board 07 calls this "le point d'entrée principal de l'app", which is a claim
// about *coverage* rather than about looks: what cannot be reached from here
// has to be found by knowing where it lives.

import type { BranchRow, LibraryRow, StatusRow } from "./ipc";
import type { Action } from "./keymap";

/// Where a row leads. The palette does four different things and the row has to
/// say which, both to the reader — board 07 prints "checkout ⏎" on a branch and
/// "ouvrir ⏎" on a file — and to whatever runs it.
export type RowKind = "action" | "repository" | "branch" | "file";

export type Row = {
  kind: RowKind;
  /// What running it needs: the action's id, the repository's path, the
  /// branch's name, the file's path.
  key: string;
  label: string;
  /// The dimmed half of the line — a path, a divergence, a status letter.
  detail: string;
  /// What ⏎ will do, in two words. Board 07 prints it at the right of the row.
  hint: string;
  /// Which characters of `label` matched, so they can be drawn in accent —
  /// board 07 §"Surlignage fuzzy".
  marks: number[];
  /// Dimmed rather than hidden when it cannot run now: an action that
  /// disappears is one nobody learns.
  enabled: boolean;
};

export type Group = { name: string; rows: Row[] };

/// How many rows of one group are worth showing before it stops being a list
/// and starts being a wall. Typing narrows it; this is the floor under an empty
/// query.
const PER_GROUP = 8;

export type Sources = {
  actions: Action[];
  repositories: LibraryRow[];
  branches: BranchRow[];
  files: StatusRow[];
};

/// The palette's answer to what has been typed.
///
/// Groups in a fixed order rather than mixed by score: a palette whose rows
/// change *category* as you type makes the next keystroke unpredictable, and
/// the whole point of ⏎ on the first row is that you can press it without
/// looking.
export function search(query: string, sources: Sources): Group[] {
  const groups: Group[] = [
    {
      name: "Actions",
      rows: rank(
        query,
        sources.actions.map((action) => ({
          kind: "action" as const,
          key: action.id,
          label: action.label,
          detail: "",
          hint: "exécuter ⏎",
          marks: [],
          enabled: action.enabled(),
        })),
      ),
    },
    {
      name: "Dépôts",
      rows: rank(
        query,
        sources.repositories.map((row) => ({
          kind: "repository" as const,
          key: row.path,
          label: row.name,
          detail: row.path,
          hint: "ouvrir ⏎",
          marks: [],
          enabled: !row.missing,
        })),
      ),
    },
    {
      name: "Branches",
      rows: rank(
        query,
        sources.branches.map((row) => ({
          kind: "branch" as const,
          key: row.name,
          label: row.name,
          detail: divergence(row),
          hint: "basculer ⏎",
          marks: [],
          enabled: !row.head,
        })),
      ),
    },
    {
      name: "Fichiers",
      rows: rank(
        query,
        sources.files.map((row) => ({
          kind: "file" as const,
          key: row.path,
          label: row.path,
          detail: row.code.trim(),
          hint: "ouvrir ⏎",
          marks: [],
          enabled: true,
        })),
      ),
    },
  ];
  return groups.filter((group) => group.rows.length > 0);
}

/// Every row of one group, best first, cut to a length a reader can scan.
function rank(query: string, rows: Row[]): Row[] {
  const trimmed = query.trim();
  if (trimmed === "") return rows.slice(0, PER_GROUP);
  return rows
    .map((row) => ({ row, hit: fuzzy(trimmed, row.label) }))
    .filter((found): found is { row: Row; hit: Hit } => found.hit !== null)
    .sort((a, b) => b.hit.score - a.hit.score)
    .slice(0, PER_GROUP)
    .map(({ row, hit }) => ({ ...row, marks: hit.marks }));
}

function divergence(row: BranchRow): string {
  if (row.head) return "branche courante";
  const tracking = row.tracking;
  if (!tracking) return "";
  if (tracking.gone) return "distant disparu";
  return [
    tracking.ahead > 0 ? `↑${tracking.ahead}` : "",
    tracking.behind > 0 ? `↓${tracking.behind}` : "",
  ]
    .filter(Boolean)
    .join(" ");
}

export type Hit = { score: number; marks: number[] };

/// Whether `text` contains the letters of `query` in order, and how well.
///
/// A subsequence match, which is what makes `thm` find "Theme" — the point of a
/// fuzzy palette is that you type what you remember, not what is written. The
/// score exists to put the *right* "Theme" first when eight rows match:
///
/// * a letter that starts a word counts double — `wc` should find "Working
///   Copy" ahead of anything with a stray `w` and `c` in the middle;
/// * a letter next to the previous one counts double again, so a run beats a
///   scatter;
/// * a gap costs a little, and a long label costs a little, so the shortest
///   row that says the same thing wins.
export function fuzzy(query: string, text: string): Hit | null {
  const wanted = query.toLowerCase();
  const against = text.toLowerCase();
  const marks: number[] = [];
  let score = 0;
  let at = 0;

  for (const letter of wanted) {
    if (letter === " ") continue;
    const found = against.indexOf(letter, at);
    if (found < 0) return null;
    const previous = marks[marks.length - 1];
    const contiguous = previous !== undefined && found === previous + 1;
    const starts = found === 0 || /[\s/\-_.:@]/.test(against[found - 1] ?? "");
    score += 1 + (contiguous ? 2 : 0) + (starts ? 2 : 0) - Math.min(found - at, 4) * 0.1;
    marks.push(found);
    at = found + 1;
  }
  // A shorter row saying the same thing is the better answer.
  return { score: score - against.length * 0.01, marks };
}

/// The rows of every group, in the order they are drawn — what ↑ and ↓ move
/// through, and what ⏎ runs.
export function flatten(groups: Group[]): Row[] {
  return groups.flatMap((group) => group.rows);
}
