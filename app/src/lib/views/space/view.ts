import type { ViewDef } from "../registry";
import SpaceView from "./SpaceView.svelte";

const view: ViewDef = { id: "space", label: "Free space", section: "tools", order: 21, component: SpaceView };
export default view;
