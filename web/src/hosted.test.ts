// Searching the user's GitHub from the clone dialog (DESIGN v2).

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Hosted, HostedRepo } from "./ipc";

const backend = vi.hoisted(() => ({
  answer: null as unknown,
  calls: 0,
  forges: { github: true, gitlab: [] as string[] },
  asked: [] as string[],
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: { host?: string }) => {
    if (command === "forges") return Promise.resolve(backend.forges);
    if (command === "gitlab_repositories") {
      backend.asked.push(args?.host ?? "");
      return Promise.resolve({ ...LIST, host: args?.host, repos: [repo("team/api")] });
    }
    if (command !== "github_repositories") return Promise.resolve(null);
    backend.calls += 1;
    return backend.answer instanceof Error
      ? Promise.reject(backend.answer)
      : Promise.resolve(backend.answer);
  },
}));

function repo(full_name: string, description: string | null = null): HostedRepo {
  return {
    full_name,
    description,
    private: false,
    fork: false,
    archived: false,
    pushed_at: null,
    ssh_url: `git@github.com:${full_name}.git`,
    clone_url: `https://github.com/${full_name}.git`,
  };
}

const LIST: Hosted = {
  host: "github.com",
  ssh: true,
  repos: [repo("me/blog", "Old omagit notes"), repo("org/omagit-site"), repo("me/omagit")],
};

/// The web view's storage, which the test environment does not have.
const kept = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (key: string) => kept.get(key) ?? null,
  setItem: (key: string, value: string) => void kept.set(key, value),
  clear: () => kept.clear(),
});

beforeEach(() => {
  localStorage.clear();
  backend.calls = 0;
  backend.answer = LIST;
  backend.forges = { github: true, gitlab: [] };
  backend.asked = [];
  vi.resetModules();
});

describe("the search", () => {
  it("puts a name that starts with the word first, then the name, then the description", async () => {
    const { search } = await import("./hosted");
    expect(search(LIST.repos, "omagit").map((r) => r.full_name)).toEqual([
      "org/omagit-site",
      "me/omagit",
      "me/blog",
    ]);
  });

  it("needs every word, and keeps the forge's order when nothing is typed", async () => {
    const { search } = await import("./hosted");
    expect(search(LIST.repos, "me omagit").map((r) => r.full_name)).toEqual([
      "me/omagit",
      "me/blog",
    ]);
    expect(search(LIST.repos, "  ").map((r) => r.full_name)).toEqual(
      LIST.repos.map((r) => r.full_name),
    );
    expect(search(LIST.repos, "nothing-like-it")).toEqual([]);
  });
});

describe("a remote, compared", () => {
  it("is the same over SSH and HTTPS, with or without .git", async () => {
    const { remoteKey } = await import("./hosted");
    const key = "github.com/me/omagit";
    expect(remoteKey("git@github.com:Me/omagit.git")).toBe(key);
    expect(remoteKey("https://github.com/me/omagit")).toBe(key);
    expect(remoteKey("ssh://git@github.com/me/omagit.git")).toBe(key);
    expect(remoteKey("ssh://git@github.com:2222/me/omagit.git")).not.toBe(key);
  });
});

describe("the list", () => {
  it("is kept, shown at once next time, and only asked for again once stale", async () => {
    let hosted = await import("./hosted");
    await hosted.loadHosted(false, 1_000);
    expect(backend.calls).toBe(1);

    vi.resetModules();
    hosted = await import("./hosted");
    await hosted.loadHosted(false, 2_000);
    expect(backend.calls).toBe(1);
    expect(hosted.hosted.list?.repos).toHaveLength(3);

    await hosted.loadHosted(false, 1_000 + 11 * 60 * 1000);
    expect(backend.calls).toBe(2);
  });

  it("keeps the last list when gh fails, and says why", async () => {
    const hosted = await import("./hosted");
    await hosted.loadHosted(false, 1_000);
    backend.answer = new Error("HTTP 401: Bad credentials");
    await hosted.loadHosted(true, 2_000);
    expect(hosted.hosted.list?.repos).toHaveLength(3);
    expect(hosted.hosted.error).toContain("Bad credentials");
  });
});

describe("the sources", () => {
  it("are GitHub when there is a gh, then each GitLab host glab knows", async () => {
    const { sources } = await import("./hosted");
    expect(sources({ github: true, gitlab: ["git.example.com"] })).toEqual([
      "github",
      "gitlab:git.example.com",
    ]);
    expect(sources({ github: false, gitlab: [] })).toEqual([]);
  });

  it("starts on GitLab when there is no gh, and asks that host", async () => {
    backend.forges = { github: false, gitlab: ["git.example.com"] };
    const hosted = await import("./hosted");
    await hosted.loadHosted(false, 1_000);
    expect(hosted.hosted.source).toBe("gitlab:git.example.com");
    expect(backend.asked).toEqual(["git.example.com"]);
    expect(backend.calls).toBe(0);
    expect(hosted.hosted.list?.repos.map((r) => r.full_name)).toEqual(["team/api"]);
  });

  it("keeps each source's list apart", async () => {
    backend.forges = { github: true, gitlab: ["git.example.com"] };
    const hosted = await import("./hosted");
    await hosted.loadHosted(false, 1_000);
    expect(hosted.hosted.list?.repos).toHaveLength(3);
    await hosted.chooseSource("gitlab:git.example.com");
    expect(hosted.hosted.list?.repos).toHaveLength(1);
  });

  it("says there is nothing to search with when there is neither client", async () => {
    backend.forges = { github: false, gitlab: [] };
    const hosted = await import("./hosted");
    await hosted.loadHosted();
    expect(hosted.hosted.error).toBe("gh-missing");
  });
});
