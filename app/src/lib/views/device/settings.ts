import { undo } from "$lib/state/undo.svelte";
import type { SettingsEntry } from "../settings/types";
import { relink } from "./relink.svelte";
import RelinkSettings from "./RelinkSettings.svelte";
import SafetyCard from "./SafetyCard.svelte";

const entries: SettingsEntry[] = [
  {
    id: "device-safety",
    area: "Device",
    title: "Safety: undo, restore and fix",
    help: "How many edits are kept to undo, how big a share of dead rows the fix tool refuses to remove, and how long the undo pill stays.",
    keywords: "undo restore fix dead rows guard pill snapshot trust",
    load: () => undo.loadSettings(),
    how: "inline",
    component: SafetyCard,
  },
  {
    id: "device-relink",
    area: "Device",
    title: "Finding missing audio",
    help: "Whether files are also matched by their tags, how far their length may differ, how deep the search goes and which matches are ticked at the start.",
    keywords: "relink missing audio search folder tags length tolerance preselect",
    load: () => relink.loadSettings(),
    how: "inline",
    component: RelinkSettings,
  },
];

export default entries;
