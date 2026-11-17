import type { SettingsEntry } from "../settings/types";
import ConnectSettings from "./ConnectSettings.svelte";

const entries: SettingsEntry[] = [
  {
    id: "pacemaker-connect",
    area: "Library",
    title: "Pacemaker connected",
    help: "Whether a small prompt offers to open a Pacemaker when one is plugged in, and how often the app looks for one.",
    keywords: "pacemaker connected plug in plugged usb drive detect popup prompt ask open notification dont ask again check interval seconds rescan",
    how: "inline",
    component: ConnectSettings,
  },
];

export default entries;
