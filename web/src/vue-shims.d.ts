/// `.vue` is not a TypeScript module extension, so the compiler is told what
/// one resolves to. `vue-tsc` type-checks the templates themselves.
declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>;
  export default component;
}
