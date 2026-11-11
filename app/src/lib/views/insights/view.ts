import type { ViewDef } from "../registry";
import GapMapView from "./GapMapView.svelte";

const view: ViewDef = { id: "gap-map", label: "Gap map", section: "library", order: 50, component: GapMapView };
export default view;
