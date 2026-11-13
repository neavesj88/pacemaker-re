import type { ViewDef } from "../registry";
import MatcherView from "./MatcherView.svelte";

const view: ViewDef = { id: "matcher", label: "Track matcher", section: "tools", order: 35, component: MatcherView };
export default view;
