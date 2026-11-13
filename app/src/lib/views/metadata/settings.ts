import { energySource } from "$lib/state/energySource.svelte";
import type { SettingsEntry } from "../settings/types";
import CleanupSettingsPane from "./CleanupSettingsPane.svelte";
import EnergySourceSelect from "./EnergySourceSelect.svelte";
import TagWriteFields from "./TagWriteFields.svelte";
import { cleanup } from "./cleanupState.svelte";

const entries: SettingsEntry[] = [
  {
    id: "energy-source",
    area: "Library",
    title: "Energy source",
    help: "Which energy counts where one number per track is needed: your own rating, the Energy (auto) estimate, or yours with the estimate as a fallback.",
    keywords: "energy rating auto search filter smart cases gap map set builder setlist",
    load: () => energySource.load(),
    how: "inline",
    component: EnergySourceSelect,
  },
  {
    id: "metadata-cleanup",
    area: "Library",
    title: "Metadata clean-up recipes",
    help: "The text recipes of the Tidy metadata page: spacing, case, feat. forms, promo text, replacement lists and the tag-write choices.",
    keywords: "tidy tags titles artists feat promo replace rename write back id3",
    load: () => cleanup.load(),
    how: "inline",
    component: CleanupSettingsPane,
  },
  {
    id: "tag-write",
    area: "Library",
    title: "Write tags to files",
    help: "Which of the library's tag fields are written into the audio files' own tags when you write tags back (nothing is written until you confirm the list of files).",
    keywords: "tags write back id3 audio files fields title artist album",
    load: () => cleanup.load(),
    how: "inline",
    component: TagWriteFields,
  },
];

export default entries;
