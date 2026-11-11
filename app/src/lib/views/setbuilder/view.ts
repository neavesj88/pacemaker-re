import type { ViewDef } from "../registry";
import SetBuilderView from "./SetBuilderView.svelte";

const view: ViewDef = { id: "setbuilder", label: "Set builder", section: "tools", order: 15, component: SetBuilderView };
export default view;
