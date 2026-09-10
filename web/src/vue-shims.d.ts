/// `.vue` is not a TypeScript module extension, so the compiler is told what
/// one resolves to. `vue-tsc` type-checks the templates themselves.
declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>;
  export default component;
}

/// `import.meta.glob`, which is Vite's and not TypeScript's.
///
/// Declared here rather than by pulling in `vite/client`: that reference brings
/// the whole asset-module surface — `*.svg`, `*.css?inline`, `?raw` — none of
/// which this front end uses, and every one of which would then type-check as
/// if it did.
interface ImportMeta {
  glob<T>(pattern: string, options: { eager: true }): Record<string, T>;
}
