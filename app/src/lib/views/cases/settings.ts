import { smartSettings } from "$lib/state/smartSettings.svelte";
import type { SettingsEntry } from "../settings/types";
import { caseExport } from "$lib/state/caseExport.svelte";
import CaseExportSettings from "./CaseExportSettings.svelte";
import SmartSettingsDialog from "./SmartSettingsDialog.svelte";

const REFRESH = { ask: "ask before refreshing", auto: "refresh by themselves", off: "refresh only on request" } as const;

const entries: SettingsEntry[] = [
  {
    id: "smart-cases",
    area: "Cases",
    title: "Smart cases",
    help: "Whether the case written for each smart case (the Pacemaker copy, or the ★ case in a library on this computer) refreshes by itself, and how smart cases appear in exports.",
    keywords: "smart cases refresh copies rekordbox xml export folder",
    load: () => smartSettings.ensure(),
    how: "dialog",
    component: SmartSettingsDialog,
    summary: () => {
      const s = smartSettings.value;
      return `Copies ${REFRESH[s.refresh_mode]}. The whole-library XML ${s.export_smart ? "includes" : "leaves out"} smart cases.`;
    },
  },
  {
    id: "case-export",
    area: "Cases",
    title: "Case export",
    help: "The format the export dialog starts with, relative paths, whether the audio is copied and how, and what to do with a file already there.",
    keywords: "export cases folder m3u8 csv rekordbox playlist copy audio conflict relative paths",
    load: () => caseExport.ensure(),
    how: "inline",
    component: CaseExportSettings,
  },
];

export default entries;
