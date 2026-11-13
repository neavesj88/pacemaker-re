import type { SettingsEntry } from "../settings/types";
import SetlistSettings from "./SetlistSettings.svelte";

const entries: SettingsEntry[] = [
  {
    id: "setlist-print",
    area: "Import and export",
    title: "Printable setlist",
    help: "Which columns are printed, how times and keys are shown, the paper and text size and the page options.",
    keywords: "setlist print page columns paper landscape key camelot times clock",
    how: "inline",
    component: SetlistSettings,
  },
];

export default entries;
