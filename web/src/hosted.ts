// The user's repositories on GitHub, for the clone dialog to search (DESIGN
// v2, board "Clone").
//
// Listing two hundred repositories takes `gh` several seconds, which is too
// long to stare at an empty list every time the dialog opens. So the last list
// is kept in the web view's storage and shown at once, and a fresh one is asked
// for behind it when the kept one is more than a few minutes old.

import { reactive } from "vue";
import { api, type Hosted, type HostedRepo } from "./ipc";

const KEY = "omagit.hosted.github";
/// How old the kept list may be before opening the dialog asks for a new one.
const FRESH_FOR = 10 * 60 * 1000;

export const hosted = reactive<{
  list: Hosted | null;
  /// When `list` was read, in milliseconds.
  at: number;
  loading: boolean;
  /// `"gh-missing"`, or what `gh` said.
  error: string | null;
}>({ list: null, at: 0, loading: false, error: null });

function remembered(): void {
  if (hosted.list) return;
  try {
    const kept = JSON.parse(localStorage.getItem(KEY) ?? "null") as {
      at: number;
      list: Hosted;
    } | null;
    if (kept?.list?.repos) {
      hosted.list = kept.list;
      hosted.at = kept.at;
    }
  } catch {
    // A kept list that does not parse is a list not kept.
  }
}

/// Show the kept list, and ask `gh` for a new one if it is stale or `force`.
export async function loadHosted(force = false, now = Date.now()): Promise<void> {
  remembered();
  if (hosted.loading) return;
  if (!force && hosted.list && now - hosted.at < FRESH_FOR) return;
  hosted.loading = true;
  try {
    const list = await api.githubRepositories();
    hosted.list = list;
    hosted.at = now;
    hosted.error = null;
    try {
      localStorage.setItem(KEY, JSON.stringify({ at: now, list }));
    } catch {
      // Storage full or refused: the list still shows, it just is not kept.
    }
  } catch (error) {
    hosted.error = error instanceof Error ? error.message : String(error);
  } finally {
    hosted.loading = false;
  }
}

/// A remote URL reduced to `host/owner/name`, so an SSH URL and an HTTPS one
/// for the same repository compare equal.
export function remoteKey(url: string): string {
  return url
    .trim()
    .toLowerCase()
    .replace(/^[a-z+]+:\/\//, "")
    .replace(/^[^@/]+@/, "")
    .replace(/^([^/:]+):(?!\d)/, "$1/")
    .replace(/\.git$/, "")
    .replace(/\/+$/, "");
}

/// The repositories matching what was typed, best first.
///
/// Every word has to appear, in the name or the description. A word at the
/// start of the repository's own name ranks first, then anywhere in the name,
/// then only in the description; within a rank, the order `gh` gave — most
/// recently pushed — is kept, which is usually the one being looked for.
export function search(repos: readonly HostedRepo[], query: string): HostedRepo[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...repos];
  const scored: { repo: HostedRepo; score: number; at: number }[] = [];
  repos.forEach((repo, at) => {
    const full = repo.full_name.toLowerCase();
    const name = full.slice(full.indexOf("/") + 1);
    const description = (repo.description ?? "").toLowerCase();
    let score = 0;
    for (const word of words) {
      if (name.startsWith(word)) score += 3;
      else if (full.includes(word)) score += 2;
      else if (description.includes(word)) score += 1;
      else return;
    }
    scored.push({ repo, score, at });
  });
  scored.sort((a, b) => b.score - a.score || a.at - b.at);
  return scored.map((entry) => entry.repo);
}
