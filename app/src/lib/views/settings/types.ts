// What an area contributes to the Settings view. Each area that has settings of its own adds a
// `views/<area>/settings.ts` whose default export is a list of these; the Settings view finds them
// by glob (`registry.ts`), so a new feature's settings need no edit in this folder.
//
// An entry never re-implements a control: it points at the feature's own component or state, and
// the control stays where it is used as well. Three ways to change a setting from here:
//
// - `inline`: the feature's own settings component, shown in place when the entry is opened.
// - `dialog`: the feature's own settings dialog (it takes `onclose`), opened from the entry.
// - `open`: the feature's always-mounted panel or dialog, opened by calling its own opener.
//
// There is no "go to the view" kind: a saved setting whose control sits inside a flow (a dialog
// that needs a selection or a run) has its option fields extracted into a component that both the
// dialog and this view show, so every setting can be changed here.
import type { Component } from "svelte";
import type { SettingsArea } from "./areas";

export type { SettingsArea };

type Base = {
  /** Unique across all areas; also the anchor (`#settings-<id>`). */
  id: string;
  area: SettingsArea;
  title: string;
  /** One sentence: what these settings decide. */
  help: string;
  /** Extra words the search box matches, beyond the title and help. */
  keywords?: string;
  /** Reads the saved values (the feature's own loader); run before the controls are shown. */
  load?: () => unknown;
};

export type SettingsEntry = Base &
  (
    | { how: "inline"; component: Component<any>; props?: Record<string, unknown> }
    | { how: "dialog"; component: Component<{ onclose: () => void }>; summary?: () => string }
    | { how: "open"; open: () => unknown; button: string; summary?: () => string }
  );
