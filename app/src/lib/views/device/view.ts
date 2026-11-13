import { session } from "$lib/state/session.svelte";
import type { ViewDef } from "../registry";
import DeviceView from "./DeviceView.svelte";

const view: ViewDef = { id: "device", label: "Device", section: "tools", order: 20, component: DeviceView, title: () => (session.isComputer ? "This library" : "Device") };
export default view;
