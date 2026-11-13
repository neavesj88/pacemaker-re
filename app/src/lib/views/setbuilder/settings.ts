import type { SettingsEntry } from "../settings/types";
import SetSettings from "./SetSettings.svelte";

const entries: SettingsEntry[] = [
  {
    id: "set-builder",
    area: "Mixes",
    title: "Set builder",
    help: "Which key moves count as good, tempo limits, the energy curve, the target length and how the set becomes a case or a mix.",
    keywords: "set builder camelot key moves tempo energy curve length sections intro outro",
    how: "inline",
    component: SetSettings,
    props: { hasEnergy: true },
  },
];

export default entries;
