// The words of a toast: the rule that keeps them short, and the pure helpers that apply it. Pure, so it
// runs under plain node (toastText.test.mjs).
//
// THE RULE: one sentence, at most 12 words (never more than 15). Say what happened, with one rounded
// number ("Moved 0.4 s later", "Matched 5 tracks to 128 BPM"). No lists of names (count them), no how-it-
// works and no setting names (that is Help). An error says what could not be done plus the first clause
// of the reason; the full text goes behind the card's Details button.

/** The length a toast should aim for, and the length it never exceeds (the guard test checks both). */
export const TOAST_WORDS = 12;
export const TOAST_HARD = 15;

export function wordCount(text: string): number {
  return text.split(/\s+/).filter(Boolean).length;
}

/** The text up to its first full stop, semicolon or opening bracket, at most `max` words. */
export function firstClause(text: string, max = 10): string {
  const flat = text.replace(/\s+/g, " ").trim();
  const cut = flat.search(/(?:[.;!?](?:\s|$))|\s\(/);
  const head = (cut >= 0 ? flat.slice(0, cut) : flat).trim();
  const words = head.split(" ");
  return words.length <= max ? head : words.slice(0, max).join(" ") + "…";
}

/** The readable text of whatever a failed call threw (a string from Rust, an Error, anything else). */
export function errorMessage(e: unknown): string {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  return raw.replace(/^(?:Error|invalid):\s*/i, "").trim();
}

/** `head` ("Couldn't save the mix.") plus the first clause of the reason; the whole reason is the details. */
export function errorToast(head: string, e: unknown): { text: string; details?: string } {
  const full = errorMessage(e);
  const h = /[.!?]$/.test(head) ? head : head + ".";
  const room = Math.max(0, Math.min(10, TOAST_HARD - wordCount(h)));
  const reason = room > 1 ? firstClause(full, room) : "";
  const lower = h.toLowerCase();
  const same = reason === "" || lower.includes(reason.toLowerCase());
  const cap = reason.charAt(0).toUpperCase() + reason.slice(1);
  const text = same ? h : `${h} ${/[.!?…]$/.test(cap) ? cap : cap + "."}`;
  return { text, details: full && full !== reason ? full : undefined };
}

/** The safety net for text built at run time (a Rust summary, a list of names): over 15 words, the card
 *  shows the first sentence, cut to 15 words, and the whole text moves to Details. */
export function condense(text: string): { text: string; details?: string } {
  const flat = text.replace(/\s+/g, " ").trim();
  if (wordCount(flat) <= TOAST_HARD) return { text: flat };
  const sentence = flat.split(/(?<=[.!?])\s+/)[0];
  const first = wordCount(sentence) <= TOAST_HARD ? sentence : firstClauseOrCut(sentence);
  return { text: first, details: text.trim() };
}

/** A sentence still over the limit: cut at the last comma inside 15 words, else at 15 words with an ellipsis. */
function firstClauseOrCut(sentence: string): string {
  const clause = sentence.split(/;\s/)[0];
  const words = clause.split(" ");
  if (words.length <= TOAST_HARD) return clause;
  const head = words.slice(0, TOAST_HARD).join(" ");
  const comma = head.lastIndexOf(",");
  return comma > 12 ? head.slice(0, comma) + "." : head + "…";
}

/** How long a success or info toast stays: time to read it, 3 to 7 seconds. */
export function autoMs(text: string): number {
  return Math.min(7000, Math.max(3000, 2500 + 300 * wordCount(text)));
}
