import type { ViewDef } from "../registry";
import GridView from "./GridView.svelte";

const view: ViewDef = { id: "grid", label: "Beat grid editor", section: "tools", order: 10, component: GridView };
export default view;
