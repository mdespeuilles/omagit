// The words, and the rules that keep a second language from being a liability.
//
// Two of these are about *files nobody has written yet*: adding a language has
// to be adding a file, and a file that is half finished must degrade rather
// than break. The rest is the arithmetic — plurals, placeholders — which is
// where a hand-rolled i18n usually goes wrong.

import { describe, expect, it } from "vitest";
import { LANGUAGES, count, language, systemLanguage, t, useLanguage } from "./i18n";
import { strings as english } from "./i18n/languages/en";

describe("the catalogues", () => {
  it("are found by being there, not by being listed", async () => {
    // The requirement, literally: adding a language is adding a file to
    // `languages/`. Nothing imports them by name.
    expect(LANGUAGES.map((one) => one.tag)).toEqual(["en", "fr"]);
    for (const one of LANGUAGES) {
      expect(one.name.length, one.tag).toBeGreaterThan(0);
      expect(Object.keys(one.strings).length).toBeGreaterThan(10);
    }
  });

  it("say the same things", async () => {
    // A key missing from a translation is already a compile error — the
    // catalogue is typed against English — and this is the other half: a key
    // that exists but was left in English, which compiles and reads as a bug.
    const reference = Object.keys(english);
    for (const one of LANGUAGES.filter((l) => l.tag !== "en")) {
      expect(Object.keys(one.strings).sort(), one.tag).toEqual(reference.sort());
      const untranslated = reference.filter(
        (key) => one.strings[key] === (english as Record<string, string>)[key],
      );
      // Some are meant to be identical — Git's own vocabulary, and a word that
      // is the same in both languages. Named, so the list cannot grow quietly.
      expect(untranslated.sort(), one.tag).toEqual(
        [
          // Git's own vocabulary, which a French terminal uses untranslated,
          // the words that are the same in both languages, and the two that are
          // only a placeholder. Listed rather than allowed by a rule, so the set
          // cannot grow by accident: an untranslated string is otherwise
          // indistinguishable from one that was never translated.
          "action.network.fetch",
          "action.network.pull",
          "action.network.push",
          "ask.commits.one",
          "ask.commits.other",
          "branches.merged",
          "branches.remotes",
          "branches.tags",
          "branches.title",
          "card.description",
          "card.remotes",
          "clone.destination",
          "clone.url",
          "commit.amend",
          "commit.noVerify",
          "commit.signOff",
          "commitDetail.parent",
          "commitDetail.parents",
          "commitDetail.title",
          "conflict.both",
          "conflict.ours",
          "conflict.theirs",
          "history.commits.one",
          "history.commits.other",
          "history.message",
          "menu.services",
          "menu.zoom",
          "palette.actions",
          "palette.branches",
          "palette.close",
          "settings.compact",
          "settings.editorPlain",
          "settings.git",
          "sidebar.workspace",
        ].sort(),
      );
    }
  });

  it("fall back to English rather than showing a key", () => {
    // The case the types cannot cover: a language file somebody else wrote,
    // missing a key the app has since added.
    const half = { tag: "zz", name: "Halfway", strings: { "topbar.search": "Zoek" } };
    LANGUAGES.push(half);
    try {
      useLanguage("zz");
      expect(t("topbar.search")).toBe("Zoek");
      expect(t("topbar.repositories")).toBe(english["topbar.repositories"]);
    } finally {
      LANGUAGES.pop();
      useLanguage(null);
    }
  });
});

