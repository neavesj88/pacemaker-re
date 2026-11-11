// Keeping the Pacemaker copies of smart cases (the star-prefixed device cases) in step with their
// rules. `check()` runs when the library is opened and after edits (see
// views/cases/Overlays.svelte) and does what the "refresh" setting says: nothing (off), ask with
// the changes named (ask), or write and report (auto). `review()` is the button: it always shows
// what a refresh would do. Every write is `smart_cases_refresh_apply`, which goes through the
// device write gate; the preview and the write take their track lists from one function.
import { smartRefreshApply, smartRefreshPlan, type RefreshReport } from "$lib/api/cases";
import { changed, doneSummary, needsAttention, planSignature, skipped } from "$lib/views/cases/smartRefreshLogic";
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
  /** The plan the user answered "Not now" to, and one whose write failed: not asked / retried
   *  until something different would change. */
  #dismissed = "";
  #failed = "";

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
      if (!needsAttention(plan)) {
        this.#dismissed = "";
        this.#failed = "";
        return;
      }
      const sig = planSignature(plan);
      if (sig === this.#dismissed || sig === this.#failed) return;
      if (mode === "ask") {
        this.manual = false;
        this.report = plan;
      } else {
        await this.#write(root, sig);
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
      if (plan.items.length === 0) toast.show("No smart case is on the Pacemaker", "ok");
      else if (plan.changes === 0 && skipped(plan).length === 0) toast.show(doneSummary(plan), "ok");
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
    await this.#write(root, sig);
  }

  /** "Not now": remembered for this exact set of changes. */
  dismiss() {
    if (this.report && !this.manual) this.#dismissed = planSignature(this.report);
    this.report = null;
  }

  async #write(root: string, sig: string) {
    if (!jobs.guard()) return;
    this.applying = true;
    try {
      const done = await smartRefreshApply(root);
      if (this.#root() !== root) return;
      const wrote = changed(done);
      const stuck = skipped(done);
      this.#failed = stuck.length > 0 && wrote.length === 0 ? sig : "";
      await cases.loadSmart();
      await cases.refresh();
      toast.show(doneSummary(done), stuck.length ? "bad" : "ok", 9000);
      if (stuck.length) session.error = stuck.map((i) => `“${i.name}” was not refreshed: ${i.error ?? "unknown reason"}`).join("\n");
    } catch (e) {
      this.#failed = sig;
      session.error = String(e);
    } finally {
      this.applying = false;
    }
  }

  reset() {
    this.report = null;
    this.manual = false;
    this.#dismissed = "";
    this.#failed = "";
  }
}

export const smartRefresh = new SmartRefresh();
session.onReset(() => smartRefresh.reset());
