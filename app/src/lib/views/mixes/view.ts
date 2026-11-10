import type { ViewDef } from "../registry";
import MixesView from "./MixesView.svelte";

const view: ViewDef = { id: "mixes", label: "Mixes", section: "library", order: 20, component: MixesView, count: (l) => l.mixes.length };
export default view;
