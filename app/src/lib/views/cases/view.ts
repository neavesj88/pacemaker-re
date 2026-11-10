import type { ViewDef } from "../registry";
import CasesView from "./CasesView.svelte";

const view: ViewDef = { id: "cases", label: "Cases", section: "library", order: 30, component: CasesView, count: (l) => l.cases.length };
export default view;
