import type { ViewDef } from "../registry";
import HelpView from "./HelpView.svelte";

// After Settings, the last entry of Tools. It is also reached with F1 and from the "?" links.
const view: ViewDef = { id: "help", label: "Help", section: "tools", order: 100, component: HelpView };
export default view;
