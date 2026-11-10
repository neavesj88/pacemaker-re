import type { ViewDef } from "../registry";
import TracksView from "./TracksView.svelte";

const view: ViewDef = { id: "tracks", label: "Tracks", section: "library", order: 10, component: TracksView, count: (l) => l.tracks.length };
export default view;
