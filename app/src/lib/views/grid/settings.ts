import { suspects } from "$lib/state/gridSuspects.svelte";
import type { SettingsEntry } from "../settings/types";

const entries: SettingsEntry[] = [
  {
    id: "suspect-grid",
    area: "Analysis",
    title: "Suspect beat grids",
    help: "The tempo window for each genre, the confidence floor and the other numbers behind the Suspect grid filter and its suggested fixes.",
    keywords: "grid beat suspect tempo window genre confidence floor batch fix",
    load: () => suspects.ensure(),
    how: "open",
    open: () => (suspects.editing = true),
    button: "Change…",
    summary: () => `Flags grids below ${suspects.rules.min_confidence_pct}% confidence. ${suspects.auto ? "Checks the library whenever it loads." : "Checks only when asked."}`,
  },
];

export default entries;
