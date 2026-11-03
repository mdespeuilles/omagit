// The user's repositories on GitHub and on GitLab hosts, for the clone
// dialog to search (DESIGN v2, board "Clone").
//
// Listing two hundred repositories takes `gh` several seconds, which is too
// long to stare at an empty list every time the dialog opens. So the last list
// of each source is kept in the web view's storage and shown at once, and a
// fresh one is asked for behind it when the kept one is a few minutes old.
//
// The sources are what the machine has: GitHub when there is a `gh`, and each
// host `glab` is configured for.

import { reactive } from "vue";
import { api, type Forges, type Hosted, type HostedRepo } from "./ipc";

/// How old the kept list may be before opening the dialog asks for a new one.
const FRESH_FOR = 10 * 60 * 1000;

/// A source, as a key: `"github"`, or `"gitlab:<host>"`.
export type Source = string;

export const hosted = reactive<{
  /// What the machine can search, once asked.
  forges: Forges | null;
  /// The source being searched.
  source: Source;
  list: Hosted | null;
  /// When `list` was read, in milliseconds.
  at: number;
  loading: boolean;
  /// `"gh-missing"`, or what the forge's client said.
  error: string | null;
}>({ forges: null, source: "github", list: null, at: 0, loading: false, error: null });

/// Every source this machine has, GitHub first.
export function sources(forges: Forges | null): Source[] {
  if (!forges) return ["github"];
  return [...(forges.github ? ["github"] : []), ...forges.gitlab.map((host) => `gitlab:${host}`)];
}

const keyOf = (source: Source) => `omagit.hosted.${source}`;

function remembered(): void {
  if (hosted.list) return;
  try {
    const kept = JSON.parse(localStorage.getItem(keyOf(hosted.source)) ?? "null") as {
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

/// Ask once which forges there are, and start on the first of them.
async function readForges(): Promise<void> {
  if (hosted.forges) return;
  try {
    hosted.forges = await api.forges();
  } catch {
    hosted.forges = { github: true, gitlab: [] };
  }
  const all = sources(hosted.forges);
  if (!all.includes(hosted.source) && all[0]) hosted.source = all[0];
}

/// Search another source: its kept list at once, a fresh one if stale.
export async function chooseSource(source: Source): Promise<void> {
  if (source === hosted.source) return;
  hosted.source = source;
  hosted.list = null;
  hosted.at = 0;
  hosted.error = null;
  await loadHosted();
}

/// Show the kept list, and ask the forge for a new one if it is stale or
/// `force`.
export async function loadHosted(force = false, now = Date.now()): Promise<void> {
  await readForges();
  if (sources(hosted.forges).length === 0) {
    hosted.error = "gh-missing";
    return;
  }
  remembered();
  if (hosted.loading) return;
  if (!force && hosted.list && now - hosted.at < FRESH_FOR) return;
  const source = hosted.source;
  hosted.loading = true;
  try {
    const list = source.startsWith("gitlab:")
      ? await api.gitlabRepositories(source.slice("gitlab:".length))
      : await api.githubRepositories();
    if (hosted.source !== source) return;
    hosted.list = list;
    hosted.at = now;
    hosted.error = null;
    try {
      localStorage.setItem(keyOf(source), JSON.stringify({ at: now, list }));
    } catch {
      // Storage full or refused: the list still shows, it just is not kept.
    }
  } catch (error) {
    if (hosted.source === source) {
      hosted.error = error instanceof Error ? error.message : String(error);
    }
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
