import type { SettingsEntry } from "../settings/types";
import VdjOptionsFields from "./VdjOptionsFields.svelte";

const entries: SettingsEntry[] = [
  {
    id: "virtualdj-export",
    area: "Import and export",
    title: "VirtualDJ list export",
    help: "Whether the audio is copied next to the list, how the copies are named, the folder VirtualDJ will see them in and the list's own settings.",
    keywords: "virtualdj vdj list export copy audio ordered duplicates path",
    how: "inline",
    component: VdjOptionsFields,
  },
];

export default entries;
