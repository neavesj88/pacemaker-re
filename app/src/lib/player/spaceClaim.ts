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
