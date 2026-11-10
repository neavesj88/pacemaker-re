// Startup hook (run by the shell): starts the playback engine and the player's keyboard
// shortcuts. Space plays / pauses (or plays the highlighted track when nothing is loaded);
// C is the CUE button while held. Neither fires while a text field has the focus.
import type { Unlisten } from "$lib/api/events";
import { playback } from "$lib/state/playback.svelte";
import { nav } from "$lib/state/nav.svelte";
import { visibleTracks } from "$lib/views/tracks/trackFilter";
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

/** Elements that already do something with Space (and Enter): the shortcut must leave them alone. */
const OWN_SPACE =
  "button, a[href], summary, select, [role=button], [role=checkbox], [role=radio], [role=switch], [role=menuitem], " +
  "[role=option], [role=tab], input[type=checkbox], input[type=radio], input[type=button], " +
  "input[type=submit], input[type=reset]";

/** A confirm / delete dialog or menu is open: the keyboard belongs to it. */
function overlayOpen(): boolean {
  return document.querySelector("dialog[open], [role=dialog], [role=alertdialog], [aria-modal=true], [role=menu]") !== null;
}

/** Space should drive the player here: not on a control, not under an open dialog. */
function spaceFree(t: EventTarget | null): boolean {
  if (overlayOpen()) return false;
  return !(t instanceof Element && t.closest(OWN_SPACE));
}

export default function init(): Unlisten {
  const stopEngine = engine.start();

  const down = (e: KeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey || typing(e.target) || !session.library) return;
    if (e.code === "Space") {
      if (!spaceFree(e.target)) return;
      e.preventDefault(); // the page must not scroll
      if (e.repeat) return;
      if (playback.trackId === null && playback.mixPreview === null) {
        const id = selection.trackIds[0];
        // next / previous then follow the table the user is looking at (its filter and sort)
        if (id !== undefined) playback.load(id, true, nav.view === "tracks" ? visibleTracks(session.library.tracks).map((t) => t.track_id) : undefined);
      } else playback.toggle();
    } else if (e.code === "KeyC" && !e.repeat && !e.shiftKey && !overlayOpen()) {
      engine.cuePress();
    }
  };
  const up = (e: KeyboardEvent) => {
    if (typing(e.target)) return;
    if (e.code === "Space") {
      if (session.library && spaceFree(e.target)) e.preventDefault();
    } else if (e.code === "KeyC") engine.cueRelease();
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
