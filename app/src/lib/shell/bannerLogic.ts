// When the red error banner clears by itself. Pure, so it runs under plain node (bannerLogic.test.mjs).

/** What the banner remembers: the message on screen and the view it first appeared on. */
export type ErrorSeen = { text: string; view: string };

/** `next` is what to remember; `clear` is true when the message is a leftover from another view and
 *  should go: an error is about what the user was doing, so it stays while they look at that view and
 *  is dropped when they go elsewhere. A message that appears after the move (a new action) stays. */
export function errorScope(seen: ErrorSeen, text: string, view: string): { next: ErrorSeen; clear: boolean } {
  if (!text) return { next: { text: "", view }, clear: false };
  if (text !== seen.text) return { next: { text, view }, clear: false };
  return { next: seen, clear: view !== seen.view };
}
