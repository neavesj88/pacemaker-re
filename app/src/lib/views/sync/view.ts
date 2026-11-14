import { session } from "$lib/state/session.svelte";
import type { ViewDef } from "../registry";
import SyncView from "./SyncView.svelte";

// Offered once there is a library on this computer to sync (or one is open); without one the
// Welcome screen leads to making it.
const view: ViewDef = {
  id: "sync",
  label: "Sync",
  section: "library",
  order: 70,
  component: SyncView,
  visible: () => !!session.computerOffer?.exists || session.isComputer,
};
export default view;
