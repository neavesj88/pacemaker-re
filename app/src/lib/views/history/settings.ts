import { playlog } from "$lib/state/playlog.svelte";
import type { SettingsEntry } from "../settings/types";
import PlayLogSettings from "./PlayLogSettings.svelte";

const entries: SettingsEntry[] = [
  {
    id: "play-log",
    area: "History",
    title: "Play log",
    help: "How the app turns the Pacemaker's play counters into a log of sessions: the gap that starts a new session, how long it is kept and how it identifies a track.",
    keywords: "history play log sessions retention counters",
    load: () => playlog.loadSettings(),
    how: "inline",
    component: PlayLogSettings,
  },
];

export default entries;
