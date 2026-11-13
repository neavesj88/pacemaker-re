<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { caseTracks } from "$lib/api/cases";
  import { type VdjPlan, pickExportFolder, vdjExportCancel, vdjExportPlan, vdjExportStart } from "$lib/api/vdj";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import VdjOptionsFields from "./VdjOptionsFields.svelte";
  import { vdj } from "./vdj.svelte";
  import { buildRequest, entryWords, formatBytes, planLine, selectedInCase, startGate, startLabel, suggestListName } from "./vdjLogic";

  // Exports one case (or the tracks selected in it) as a VirtualDJ list, optionally with copies of
  // the audio, into a folder the user picks. It shows, before anything is written, what happens to
  // every track; the numbers and names come from the same backend function the export runs.
  // Nothing here writes to the Pacemaker.
  let { caseId, caseName, onclose }: { caseId: number; caseName: string; onclose: () => void } = $props();

  const root = untrack(() => session.library?.root ?? "");

  let caseIds = $state<number[]>([]);
  let loadFailed = $state("");
  // the selection as it was when the dialog opened: later clicks in the lists behind do not change it
  const selectedAtOpen = untrack(() => [...selection.trackIds]);
  const selected = $derived(selectedInCase(selectedAtOpen, caseIds));

  let listName = $state(untrack(() => suggestListName(caseName)));
  let scope = $state<"all" | "selected">("all");
  let replace = $state(false);

  let plan = $state<VdjPlan | null>(null);
  let planError = $state("");
  let planning = $state(false);
  let startError = $state("");

  onMount(() => {
    void (async () => {
      try {
        if (!root) throw "No library is open.";
        caseIds = await caseTracks(root, caseId);
        // start from the selection when some of the case's tracks are selected
        if (selectedInCase(selectedAtOpen, caseIds).length > 0) scope = "selected";
      } catch (e) {
        loadFailed = String(e);
      }
    })();
  });

  const onlyIds = $derived(scope === "selected" ? selected : []);
  const request = $derived(buildRequest({ root, caseId, onlyTrackIds: onlyIds, listName, destDir: vdj.dest, options: vdj.options }));

  // ---- the preview -----------------------------------------------------------------------
  // Re-planned a moment after any change; an answer that arrives after a newer request is dropped.
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const req = request;
    vdj.finished; // re-plan when an export ends: the list and the copies exist now
    const ready = caseIds.length > 0 && req.dest_dir !== "" && !loadFailed;
    untrack(() => {
      clearTimeout(timer);
      startError = "";
      if (!ready) {
        plan = null;
        planError = "";
        planning = false;
        return;
      }
      planning = true;
      const mine = ++seq;
      timer = setTimeout(async () => {
        try {
          const p = await vdjExportPlan(req);
          if (mine !== seq) return;
          plan = p;
          planError = "";
          if (!p.list_exists) replace = false;
        } catch (e) {
          if (mine !== seq) return;
          plan = null;
          planError = String(e);
        } finally {
          if (mine === seq) planning = false;
        }
      }, 250);
    });
    return () => clearTimeout(timer);
  });

  // the go-ahead to replace the list was for one export: the next one asks again
  let seenFinished = untrack(() => vdj.finished);
  $effect(() => {
    const f = vdj.finished;
    untrack(() => {
      if (f !== seenFinished) {
        seenFinished = f;
        replace = false;
      }
    });
  });

  // remember the options and the folder
  $effect(() => {
    JSON.stringify($state.snapshot(vdj.options));
    vdj.dest;
    vdj.persist();
  });

  const gate = $derived(startGate({ plan, planError, planning, replace, running: vdj.running, folder: vdj.dest }));

  async function choose() {
    try {
      const p = await pickExportFolder();
      if (p) vdj.dest = p;
    } catch (e) {
      startError = `The folder picker failed: ${String(e)}`;
    }
  }

  async function start() {
    if (!gate.ok) return;
    startError = "";
    const agreed = replace;
    vdj.begin();
    try {
      await vdjExportStart(request, agreed);
    } catch (e) {
      vdj.running = false;
      startError = String(e);
    }
  }

  function stop() {
    void vdjExportCancel().catch((e) => (startError = String(e)));
  }

  const folderHint = $derived(plan?.folder ?? "");
</script>

