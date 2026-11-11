// The Vocal filter flag of the open library's card, shared by the Device view (which sets it) and the
// mix editor (which names the Wah tool after it). The backend renders previews and exports with the
// same flag, so what the editor shows is what the mix will sound like.
import { deviceVocal, deviceVocalSet, type VocalMode, type VocalState } from "$lib/api/device";
import { session } from "./session.svelte";

class Vocal {
  /** null until read (or when it could not be read). */
  state = $state<VocalState | null>(null);
  /** Why the last read failed ("" when it did not). */
  error = $state("");
  #root = "";
  #seq = 0;

  /** What the device will play and the mixes render: "off" until read, or under the kill switch. */
  get mode(): VocalMode {
    return this.state?.effective ?? "off";
  }

  /** Reads the flag from `root`. A newer call wins, so a slow read of a drive that was swapped out
   *  never lands on the new one. */
  async load(root: string) {
    if (root !== this.#root) this.state = null;
    this.#root = root;
    const seq = ++this.#seq;
    try {
      const s = await deviceVocal(root);
      if (seq === this.#seq) {
        this.state = s;
        this.error = "";
      }
    } catch (e) {
      if (seq === this.#seq) {
        this.state = null;
        this.error = String(e);
      }
    }
  }

  /** Writes the flag to the card; throws the backend's message when it is refused. */
  async set(root: string, mode: VocalMode) {
    const seq = ++this.#seq;
    const s = await deviceVocalSet(root, mode);
    if (root === this.#root && seq === this.#seq) this.state = s;
    else if (root === this.#root) await this.load(root);
  }

  clear() {
    this.#seq++;
    this.#root = "";
    this.state = null;
    this.error = "";
  }
}

export const vocal = new Vocal();
session.onReset(() => vocal.clear());
