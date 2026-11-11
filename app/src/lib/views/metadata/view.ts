import type { ViewDef } from "../registry";
import TidyView from "./TidyView.svelte";

const view: ViewDef = { id: "metatidy", label: "Tidy metadata", section: "tools", order: 60, component: TidyView };
export default view;
