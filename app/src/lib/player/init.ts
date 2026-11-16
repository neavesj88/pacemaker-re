// Startup hook (run by the shell): starts the playback engine and the player's keyboard
// shortcuts. Space plays / pauses (or plays the highlighted track when nothing is loaded);
// C is the CUE button while held. Neither fires while a text field has the focus, and Space yields
// to a row, editor or view that already handled the key (see spaceClaim.ts). With the pointer over
// the mix timeline or the player's waveform, Space moves the cue / playhead to the time under the
// pointer instead (hoverCue.ts); Alt then means "exactly here, no beat snap".
import type { Unlisten } from "$lib/api/events";
import { playback } from "$lib/state/playback.svelte";
import { nav } from "$lib/state/nav.svelte";
import { selection } from "$lib/state/selection.svelte";
import { session } from "$lib/state/session.svelte";
import { engine } from "./engine.svelte";
import { preview } from "$lib/views/mixes/preview.svelte";
import { playerOwnsSpace, spaceStopsMix } from "./spaceClaim";
import { decideSpace } from "./hoverCue";
import { hoverCue } from "./hoverCue.svelte";
import { toSamples } from "$lib/views/mixes/layout";

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

  // Hover, then Space: the pointer over the mix timeline or the player's waveform wins over a row
  // or a button that happens to have the focus (right after selecting a track, or clicking Play
  // with the mouse). It runs before them (capture), so they never see the press.
  const hoverDown = (e: KeyboardEvent) => {
    if (e.code !== "Space" || e.ctrlKey || e.metaKey || typing(e.target) || !session.library) return;
    if (hoverCue.surface === null || overlayOpen()) return;
    const t = e.target;
    // a button or checkbox-like control keeps Space unless it is a plain button (those stay operable by Enter)
    if (t instanceof Element && t.closest(OWN_SPACE) && !t.closest("button, [role=button]")) return;
    const hover = decideSpace({
      surface: hoverCue.surface,
      typing: false,
      ownedByControl: false,
      mixPlayingElsewhere: spaceStopsMix(preview.playing, nav.view),
    });
    if (hover === "default") return;
    e.preventDefault(); // the page must not scroll
    e.stopPropagation();
    if (e.repeat) return;
    if (hover === "move-mix-cue") void preview.seek(toSamples(hoverCue.secs));
    else {
      // the cue flag moves; a track that is playing carries on from there
      playback.setCue(hoverCue.secs);
      if (playback.playing) playback.seek(hoverCue.secs);
    }
  };

  const down = (e: KeyboardEvent) => {
    if (e.key === "Alt") hoverCue.exact = true;
    if (e.ctrlKey || e.metaKey || typing(e.target) || !session.library) return;
    // Alt is only for the hover cue (exact time); every other shortcut leaves Alt chords alone
    if (e.altKey && !(e.code === "Space" && hoverCue.surface !== null)) return;
    if (e.code === "Space") {
      if (!spaceFree(e.target)) return;
      // Alt+Space only ever means the hover cue; with nothing to move it is not ours
      if (e.altKey) return;
      // a table row, the mix editor or the grid editor already acted on this press: one key, one
      // action (two toggles in a row left the player paused)
      if (!playerOwnsSpace(e.defaultPrevented)) return;
      e.preventDefault(); // the page must not scroll
      if (e.repeat) return;
      // a mix left playing on another tab: Space is its pause (the player bar says it is playing)
      if (spaceStopsMix(preview.playing, nav.view)) {
        preview.pause();
        return;
      }
      if (playback.trackId === null && playback.mixPreview === null) {
        const id = selection.trackIds[0];
        // an empty queue makes next / previous follow the Tracks table as it is at that moment
        // (its search, filters and sort), instead of a list frozen when playing started
        if (id !== undefined) playback.load(id, true, nav.view === "tracks" ? [] : undefined);
      } else playback.toggle();
    } else if (e.code === "KeyC" && !e.repeat && !e.shiftKey && !overlayOpen()) {
      engine.cuePress();
    }
  };
  const up = (e: KeyboardEvent) => {
    if (e.key === "Alt") hoverCue.exact = false;
    if (typing(e.target)) return;
    if (e.code === "Space") {
      // the button that had the press taken by the hover cue must not click on release
      const t = e.target;
      const hoverButton = hoverCue.surface !== null && !overlayOpen() && t instanceof Element && t.closest("button, [role=button]") !== null;
      if (session.library && (spaceFree(e.target) || hoverButton)) e.preventDefault();
    } else if (e.code === "KeyC") engine.cueRelease();
  };
  // losing the window while CUE is held must not leave it playing
  const blur = () => {
    engine.cueRelease();
    hoverCue.exact = false;
    hoverCue.clear();
  };

  window.addEventListener("keydown", hoverDown, true);
  window.addEventListener("keydown", down);
  window.addEventListener("keyup", up);
  window.addEventListener("blur", blur);
  return () => {
    window.removeEventListener("keydown", hoverDown, true);
    window.removeEventListener("keydown", down);
    window.removeEventListener("keyup", up);
    window.removeEventListener("blur", blur);
    stopEngine();
  };
}
