// What the Device view's state does when a device job ends. Pure, so it runs under plain node
// (reloadLogic.test.mjs). A job ends long after it started: the user may have moved to another view,
// switched library or unplugged the drive, and the old root must not be read then (a missing drive
// shows as a red error banner after a job that succeeded).
import { sameRoot } from "../../state/place.ts";

export type ReloadPlan = "load" | "clear" | "forget" | "none";

/** `deviceRoot`: the root the Device view last read ("" if it never did). `libraryRoot`: the open
 *  library's root (null when none). `viewShown`: the Device view is the one on screen.
 *  - load: read it again now (it is open and on screen);
 *  - clear: it is open but not on screen: drop what was read, the view reads it when it is shown;
 *  - forget: what was read belongs to another library: drop it and the root;
 *  - none: nothing to do (never read, no library, or the library was ejected). */
export function reloadPlan(deviceRoot: string, libraryRoot: string | null, detached: boolean, viewShown: boolean): ReloadPlan {
  if (!deviceRoot || !libraryRoot || detached) return "none";
  if (!sameRoot(deviceRoot, libraryRoot)) return "forget";
  return viewShown ? "load" : "clear";
}
