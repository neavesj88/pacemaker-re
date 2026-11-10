// Startup hook (run by the shell): starts the playback engine and the player's keyboard
// shortcuts. Space plays / pauses (or plays the highlighted track when nothing is loaded);
// C is the CUE button while held. Neither fires while a text field has the focus.
import type { Unlisten } from "$lib/api/events";
import { playback } from "$lib/state/playback.svelte";
import { selection } from "$lib/state/selection.svelte";
import { session } from "$lib/state/session.svelte";
import { engine } from "./engine.svelte";

/** True when the key press belongs to something being typed into. */
function typing(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false;
  if (t.isContentEditable) return true;
  if (t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement) return true;
  if (t instanceof HTMLInputElement) return !["button", "checkbox", "radio", "range", "submit", "reset"].includes(t.type);
  return false;
}

export default function init(): Unlisten {
  const stopEngine = engine.start();

  const down = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey || typing(e.target) || !session.library) return;
    if (e.code === "Space") {
      // also keeps a focused button from being "clicked" by the same key
      e.preventDefault();
      if (e.repeat) return;
      if (playback.trackId === null && playback.mixPreview === null) {
        const id = selection.trackIds[0];
        if (id !== undefined) playback.load(id, true);
      } else playback.toggle();
    } else if (e.code === "KeyC" && !e.repeat) {
      engine.cuePress();
    }
  };
  const up = (e: KeyboardEvent) => {
    if (typing(e.target)) return;
    if (e.code === "Space" && session.library) e.preventDefault();
    else if (e.code === "KeyC") engine.cueRelease();
  };
  // losing the window while CUE is held must not leave it playing
  const blur = () => engine.cueRelease();

  window.addEventListener("keydown", down);
  window.addEventListener("keyup", up);
  window.addEventListener("blur", blur);
  return () => {
    window.removeEventListener("keydown", down);
    window.removeEventListener("keyup", up);
    window.removeEventListener("blur", blur);
    stopEngine();
  };
}
