import { mixExport } from "$lib/state/mixexport.svelte";
import { tracklist } from "$lib/state/tracklist.svelte";
import type { SettingsEntry } from "../settings/types";
import MixExportSettings from "./MixExportSettings.svelte";
import RecipeSettings from "./RecipeSettings.svelte";
import TracklistFields from "./TracklistFields.svelte";

const entries: SettingsEntry[] = [
  {
    id: "auto-transitions",
    area: "Mixes",
    title: "Auto transitions",
    help: "Which recipe the Auto buttons build, how long it is, where a hard cut falls and what happens to your own tools that are in the way.",
    keywords: "mix transition recipe crossfade cut bars auto sections",
    how: "inline",
    component: RecipeSettings,
  },
  {
    id: "mix-export",
    area: "Mixes",
    title: "Mix audio export",
    help: "Format and quality, target loudness, tags and cover, the gain limits and how a part of a mix ends. They are the values the export dialog opens with.",
    keywords: "export flac mp3 ogg wav loudness lufs peak tags cover chapters limiter",
    load: () => mixExport.ensureLoaded(),
    how: "inline",
    component: MixExportSettings,
  },
  {
    id: "tracklist",
    area: "Mixes",
    title: "Track list and CUE sheet",
    help: "Where a track's start time falls, how seconds are rounded, how close tracks are joined for YouTube, and whether a CUE sheet is written next to every audio export.",
    keywords: "track list cue sheet youtube chapters mixcloud csv numbered",
    load: () => tracklist.ensureLoaded(),
    how: "inline",
    component: TracklistFields,
  },
];

export default entries;
