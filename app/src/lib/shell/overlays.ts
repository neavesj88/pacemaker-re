// Always-mounted global overlays (modals, drop zones, dialogs). Every `views/<area>/Overlays.svelte`
// (and `player/Overlays.svelte`) is rendered once by the shell. An area adds its file; nothing
// here is edited. Order follows the path, so overlays must not depend on stacking by import order.
import type { Component } from "svelte";

const modules = import.meta.glob<{ default: Component }>(["../views/*/Overlays.svelte", "../player/Overlays.svelte"], {
  eager: true,
});

export const overlays: Component[] = Object.entries(modules)
  .sort(([a], [b]) => a.localeCompare(b))
  .map(([, m]) => m.default);
