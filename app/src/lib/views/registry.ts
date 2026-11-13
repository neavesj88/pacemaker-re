// The list of top-level views, discovered from `views/<area>/view.ts` (default export: a
// ViewDef). The sidebar and the view switcher are both driven from here. A new view is a new
// folder with a view.ts; this file is not edited.
import type { Component } from "svelte";
import type { LibraryView } from "$lib/api/types";
import type { ViewId } from "./ids";

export type ViewDef = {
  id: ViewId;
  label: string;
  /** Sidebar section. */
  section: "library" | "tools";
  /** Position in the sidebar within its section (lower first). */
  order: number;
  component: Component;
  /** Sidebar entry shown only while this is true (a view that needs something to exist first). */
  visible?: () => boolean;
  /** Number shown beside the label, if the view has one. */
  count?: (lib: LibraryView) => number;
};

const modules = import.meta.glob<{ default: ViewDef }>("./*/view.ts", { eager: true });

export const views: ViewDef[] = Object.values(modules)
  .map((m) => m.default)
  .sort((a, b) => a.order - b.order);

export const viewById = (id: ViewId): ViewDef => views.find((v) => v.id === id) ?? views[0];
