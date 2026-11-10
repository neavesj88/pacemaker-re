import type { ViewDef } from "../registry";
import ImportView from "./ImportView.svelte";

const view: ViewDef = { id: "import", label: "Import", section: "tools", order: 30, component: ImportView };
export default view;
