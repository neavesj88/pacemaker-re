import { duplicates } from "$lib/state/duplicates.svelte";
import type { SettingsEntry } from "../settings/types";
import DupSettingsDialog from "./DupSettingsDialog.svelte";

const KEEPER = { bitrate: "highest bitrate", plays: "most plays", newest: "newest" } as const;

const entries: SettingsEntry[] = [
  {
    id: "duplicates",
    area: "Cases",
    title: "Duplicate finder",
    help: "How strict each way of finding copies is, which copy is suggested for keeping, and what a merge does with cases, ratings and plays.",
    keywords: "duplicates copies merge keeper fingerprint sound tags size length",
    load: () => duplicates.ensureSettings(),
    how: "dialog",
    component: DupSettingsDialog,
    summary: () => {
      const s = duplicates.settings;
      const ways = [s.by_tags && "artist and title", s.by_size && "size and length", s.by_audio && "sound"].filter(Boolean).join(", ");
      return `Finds copies by ${ways || "nothing (every way is off)"}. Suggests keeping the one with the ${KEEPER[s.keeper_rule]}.`;
    },
  },
];

export default entries;
