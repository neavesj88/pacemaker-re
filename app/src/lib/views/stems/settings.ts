import { stems } from "$lib/state/stems.svelte";
import type { SettingsEntry } from "../settings/types";
import SettingsCard from "./SettingsCard.svelte";

const entries: SettingsEntry[] = [
  {
    id: "stems",
    area: "Stems",
    title: "Stem separation",
    help: "What the separation engine runs on, the output format, the overlap and the model and runtime locations.",
    keywords: "stems vocal instrumental separation model onnx cpu gpu format overlap",
    load: () => stems.refresh(),
    how: "inline",
    component: SettingsCard,
  },
];

export default entries;
