import type { ViewDef } from "../registry";
import DuplicatesView from "./DuplicatesView.svelte";

const view: ViewDef = { id: "duplicates", label: "Duplicates", section: "library", order: 50, component: DuplicatesView };
export default view;