<!-- While the export runs, Escape and a click outside do nothing: the Close button stays, and the footer says the export carries on. -->
<svelte:window onkeydown={(e) => e.key === "Escape" && !vdj.running && onclose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={() => !vdj.running && onclose()}>
  <div class="dlg" role="dialog" aria-modal="true" aria-label="Export to VirtualDJ" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <header>
      <h2>Export to VirtualDJ</h2>
      <span class="muted">case · {caseName}</span>
      <span class="grow"></span>
      <Button small onclick={onclose}>Close</Button>
    </header>

    <div class="body">
      {#if loadFailed}
        <p class="bad empty">{loadFailed}</p>
      {:else}
        <p class="muted intro">
          Writes a VirtualDJ list (VirtualDJ 2024 or later) of this case into a folder you choose, and can copy the audio next to it. Nothing on
          the Pacemaker is changed or deleted. Afterwards put the <code>.xml</code> file into VirtualDJ's <i>My Lists</i> folder.
        </p>

        <fieldset disabled={vdj.running}>
          <div class="row">
            <label for="vdj-name">List name</label>
            <input id="vdj-name" bind:value={listName} maxlength="120" placeholder="Name of the list, its folder and its file" />
          </div>
          <div class="row">
            <label for="vdj-dest">Folder</label>
            <input id="vdj-dest" bind:value={vdj.dest} placeholder="Where to export, such as E:\ (a USB stick)" spellcheck="false" />
            <Button onclick={choose}>Choose…</Button>
          </div>
          {#if folderHint}
            <p class="muted small indent">Everything goes into <code>{folderHint}</code></p>
          {/if}

          {#if selected.length > 0}
            <div class="row" role="radiogroup" aria-label="Which tracks">
              <span class="lbl">Tracks</span>
              <label class="inline"><input type="radio" bind:group={scope} value="all" /> The whole case ({caseIds.length})</label>
              <label class="inline"><input type="radio" bind:group={scope} value="selected" /> Only the {selected.length} selected</label>
            </div>
          {/if}

          <VdjOptionsFields {folderHint} {listName} />
        </fieldset>

        <section class="plan" aria-live="polite">
          {#if !vdj.dest.trim()}
            <p class="muted">Choose a folder to see what will be exported.</p>
          {:else if planError}
            <p class="bad" role="alert">{planError}</p>
          {:else if plan}
            <p class="sum">{planLine(plan)}</p>
            {#each plan.notes as n (n)}<p class="muted small">{n}</p>{/each}
            <ol class="tracks">
              {#each plan.entries as e (e.track_id)}
                <li class={e.status}>
                  <span class="t">{e.label}</span>
                  <span class="w">{entryWords(e)}</span>
                </li>
              {/each}
            </ol>
            <p class="muted small">The list file: <code>{plan.list_file}</code></p>
          {:else if planning}
            <p class="muted">Working out what will be exported…</p>
          {/if}
        </section>
      {/if}
    </div>

    {#if plan?.list_exists}
      <div class="replace">
        <label class="inline">
          <input type="checkbox" bind:checked={replace} disabled={vdj.running} />
          <span>Replace the list file that is already there: <code>{plan.list_file}</code>. The audio files in the folder are not replaced.</span>
        </label>
      </div>
    {/if}

    <footer>
      {#if vdj.running}
        <div class="prog" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(vdj.pct * 100)}>
          <div class="fill" style:width="{Math.round(vdj.pct * 100)}%"></div>
        </div>
        <span class="small cur">{vdj.current || "Starting…"}</span>
        <span class="small muted note">Closing this window does not stop the export.</span>
        <Button onclick={stop}>Stop</Button>
      {:else}
        <span class="msg small" class:bad={!!startError || vdj.result?.bad}>
          {startError || (vdj.result ? vdj.result.text : gate.ok ? (plan && plan.copy_audio && plan.copy_bytes > 0 ? `${formatBytes(plan.copy_bytes)} will be written to the folder.` : "") : planError ? "" : gate.why)}
        </span>
        <Button variant="primary" onclick={start} disabled={!gate.ok}>{startLabel(plan)}</Button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--scrim);
  }
  .dlg {
    display: flex;
    flex-direction: column;
    width: min(820px, 96vw);
    max-height: min(860px, 94vh);
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .grow {
    flex: 1;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 6px 14px 12px;
  }
  .intro {
    margin: 8px 0;
  }
  fieldset {
    border: 0;
    margin: 0;
    padding: 0;
    min-width: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    flex-wrap: wrap;
  }
  .row > label:first-child,
  .lbl {
    width: 150px;
    flex: none;
    color: var(--muted);
  }
  .row input:not([type]) {
    flex: 1;
    min-width: 200px;
  }
  .inline {
    display: inline-flex;
    align-items: flex-start;
    gap: 6px;
  }
  .inline input {
    margin-top: 3px;
  }
  .small {
    font-size: 12px;
  }
  .indent {
    margin: 0 0 0 22px;
  }
  p {
    margin: 6px 0;
  }
  .plan {
    margin-top: 12px;
    border-top: 1px solid var(--line);
    padding-top: 8px;
  }
  .sum {
    font-weight: 600;
  }
  .tracks {
    list-style: none;
    margin: 6px 0;
    padding: 0;
    max-height: 260px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .tracks li {
    display: grid;
    grid-template-columns: minmax(120px, 2fr) 3fr;
    gap: 10px;
    padding: 4px 10px;
    font-size: 12.5px;
  }
  .tracks li + li {
    border-top: 1px solid var(--line);
  }
  .t {
    overflow-wrap: anywhere;
  }
  .w {
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  li.left_out .w {
    color: var(--bad-text);
  }
  li.left_out {
    background: var(--bad-bg);
  }
  .replace {
    padding: 8px 14px;
    border-top: 1px solid var(--line);
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  .bad {
    color: var(--bad-text);
  }
  .empty {
    padding: 20px 4px;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    border-top: 1px solid var(--line);
  }
  .msg {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    color: var(--muted);
  }
  .msg.bad {
    color: var(--bad-text);
  }
  .prog {
    flex: 1;
    height: 8px;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent-ramp);
  }
  .note {
    max-width: 200px;
  }
  .cur {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
</style>
