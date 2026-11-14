import { session } from "$lib/state/session.svelte";
import type { ViewDef } from "../registry";
import LibrariesView from "./LibrariesView.svelte";

// Two libraries side by side, with drag and drop between them. Offered once there is a library on
// this computer (or one is open) or another library is on hand to put beside the open one.
const view: ViewDef = {
  id: "libraries",
  label: "Libraries",
  section: "library",
  order: 65,
  component: LibrariesView,
  visible: () => !!session.computerOffer?.exists || session.isComputer || session.volumes.length > (session.library && !session.isComputer ? 1 : 0),
};
export default view;
