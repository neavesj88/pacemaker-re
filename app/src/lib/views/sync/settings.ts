import type { SettingsEntry } from "../settings/types";
import SyncSettingsFields from "./SyncSettingsFields.svelte";
import { sync } from "./sync.svelte";

const entries: SettingsEntry[] = [
  {
    id: "sync",
    area: "Library",
    title: "Sync with a Pacemaker",
    help: "What happens when a track, mix or case is on both sides, whether anything is ever removed, and whether tags and labels travel with the tracks.",
    keywords: "sync send get pacemaker computer merge replace remove play counts labels tags newer same name",
    load: () => sync.loadSettings(),
    how: "inline",
    component: SyncSettingsFields,
  },
];

export default entries;
