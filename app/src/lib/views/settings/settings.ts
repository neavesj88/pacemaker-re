import { session } from "$lib/state/session.svelte";
import type { SettingsEntry } from "./types";
import ComputerLibrarySettings from "./ComputerLibrarySettings.svelte";
import Licences from "./Licences.svelte";

const entries: SettingsEntry[] = [
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
