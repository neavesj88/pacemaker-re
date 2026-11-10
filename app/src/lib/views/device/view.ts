import type { ViewDef } from "../registry";
import DeviceView from "./DeviceView.svelte";

const view: ViewDef = { id: "device", label: "Device", section: "tools", order: 20, component: DeviceView };
export default view;
