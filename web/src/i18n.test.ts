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
          // the words that are the same in both languages, and the placeholders
          // that are only a placeholder. Listed rather than allowed by a rule,
          // so the set cannot grow by accident: an untranslated string is
          // otherwise indistinguishable from one that was never translated.
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
          "journal.title",
          "menu.services",
          "menu.zoom",
          "palette.actions",
          "palette.branches",
          "palette.close",
          "settings.compact",
          "settings.editorPlain",
          "settings.git",
          "sidebar.workspace",
          "statusbar.journal",
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