describe("the source", () => {
  // The files as *text*, through Vite rather than through `node:fs`: the same
  // mechanism that finds the catalogues, and one that needs no node types in a
  // front end that has none. What is checked is what is written — a string that
  // never reaches a screen is exactly the kind that stays behind when a
  // language moves.
  const raw = import.meta.glob<string>("./**/*.{ts,vue}", {
    eager: true,
    query: "?raw",
    import: "default",
  });

  const sources = (): [string, string][] =>
    Object.entries(raw).filter(
      ([path]) => !path.includes("/i18n/languages/") && !path.includes(".test."),
    );

  /// Comments are the project's own prose and may say anything; what is checked
  /// is the code and the markup.
  const withoutComments = (text: string): string =>
    text
      .replace(/<!--[\s\S]*?-->/g, "")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .split("\n")
      .filter((line) => !line.trimStart().startsWith("//"))
      .join("\n");

  const FRENCH = /[éèêëàâçùûôîï]/i;

  /// French without an accent in it, which the letters above cannot see —
  /// "introuvable sur le disque" has none, and hid in `state.ts` through the
  /// whole conversion. Two of these words in one string, with a space in it, is
  /// a sentence: none of them is an English word, so a match is not a
  /// coincidence.
  const FRENCH_WORDS = new RegExp(
    String.raw`\b(le|la|les|des|du|une|dans|sur|pas|aucun|aucune|est|sont|avec` +
      String.raw`|pour|par|cette|ces|qui|que|sans|plus|tout|toute|comme|mais|au|aux)\b`,
    "gi",
  );

  it("holds no French outside the catalogues", () => {
    // The nine strings this found the first time it ran had all been missed by
    // hand — two notes on the Preferences screen, a "Défaut" that appears
    // twice, a branch whose upstream is gone. Reading every file is the only
    // way that does not depend on somebody looking.
    const left: string[] = [];
    for (const [path, text] of sources()) {
      withoutComments(text)
        .split("\n")
        .forEach((line, at) => {
          if (FRENCH.test(line)) left.push(`${path}:${at + 1}: ${line.trim()}`);
        });
    }
    // `backend.fake.ts` is a test double: what it throws stands in for `git`'s
    // own words, which are never translated.
    expect(left.filter((one) => !one.startsWith("./backend.fake.ts"))).toEqual([]);
  });

  it("holds no accent-free French either", () => {
    const left: string[] = [];
    for (const [path, text] of sources()) {
      if (path.includes("backend.fake")) continue;
      withoutComments(text)
        .split("\n")
        .forEach((line, at) => {
          for (const match of line.matchAll(/"([^"\n]{6,160})"/g)) {
            const quoted = match[1] ?? "";
            if (!quoted.includes(" ")) continue;
            if ((quoted.match(FRENCH_WORDS) ?? []).length >= 2) {
              left.push(`${path}:${at + 1}: ${quoted}`);
            }
          }
        });
    }
    expect(left).toEqual([]);
  });

  it("keeps the reference catalogue in English", () => {
    // A French string left in `en.ts` compiles, translates, and reads as a bug
    // — the other catalogues are typed against it, not proof-read against it.
    const wrong = Object.entries(english).filter(([, value]) => FRENCH.test(value));
    expect(wrong).toEqual([]);
  });
});

describe("a string that counts", () => {
  it("agrees in each language's own way", () => {
    useLanguage("en");
    expect(count("statusbar.conflicts", 1)).toBe("1 conflict");
    expect(count("statusbar.conflicts", 3)).toBe("3 conflicts");

    useLanguage("fr");
    expect(count("statusbar.conflicts", 1)).toBe("1 conflit");
    expect(count("statusbar.conflicts", 3)).toBe("3 conflits");
    // French counts zero as singular and English does not. `Intl.PluralRules`
    // knows that; a `n > 1 ? "s" : ""` written by hand — which is what this
    // replaced — knew it for exactly one language.
    expect(count("statusbar.conflicts", 0)).toBe("0 conflit");
    useLanguage("en");
    expect(count("statusbar.conflicts", 0)).toBe("0 conflicts");
  });

  it("fills the places the sentence leaves", () => {
    useLanguage("en");
    expect(t("tabs.close", { name: "omagit" })).toBe("Close omagit");
    // A placeholder nobody filled stays visible rather than turning into
    // "undefined": it is a missing argument, and it should look like one.
    expect(t("tabs.close")).toContain("{name}");
  });
});

describe("the language in force", () => {
  it("follows the system until somebody says otherwise", () => {
    useLanguage(null);
    expect(language.value).toBe(systemLanguage());

    useLanguage("fr");
    expect(language.value).toBe("fr");
    expect(document.documentElement.getAttribute("lang")).toBe("fr");

    useLanguage(null);
    expect(language.value).toBe(systemLanguage());
  });

  it("falls back to English for a system language we do not have", () => {
    // jsdom reports `en-US`; what is pinned here is the resolution, which has
    // to answer for `pt-BR` as much as for `fr-CA`.
    expect(systemLanguage()).toBe("en");
  });
});
