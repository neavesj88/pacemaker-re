import { mixExport } from "$lib/state/mixexport.svelte";
import { tracklist } from "$lib/state/tracklist.svelte";
import type { SettingsEntry } from "../settings/types";
import FollowSettings from "./FollowSettings.svelte";
import MixExportSettings from "./MixExportSettings.svelte";
import RecipeSettings from "./RecipeSettings.svelte";
import SnapSettings from "./SnapSettings.svelte";
import TempoSettings from "./TempoSettings.svelte";
import TracklistFields from "./TracklistFields.svelte";
import ViewSettings from "./ViewSettings.svelte";

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
    id: "mix-snap",
    area: "Mixes",
    title: "Snap and beat-align",
    help: "Whether a clip dragged over or next to another is lined up with its beat grid, and on what: beats, bars, phrases or sections. Alt while dragging moves freely.",
    keywords: "mix snap beat bar phrase grid align beatmatch drag magnet",
    how: "inline",
    component: SnapSettings,
  },
  {
    id: "mix-view",
    area: "Mixes",
    title: "Mix editor view",
    help: "How the mix editor looks: clips that shrink as you zoom out (or stay full height), the whole-mix strip above the ruler, and the section markers on the clips. The editor's View menu has the same choices.",
    keywords: "mix view clip height lane shrink zoom out full height whole mix strip overview minimap sections markers timeline lane size",
    how: "inline",
    component: ViewSettings,
  },
  {
    id: "mix-follow",
    area: "Mixes",
    title: "Following the playhead",
    help: "Whether the timeline scrolls while a mix plays and how: smoothly with the playhead (the default) or a page at a time when it reaches the edge, and where the playhead sits. The beat grid editor's Follow uses it too.",
    keywords: "mix follow playhead scroll smooth page jump timeline playing auto scroll grid editor",
    how: "inline",
    component: FollowSettings,
  },
  {
    id: "mix-tempo",
    area: "Mixes",
    title: "Tempo matching",
    help: "How a track is matched to the mix tempo (half and double time included), how much stretch is applied without a warning, and what a new track does when it would need more, and how much memory the live player uses.",
    keywords: "mix tempo bpm match stretch half double time warning sync memory ram playback decode",
    how: "inline",
    component: TempoSettings,
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
