import type { ViewDef } from "../registry";
import SettingsView from "./SettingsView.svelte";

const view: ViewDef = { id: "settings", label: "Settings", section: "tools", order: 99, component: SettingsView };
export default view;
