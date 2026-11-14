// Which library read is allowed to take effect. Reads overlap (a job's reload of the open library
// while the user picks another library from a slow Pacemaker); the last one the user asked for wins
// and a read that was overtaken is dropped. Pure, so it runs under plain node (loadSeq.test.mjs).

export class LoadSeq {
  #seq = 0;
  #switching = false;

  /** A library switch the user asked for is being read. */
  get switching() {
    return this.#switching;
  }

  /** Starts a read. `inPlace` is a reload of the open library (after a job): it is refused (null)
   *  while a switch is in flight, since it would only read the library being left. A switch always
   *  starts and makes every earlier read stale. */
  begin(inPlace: boolean): number | null {
    if (inPlace && this.#switching) return null;
    const t = ++this.#seq;
    if (!inPlace) this.#switching = true;
    return t;
  }

  /** The read `t` is still the newest, so its result may be used. */
  current(t: number): boolean {
    return t === this.#seq;
  }

  /** The read `t` ended. True when it was the newest (the caller then clears its "reading" state). */
  end(t: number): boolean {
    if (t !== this.#seq) return false;
    this.#switching = false;
    return true;
  }
}

/** The root to re-read in place when a library switch ends, or null. A reload refused while the switch was
 *  being read (`pending`) is only still owed when the switch did not open a library and what stays open
 *  (`attachedRoot`: null when nothing is open or the open one was ejected) is still there to read. */
export function reloadAfter(pending: boolean, opened: boolean, keepView: boolean, attachedRoot: string | null): string | null {
  return pending && !opened && !keepView ? attachedRoot : null;
}
