import { energy } from "$lib/state/energy.svelte";
import { sections } from "$lib/state/sections.svelte";
import type { SettingsEntry } from "../settings/types";
import KeySettingsFields from "./KeySettingsFields.svelte";

const entries: SettingsEntry[] = [
  {
    id: "key-detection",
    area: "Analysis",
    title: "Key detection",
    help: "Whether Detect keys replaces keys that are already set, and the confidence a result needs before it is written.",
    keywords: "key detect camelot confidence overwrite",
    how: "inline",
    component: KeySettingsFields,
  },
  {
    id: "energy-auto",
    area: "Analysis",
    title: "Energy (auto)",
    help: "How the 1 to 10 estimate is scaled (fit to the library or fixed), the percentiles, the shortest track measured and how many tracks are analysed at once.",
    keywords: "energy auto estimate scale percentile threads",
    how: "open",
    open: () => energy.openPanel(),
    button: "Open the panel",
    summary: () => {
      const s = energy.state?.settings;
      return s ? `${s.scale === "fit" ? "Scale fitted to this library" : "Fixed scale"}; no estimate under ${s.min_secs} s.` : "";
    },
  },
  {
    id: "sections",
    area: "Analysis",
    title: "Sections",
    help: "How sensitive section detection is, the shortest section, the phrase length boundaries snap to and where the phrase grid starts.",
    keywords: "sections intro build drop breakdown outro phrase snap sensitivity",
    how: "open",
    open: () => sections.openPanel(),
    button: "Open the panel",
    summary: () => {
      const s = sections.state?.settings;
      return s ? `Sensitivity ${s.sensitivity} of 5; sections of at least ${s.min_section_bars} bars.` : "";
    },
  },
];

export default entries;
