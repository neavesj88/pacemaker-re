import type { SettingsEntry } from "./types";
import Licences from "./Licences.svelte";

const entries: SettingsEntry[] = [
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
