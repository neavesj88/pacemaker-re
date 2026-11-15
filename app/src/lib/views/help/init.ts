// Startup hook (run by the shell): F1 opens Help from anywhere, and Ctrl+/ opens it with the cursor
// in the search box. Neither does anything while a dialog is open, because Help would appear behind it.
import type { Unlisten } from "$lib/api/events";
import { openHelp, trackReturnView } from "./help.svelte";

function dialogOpen(): boolean {
  return document.querySelector("dialog[open], [aria-modal=true]") !== null;
}

export default function init(): Unlisten {
  const down = (e: KeyboardEvent) => {
    if (e.defaultPrevented || dialogOpen()) return;
    if (e.key === "F1" && !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
      e.preventDefault();
      openHelp();
    } else if ((e.ctrlKey || e.metaKey) && !e.altKey && (e.key === "/" || e.code === "Slash")) {
      e.preventDefault();
      openHelp(undefined, { focusSearch: true });
    }
  };
  window.addEventListener("keydown", down);
  const stopTracking = trackReturnView();
  return () => {
    window.removeEventListener("keydown", down);
    stopTracking();
  };
}
