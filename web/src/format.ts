// Dates, in the one place that knows what they should read like.
//
// The backend sends seconds since the epoch and nothing else: `omagit-git` has
// no locale by rule. A hand-written month table would be the same thing said
// twice in as many languages as the app has, and they would drift — `omagit-app`
// had one, in French, and nothing had called it since the port. `Intl` is the
// browser's own table.
//
// The formatters are rebuilt when the language changes rather than made once:
// they are cheap, and one made at import time would have frozen the language
// the window happened to start in.

import { language, t } from "./i18n";

const clock = () => new Intl.DateTimeFormat(language.value, { hour: "2-digit", minute: "2-digit" });
const sameYear = () => new Intl.DateTimeFormat(language.value, { day: "numeric", month: "short" });
const otherYear = () =>
  new Intl.DateTimeFormat(language.value, { day: "numeric", month: "short", year: "numeric" });
const full = () =>
  new Intl.DateTimeFormat(language.value, { dateStyle: "long", timeStyle: "short" });

/// `today 09:14`, `yesterday 18:02`, `14 Mar`, `14 Mar 2024`.
///
/// Days rather than elapsed hours: something that happened at 23:50 was
/// yesterday at 00:10, and "il y a 20 min" would be true and useless.
export function when(seconds: number, now = new Date()): string {
  const then = new Date(seconds * 1000);
  const days = midnights(then, now);
  if (days === 0) return t("date.today", { time: clock().format(then) });
  if (days === 1) return t("date.yesterday", { time: clock().format(then) });
  return then.getFullYear() === now.getFullYear()
    ? sameYear().format(then)
    : otherYear().format(then);
}

/// `il y a 4 min`, `il y a 3 jours` — elapsed time, for things whose age is
/// the point: when the remote was last asked, how long a branch has slept.
/// `when` is for dates a reader places on a calendar; this is for "how stale".
export function ago(seconds: number, now = Date.now()): string {
  const elapsed = Math.max(0, Math.round(now / 1000 - seconds));
  const format = new Intl.RelativeTimeFormat(language.value, { numeric: "auto" });
  if (elapsed < 60) return t("date.justNow");
  if (elapsed < 3600) return format.format(-Math.round(elapsed / 60), "minute");
  if (elapsed < 86_400) return format.format(-Math.round(elapsed / 3600), "hour");
  if (elapsed < 30 * 86_400) return format.format(-Math.round(elapsed / 86_400), "day");
  if (elapsed < 365 * 86_400) return format.format(-Math.round(elapsed / (30 * 86_400)), "month");
  return format.format(-Math.round(elapsed / (365 * 86_400)), "year");
}

/// The whole thing, for a tooltip — where the short form is never enough.
export function exact(seconds: number): string {
  return full().format(new Date(seconds * 1000));
}

/// The author's own clock, from the offset they committed with.
///
/// Shown on a commit detail because it is what the author saw: a commit made at
/// 09:00 in Paris is not a commit made at 09:00 in Tokyo, and a reader chasing
/// "what was I doing that morning" wants the morning that happened.
export function authored(seconds: number, offsetSeconds: number): string {
  const shifted = new Date((seconds + offsetSeconds) * 1000);
  const sign = offsetSeconds < 0 ? "−" : "+";
  const minutes = Math.abs(Math.round(offsetSeconds / 60));
  const zone = `${sign}${pad(Math.floor(minutes / 60))}:${pad(minutes % 60)}`;
  return `${new Intl.DateTimeFormat(language.value, {
    dateStyle: "long",
    timeStyle: "short",
    timeZone: "UTC",
  }).format(shifted)} (UTC${zone})`;
}

/// How many calendar days apart, which is not the same as elapsed / 86 400.
function midnights(then: Date, now: Date): number {
  const a = Date.UTC(then.getFullYear(), then.getMonth(), then.getDate());
  const b = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
  return Math.round((b - a) / 86_400_000);
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

/// `/Users/maxence/src/layers` → `~/src/layers`.
///
/// The topbar shows where a repository is, and an absolute path under the home
/// directory spends thirty characters saying something the reader knows. The
/// Rust side does the same for its own log lines; this is the browser's copy,
/// which cannot ask the operating system what `$HOME` is and infers it from the
/// shape instead.
export function tildify(path: string): string {
  const home = /^(\/(?:Users|home)\/[^/]+)(\/|$)/.exec(path);
  return home ? `~${path.slice(home[1]!.length)}` : path;
}

/// The colour a branch wears everywhere it is drawn (DESIGN v2: a branch keeps
/// its colour across the overview, the map and the history). Derived from the
/// name alone so it never changes between reads. The trunk takes the accent, so
/// `main` looks the same in every repository and never wears the lane that
/// reads as red.
///
/// FNV-1a rather than the usual `* 31`: with only seven lanes to spread over,
/// the weaker hash put three branches of four on the same colour.
export function branchColor(branch: string): string {
  const name = branch.replace(/^[^/]+\/(?=(main|master|trunk)$)/, "");
  if (name === "main" || name === "master" || name === "trunk") return "var(--accent)";
  let hash = 0x811c9dc5;
  for (const char of name) hash = Math.imul(hash ^ char.codePointAt(0)!, 0x01000193) >>> 0;
  return `var(--lane-${1 + (hash % 7)})`;
}

/// The letter a status badge shows, and the class that colours it.
///
/// Board 01 §06: a letter *and* a colour, never a colour alone — the badge has
/// to survive greyscale, and "which of these is the deletion" is a question a
/// hue cannot answer for everyone. Shared by the file list and the overview.
export function statusBadge(row: {
  conflict: string | null;
  staged: string | null;
  unstaged: string | null;
}): { letter: string; kind: string } {
  if (row.conflict) return { letter: "U", kind: "conflict" };
  const change = row.unstaged ?? row.staged ?? "";
  const letters: Record<string, string> = {
    modified: "M",
    added: "A",
    deleted: "D",
    renamed: "R",
    copied: "C",
    "type-changed": "T",
    untracked: "?",
    ignored: "!",
  };
  return { letter: letters[change] ?? "•", kind: change };
}
