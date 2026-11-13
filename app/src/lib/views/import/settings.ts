import { watch } from "$lib/state/watch.svelte";
import type { SettingsEntry } from "../settings/types";
import WatchFolders from "./WatchFolders.svelte";

const entries: SettingsEntry[] = [
  {
    id: "watch-folders",
    area: "Import and export",
    title: "Watch folders",
    help: "Folders on this computer the app checks for new music, how often, how old a file must be and when it adds tracks by itself.",
    keywords: "watch folders import new music interval ignore tolerance automatic",
    load: () => watch.loadSettings(),
    how: "inline",
    component: WatchFolders,
  },
];

export default entries;
