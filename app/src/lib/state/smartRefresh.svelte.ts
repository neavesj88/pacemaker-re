// Keeping the Pacemaker copies of smart cases (the star-prefixed device cases) in step with their
// rules. `check()` runs when the library is opened and after edits (see
// views/cases/Overlays.svelte) and does what the "refresh" setting says: nothing (off), ask with
// the changes named (ask), or write and report (auto). `review()` is the button: it always shows
// what a refresh would do. Every write is `smart_cases_refresh_apply`, which goes through the
// device write gate; the preview and the write take their track lists from one function.
import { smartRefreshApply, smartRefreshPlan, type RefreshReport } from "$lib/api/cases";
import { changed, decideCheck, doneSummary, loopMessage, planSignature, skipped, stuckLines, type RefreshMemory } from "$lib/views/cases/smartRefreshLogic";
import { cases } from "./cases.svelte";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";
import { smartSettings } from "./smartSettings.svelte";
import { toast } from "./toast.svelte";

class SmartRefresh {
  /** The changes the dialog is showing (null: dialog closed). */
  report = $state<RefreshReport | null>(null);
  /** True when the dialog was opened by the button rather than by a check. */
  manual = $state(false);
  checking = $state(false);
  applying = $state(false);
  /** The plan the user answered "Not now" to, one whose write failed, and the plans already
   *  written automatically: not asked / retried / written again until something different would
   *  change (see decideCheck). */
  #mem: RefreshMemory = { dismissed: "", failed: "", written: new Set() };

  get open() {
    return this.report !== null;
  }

  #root(): string | null {
    return session.library?.root ?? null;
  }

  /** At least one smart case has its Pacemaker copy switched on (otherwise there is nothing to do). */
  get relevant() {
    return cases.smart.some((s) => s.on_device);
  }

  /** The automatic check, per the setting. Safe to call often. */
  async check() {
    const root = this.#root();
    if (!root || this.checking || this.applying || this.open || !this.relevant) return;
    await smartSettings.ensure();
    const mode = smartSettings.value.refresh_mode;
    if (mode === "off") return;
    // a job or an unsupported library: the next edit (or the next library load) tries again
    if (!jobs.canWrite) return;
    this.checking = true;
    try {
      const plan = await smartRefreshPlan(root);
      if (this.#root() !== root) return;
      const action = decideCheck(plan, mode, this.#mem);
      const sig = planSignature(plan);
      if (action === "ask") {
        this.manual = false;
        this.report = plan;
      } else if (action === "write") {
        await this.#write(root, sig, true);
      } else if (action === "stuck") {
        // nothing to write, but a smart case cannot be refreshed: say so once
        this.#mem.failed = sig;
        toast.show(doneSummary(plan, session.library?.kind), "bad", 9000);
        session.error = stuckLines(skipped(plan)).join("\n");
      } else if (action === "loop") {
        this.#mem.failed = sig;
        session.error = loopMessage(plan, session.library?.kind);
      }
    } catch (e) {
      session.error = String(e);
    } finally {
      this.checking = false;
    }
  }

  /** The "Refresh smart cases" button: show what would change, or say nothing does. */
  async review() {
    const root = this.#root();
    if (!root || this.checking || this.applying || !jobs.guard()) return;
    this.checking = true;
    try {
      const plan = await smartRefreshPlan(root);
      if (this.#root() !== root) return;
      if (plan.items.length === 0) toast.show(session.isComputer ? "No smart case has a ★ case in this library" : "No smart case is on the Pacemaker", "ok");
      else if (plan.changes === 0 && skipped(plan).length === 0) toast.show(doneSummary(plan, session.library?.kind), "ok");
      else {
        this.manual = true;
        this.report = plan;
      }
    } catch (e) {
      session.error = String(e);
    } finally {
      this.checking = false;
    }
  }

  /** "Refresh" in the dialog. */
  async confirm() {
    const root = this.#root();
    if (!root || !this.report || this.applying) return;
    const sig = planSignature(this.report);
    this.report = null;
    await this.#write(root, sig, false);
  }

  /** "Not now": remembered for this exact set of changes. */
  dismiss() {
    if (this.report && !this.manual) this.#mem.dismissed = planSignature(this.report);
    this.report = null;
  }

  async #write(root: string, sig: string, automatic: boolean) {
    if (!jobs.guard()) return;
    this.applying = true;
    if (automatic) this.#mem.written.add(sig);
    try {
      const done = await smartRefreshApply(root);
      if (this.#root() !== root) return;
      const wrote = changed(done);
      const stuck = skipped(done);
      this.#mem.failed = stuck.length > 0 && wrote.length === 0 ? sig : "";
      await cases.loadSmart();
      await cases.refresh();
      toast.show(doneSummary(done, session.library?.kind), stuck.length ? "bad" : "ok", 9000);
      if (stuck.length) session.error = stuckLines(stuck).join("\n");
    } catch (e) {
      this.#mem.failed = sig;
      session.error = String(e);
    } finally {
      this.applying = false;
    }
  }

  reset() {
    this.report = null;
    this.manual = false;
    this.#mem = { dismissed: "", failed: "", written: new Set() };
  }
}

export const smartRefresh = new SmartRefresh();
session.onReset(() => smartRefresh.reset());
