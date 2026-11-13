import type { SettingsEntry } from "../settings/types";
import RecipeSettings from "./RecipeSettings.svelte";

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
    title: "Mix audio export, track list and setlist",
    help: "Format, loudness, range, tags and chapters of an exported mix, and the layout of the track list and CUE sheet. They apply to the mix being exported, so they are set in its dialogs.",
    keywords: "export flac mp3 wav loudness lufs cue chapters track list setlist print",
    how: "view",
    view: "mixes",
    button: "Go to Mixes",
  },
];

export default entries;
