// Who owns the Space key. Space is the player's play / pause (init.ts), but a view that has its own
// transport (the mix editor's live preview, the grid editor's play button) listens on the window
// as well, and two window listeners must never both act on one key press. A view registers a
// claim while it is the one on screen; init.ts leaves Space alone while any claim says yes, and
// when an element handled the key itself (it called preventDefault, as a table row does) too.

const claims = new Set<() => boolean>();

/** Registers a claim; it holds while `active()` is true. Returns the function that removes it. */
export function claimSpace(active: () => boolean): () => void {
  claims.add(active);
  return () => void claims.delete(active);
}

/** True while some view owns Space. */
export function spaceClaimed(): boolean {
  for (const c of claims) if (c()) return true;
  return false;
}

/** Whether the global player shortcut should handle this Space press: nothing handled it already
 *  and no view has claimed it. `eventHandled` is the event's `defaultPrevented`. */
export function playerOwnsSpace(eventHandled: boolean): boolean {
  return !eventHandled && !spaceClaimed();
}

/** A mix that keeps playing after its tab was left has no transport on screen, so Space pauses it
 *  first (the player bar says so). `view` is the open view's id; on the Mixes view the editor's own
 *  claim handles Space. */
export function spaceStopsMix(mixPlaying: boolean, view: string): boolean {
  return mixPlaying && view !== "mixes";
}

/** The button the last mouse press landed on. `:focus-visible` cannot tell mouse focus from keyboard
 *  focus here: the browser turns it on at the first key press, which is this very Space. */
let pressed: Element | null = null;
if (typeof window !== "undefined") {
  window.addEventListener("pointerdown", (e) => void (pressed = e.target instanceof Element ? e.target.closest("button, [role=button]") : null), true);
  // Tab (either way) is the keyboard taking over the focus
  window.addEventListener("keydown", (e) => void (e.key === "Tab" && (pressed = null)), true);
}

/** A button the mouse left focused (a toolbar button clicked a moment ago) does not want Space: the
 *  user is playing the mix, not pressing it again. A button reached with the keyboard keeps Space
 *  for itself. */
export function mouseFocusedButton(el: Element | null): boolean {
  return el !== null && el === pressed && el === document.activeElement;
}
