import { energySource } from "$lib/state/energySource.svelte";
import type { SettingsEntry } from "../settings/types";
import CleanupRecipes from "./CleanupRecipes.svelte";
import EnergySourceSelect from "./EnergySourceSelect.svelte";
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
    component: CleanupRecipes,
  },
];

export default entries;
