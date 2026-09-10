// The words of the interface, one file per language.
//
// English is the reference: `languages/en.ts` declares the keys and every other
// catalogue is typed against it, so a missing key is a compile error rather
// than a blank on screen. A catalogue that misses one anyway — a third file
// added by somebody else, half finished — falls back to English rather than
// showing a raw key, which is what makes "drop a file in" safe.
//
// Adding a language is exactly that: one file in `languages/`. Nothing imports
// it by name; the directory is what is read.

import { computed, ref } from "vue";
import { strings as english } from "./languages/en";

export type Key = keyof typeof english;
/// The keys that count. A base whose `.one` exists in the reference catalogue,
/// derived rather than listed — so a plural added to `en.ts` becomes callable
/// and a plural removed becomes a compile error at its call site.
///
/// Through a generic, because that is the only shape a conditional type
/// distributes over: written against `Key` directly it asks whether the *whole*
/// union ends in `.one`, which nothing does, and every plural key came out
/// `never`.
type BaseOf<K> = K extends `${infer Base}.one` ? Base : never;
export type Plural = BaseOf<Key>;
/// What a catalogue file exports.
export type Catalogue = { name: string; strings: Record<string, string> };

/// Every catalogue in `languages/`, by tag — the file name without `.ts`.
///
/// `import.meta.glob` rather than a list of imports: a list is a second place
/// to remember, and the requirement is that adding a language is adding a file.
const found = import.meta.glob<Catalogue>("./languages/*.ts", { eager: true });

export const LANGUAGES = Object.entries(found)
  .map(([path, module]) => ({
    tag: path.slice(path.lastIndexOf("/") + 1).replace(/\.ts$/, ""),
    /// In its own language: a picker that says "French" to somebody looking for
    /// "Français" is a picker they cannot read.
    name: module.name,
    strings: module.strings,
  }))
  .sort((left, right) => left.tag.localeCompare(right.tag));

/// The language asked for, or `null` to follow the system.
const chosen = ref<string | null>(null);

/// What the system asks for, as one of the tags we have.
///
/// `navigator.language` is the web view's report of the OS setting — `fr-FR` on
/// a French macOS, `en-GB` on a British one — so `fr-FR` finds `fr`. English
/// when nothing matches, which is also the default when there is no preference
/// at all.
export function systemLanguage(): string {
  const asked = (globalThis.navigator?.language ?? "en").toLowerCase();
  const exact = LANGUAGES.find((language) => language.tag === asked);
  if (exact) return exact.tag;
  const base = asked.split("-")[0] ?? "en";
  return LANGUAGES.find((language) => language.tag === base)?.tag ?? "en";
}

/// The language in force: what was chosen, or what the system asks for.
export const language = computed(() => chosen.value ?? systemLanguage());

/// What was chosen, `null` while following the system. The Preferences screen
/// draws the difference.
export const preference = computed(() => chosen.value);

/// Set the language, or `null` to follow the system again.
///
/// Sets `<html lang>` with it: the browser's own hyphenation, spell-checking
/// and text selection read it, and a window saying `fr` in English is one that
/// will hyphenate English as if it were French.
export function useLanguage(tag: string | null): void {
  chosen.value = tag;
  globalThis.document?.documentElement.setAttribute("lang", language.value);
}

const catalogue = computed<Record<string, string>>(
  () => LANGUAGES.find((one) => one.tag === language.value)?.strings ?? english,
);

/// One string, with `{name}` placeholders filled in.
///
/// Reads `language` through Vue's reactivity, so every template that calls it
/// re-renders when the language changes. That is the whole reason it is a
/// function and not a lookup done once at start-up.
export function t(key: Key, params?: Record<string, string | number>): string {
  return fill(catalogue.value[key] ?? english[key] ?? key, params);
}

/// A string that counts: `{n} file` / `{n} files`, and whatever the rules are
/// elsewhere.
///
/// The catalogue holds one entry per plural category — `key.one`, `key.other`,
/// and `key.few` or `key.many` for the languages that have them — and
/// `Intl.PluralRules` says which applies. English and French need two; Russian
/// needs four, and a Russian file can add them without a line of code changing.
export function count(key: Plural, n: number, params?: Record<string, string | number>): string {
  const rule = new Intl.PluralRules(language.value).select(n);
  // Both read loosely: the rule is a runtime value — `zero`, `few`, `many` in
  // languages that have them — and no type can promise the catalogue holds an
  // entry for it. Missing, it falls through to `.other`, which every catalogue
  // has because `en.ts` does.
  const strings = catalogue.value;
  const reference: Record<string, string> = english;
  const text =
    strings[`${key}.${rule}`] ??
    strings[`${key}.other`] ??
    reference[`${key}.${rule}`] ??
    reference[`${key}.other`] ??
    key;
  return fill(text, { n, ...params });
}

function fill(text: string, params?: Record<string, string | number>): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}
