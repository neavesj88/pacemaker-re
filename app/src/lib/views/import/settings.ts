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
  {
    id: "interop",
    area: "Import and export",
    title: "Playlist, setlist and VirtualDJ import and export",
    help: "The choices of each import preview and export (cue points, colours, smart cases, formats, paths) are made in the import and export dialogs, which need a file or a case to work on.",
    keywords: "import export rekordbox m3u serato traktor virtualdj setlist matcher csv",
    how: "view",
    view: "import",
    button: "Go to Import",
  },
];

export default entries;
