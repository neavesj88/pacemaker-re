// Which Wah tools of a mix are Vocal tools. The Pacemaker file has no Vocal tool: a Vocal tool is
// written as the Wah slot (PeqAudioProcess, Wah-CutOff), exactly as the card's Vocal setting always
// did. The file cannot say "this one was meant as Vocal", so the app remembers it here, in its own
// storage keyed by library and mix, for labels and for the live preview (Vocal tools render as the
// vocal filter, Wah tools as Wah). Nothing here is ever written to the device file.
//
// Copies and pastes get new tool ids; a copy of a Vocal tool is a Wah tool again until it is
// marked (the tool's inspector has the switch).
import { loadPref, savePref } from "$lib/state/prefStore";
import { session } from "$lib/state/session.svelte";

const STORAGE_KEY = "pm.mixes.vocalTools.v1";

type Saved = Record<string, string[]>;

function normalise(raw: unknown): Saved {
  if (!raw || typeof raw !== "object") return {};
  const out: Saved = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    if (Array.isArray(v)) out[k] = v.filter((x): x is string => typeof x === "string");
  }
  return out;
}

class VocalTools {
  #all = $state<Saved>(loadPref(STORAGE_KEY, normalise, () => ({})));

  #key(mixId: number): string {
    return `${session.library?.root ?? ""}#${mixId}`;
  }

  /** True when the tool (a Wah slot) with this id is a Vocal tool in that mix. */
  has(mixId: number, processId: string): boolean {
    return (this.#all[this.#key(mixId)] ?? []).includes(processId);
  }

  set(mixId: number, processId: string, on: boolean) {
    const key = this.#key(mixId);
    const cur = this.#all[key] ?? [];
    const next = on ? [...new Set([...cur, processId])] : cur.filter((x) => x !== processId);
    this.#all = { ...this.#all, [key]: next };
    savePref(STORAGE_KEY, this.#all);
  }

  /** The ids marked Vocal in a mix (for the live engine's labels). */
  ids(mixId: number): string[] {
    return this.#all[this.#key(mixId)] ?? [];
  }
}

export const vocalTools = new VocalTools();
