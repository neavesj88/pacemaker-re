import type { ViewDef } from "../registry";
import StemsView from "./StemsView.svelte";

const view: ViewDef = { id: "stems", label: "Stems", section: "tools", order: 35, component: StemsView };
export default view;
