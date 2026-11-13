import { smartSettings } from "$lib/state/smartSettings.svelte";
import type { SettingsEntry } from "../settings/types";
import SmartSettingsDialog from "./SmartSettingsDialog.svelte";

const REFRESH = { ask: "ask before refreshing", auto: "refresh by themselves", off: "refresh only on request" } as const;

const entries: SettingsEntry[] = [
  {
    id: "smart-cases",
    area: "Cases",
    title: "Smart cases",
    help: "Whether the Pacemaker copies of smart cases refresh by themselves, and how smart cases appear in exports.",
    keywords: "smart cases refresh copies rekordbox xml export folder",
    load: () => smartSettings.ensure(),
    how: "dialog",
    component: SmartSettingsDialog,
    summary: () => {
      const s = smartSettings.value;
      return `Copies ${REFRESH[s.refresh_mode]}. The whole-library XML ${s.export_smart ? "includes" : "leaves out"} smart cases.`;
    },
  },
];

export default entries;
