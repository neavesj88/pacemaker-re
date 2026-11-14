import { session } from "$lib/state/session.svelte";
import type { SettingsEntry } from "./types";
import ComputerLibrarySettings from "./ComputerLibrarySettings.svelte";
import InterfaceSettings from "./InterfaceSettings.svelte";
import Licences from "./Licences.svelte";

const entries: SettingsEntry[] = [
  {
    id: "interface",
    area: "Appearance",
    title: "Interface size and messages",
    help: "How big everything in the window is, and how long error messages stay on screen.",
    keywords: "size zoom scale bigger smaller large text monitor display 4k ultrawide error message toast notification seconds",
    how: "inline",
    component: InterfaceSettings,
  },
  {
    id: "computer-library",
    area: "Library",
    title: "Library on this computer",
    help: "Where your own library is kept (tracks, mixes, cases) and whether it opens when the app starts.",
    keywords: "library computer folder path music start open default location pacemaker revival",
    load: () => session.refreshComputer(),
    how: "inline",
    component: ComputerLibrarySettings,
  },
  {
    id: "licences",
    area: "About",
    title: "Licences of included software",
    help: "The open-source packages this app contains and the licence each one is under.",
    keywords: "about third party notices licence license open source credits attribution",
    how: "inline",
    component: Licences,
  },
];

export default entries;
