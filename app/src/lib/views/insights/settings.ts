import type { SettingsEntry } from "../settings/types";
import GapSettingsFields from "./GapSettingsFields.svelte";

const entries: SettingsEntry[] = [
  {
    id: "gap-map",
    area: "Analysis",
    title: "Library gap map",
    help: "The width of a BPM band, the BPM range, how many genres are shown and when a cell or row counts as thin.",
    keywords: "gap map insights bpm band genres thin heatmap",
    how: "inline",
    component: GapSettingsFields,
  },
];

export default entries;
