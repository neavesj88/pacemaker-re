import type { SettingsEntry } from "../settings/types";
import SettingsPanel from "./SettingsPanel.svelte";
import { space } from "./space.svelte";

const entries: SettingsEntry[] = [
  {
    id: "free-space",
    area: "Device",
    title: "Free-space planner",
    help: "The target, which tracks are nominated and why, which are never suggested, and where archives go.",
    keywords: "free space planner archive card remove never played stale rating duplicates",
    load: () => space.loadSettings(),
    how: "inline",
    component: SettingsPanel,
  },
];

export default entries;
