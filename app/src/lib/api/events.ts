// Typed wrappers over Tauri events. The names mirror src-tauri/src/events.rs. An area that adds
// an event adds its name + wrapper in its own api/<area>.ts and uses `subscribeAll` here.
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DoneEvent, ProgressEvent } from "./types";

export const JOB_PROGRESS = "job-progress";
export const JOB_DONE = "job-done";

export type Unlisten = () => void;

/** Starts several listeners and returns one function that stops them all, even ones whose
 *  registration was still pending. A start that throws (no Tauri in a plain browser) is ignored. */
export function subscribeAll(starts: (() => Promise<UnlistenFn>)[]): Unlisten {
  let gone = false;
  const stops: UnlistenFn[] = [];
  for (const start of starts) {
    try {
      start()
        .then((u) => (gone ? u() : stops.push(u)))
        .catch(() => {});
    } catch {
      /* browser preview */
    }
  }
  return () => {
    gone = true;
    stops.forEach((u) => u());
  };
}

export const onJobProgress = (cb: (e: ProgressEvent) => void) => listen<ProgressEvent>(JOB_PROGRESS, (e) => cb(e.payload));
export const onJobDone = (cb: (e: DoneEvent) => void) => listen<DoneEvent>(JOB_DONE, (e) => cb(e.payload));
