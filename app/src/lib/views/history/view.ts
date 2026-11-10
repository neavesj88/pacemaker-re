import type { ViewDef } from "../registry";
import HistoryView from "./HistoryView.svelte";

const view: ViewDef = { id: "history", label: "History", section: "library", order: 40, component: HistoryView };
export default view;
