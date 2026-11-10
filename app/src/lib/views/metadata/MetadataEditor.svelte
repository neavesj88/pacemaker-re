<script lang="ts">
  // The track metadata dialog. One id: every editable tag of that track, with Previous/Next to
  // move through the library (edits to each visited track are kept and saved together). Several
  // ids ("Multiple Item Information"): a field is written to every track only if it was touched;
  // fields where the tracks differ show "Multiple values". Enter saves, Escape cancels.
  import { untrack } from "svelte";
  import { saveMetadata, trackDetails, NUMBER_FIELDS, TEXT_FIELDS, type TrackDetail } from "$lib/api/metadata";
  import { session } from "$lib/state/session.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { metadataEditor } from "$lib/state/metadata.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import {
    FIELD_LABELS,
    commonDraft,
    diffSingle,
    draftErrors,
    editFromTouched,
    fieldError,
    isEmptyEdit,
    toDraft,
    type Draft,
    type FieldKey,
  } from "./draft";

  let { ids }: { ids: number[] } = $props();

  // the editor is re-created (keyed) when it is opened for other tracks, so the ids are fixed
  const startIds = untrack(() => [...ids]);
  const multi = startIds.length > 1;

  const root = $derived(session.library?.root ?? "");
  // Previous/Next walk the library in its own order
  const order = $derived((session.library?.tracks ?? []).map((t) => t.track_id));

  let details = $state<Record<number, TrackDetail>>({});
  let drafts = $state<Record<number, Draft>>({});
  let current = $state(startIds[0]);
  // multi mode
  let multiDraft = $state<Draft | null>(null);
  let mixed = $state<Record<string, boolean>>({});
  let touched = $state<Record<string, boolean>>({});

  let loading = $state(true);
  let busy = $state(false);
  let error = $state("");
  let dialog = $state<HTMLDivElement | null>(null);

  const draft = $derived(multi ? multiDraft : (drafts[current] ?? null));
  const detail = $derived(details[current]);
  const position = $derived(order.indexOf(current));

  async function load() {
    try {
      const rows = await trackDetails(root, startIds);
      for (const d of rows) {
        details[d.track_id] = d;
      }
      if (multi) {
        const { draft: d, mixed: m } = commonDraft(rows);
        multiDraft = d;
        mixed = Object.fromEntries([...m].map((k) => [k, true]));
      } else {
        drafts[rows[0].track_id] = toDraft(rows[0]);
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
  void load();

  $effect(() => {
    if (!loading) dialog?.querySelector<HTMLElement>("input")?.focus();
  });

  async function step(delta: number) {
    const next = order[position + delta];
    if (next === undefined || busy) return;
    if (!drafts[next]) {
      try {
        const [d] = await trackDetails(root, [next]);
        details[next] = d;
        drafts[next] = toDraft(d);
      } catch (e) {
        error = String(e);
        return;
      }
    }
    error = "";
    current = next;
  }

  const val = (k: FieldKey): string => draft?.[k] ?? "";
  function setVal(k: FieldKey, v: string) {
    if (!draft) return;
    draft[k] = v;
    if (multi) touched[k] = true;
  }
  function setRating(n: number) {
    if (!draft) return;
    draft.rating = n;
    if (multi) touched.rating = true;
  }

  // what would be written, track by track; empty edits are left out
  const edits = $derived.by(() => {
    if (multi) {
      if (!multiDraft) return [];
      const e = editFromTouched(multiDraft, Object.keys(touched).filter((k) => touched[k]) as (FieldKey | "rating")[]);
      return isEmptyEdit(e) ? [] : startIds.map((id) => ({ track_id: id, ...e }));
    }
    return Object.entries(drafts)
      .map(([id, d]) => ({ track_id: Number(id), ...diffSingle(details[Number(id)], d) }))
      .filter((e) => Object.keys(e).length > 1);
  });

  const problems = $derived.by(() => {
    if (multi) {
      return multiDraft ? draftErrors(multiDraft, Object.keys(touched).filter((k) => touched[k] && k !== "rating") as FieldKey[]) : [];
    }
    return Object.entries(drafts).flatMap(([id, d]) => {
      const msgs = draftErrors(d);
      const label = details[Number(id)]?.title || `Track ${id}`;
      return msgs.map((m) => (Object.keys(drafts).length > 1 ? `${label}: ${m}` : m));
    });
  });

  const canSave = $derived(!loading && !busy && problems.length === 0 && edits.length > 0);

  async function save(then?: () => void) {
    if (busy || loading) return;
    if (problems.length) return;
    if (edits.length === 0) {
      metadataEditor.close();
      then?.();
      return;
    }
    busy = true;
    error = "";
    try {
      const n = await saveMetadata(root, edits);
      await session.load(root, true);
      toast.show(n === 1 ? "Saved 1 track" : `Saved ${n} tracks`, "ok");
      metadataEditor.close();
      then?.();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function openGrid() {
    const id = current;
    void save(() => nav.go("grid", { "grid.trackId": id }));
  }

  function onkey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      metadataEditor.close();
    } else if (e.key === "Enter" && !e.isComposing) {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "BUTTON" || t.tagName === "A")) return; // Enter activates the focused button
      e.preventDefault();
      void save();
    }
  }

  const fmtBpm = (b: number) => (b > 0 ? b.toFixed(1) : "not analysed");
</script>

<svelte:window onkeydown={onkey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={() => metadataEditor.close()}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={multi ? "Multiple Item Information" : "Track information"}
    tabindex="-1"
    bind:this={dialog}
    onclick={(e) => e.stopPropagation()}
  >
    <h2>{multi ? "Multiple Item Information" : "Track information"}</h2>
    {#if multi}
      <p class="sub">
        {startIds.length} tracks. Only the fields you change are written; the rest of each track stays as it is.
      </p>
    {:else if detail}
      <p class="sub">
        {#if position >= 0}Track {position + 1} of {order.length}{/if}
      </p>
    {/if}

    {#if loading}
      <p class="muted">Loading…</p>
    {:else if draft}
      <div class="grid">
        {#each [...TEXT_FIELDS.filter((k) => k !== "comments"), ...NUMBER_FIELDS] as k (k)}
          <label class="f">
            <span>{FIELD_LABELS[k]}</span>
            <input
              class:bad={fieldError(k, val(k)) !== ""}
              type="text"
              inputmode={NUMBER_FIELDS.includes(k as never) ? "numeric" : "text"}
              autocomplete="off"
              spellcheck="false"
              value={val(k)}
              placeholder={multi && mixed[k] && !touched[k] ? "Multiple values" : ""}
              oninput={(e) => setVal(k, e.currentTarget.value)}
            />
          </label>
        {/each}
        <label class="f wide">
          <span>{FIELD_LABELS.comments}</span>
          <input
            class:bad={fieldError("comments", val("comments")) !== ""}
            type="text"
            autocomplete="off"
            value={val("comments")}
            placeholder={multi && mixed.comments && !touched.comments ? "Multiple values" : ""}
            oninput={(e) => setVal("comments", e.currentTarget.value)}
          />
        </label>
      </div>

      <div class="row">
        <span class="lbl">Rating</span>
        <Stars rating={draft.rating} onchange={setRating} />
        {#if multi && mixed.rating && !touched.rating}<span class="muted">Multiple values</span>{/if}
        {#if !multi && detail}
          <span class="ro">
            <span class="muted">BPM</span> <span class="num">{fmtBpm(detail.bpm)}</span>
            <span class="muted">Key</span> <span class="num">{detail.key || "none"}</span>
            <Button variant="link" onclick={openGrid} title="BPM and key come from analysis; change them in the beat grid editor">
              Edit in grid editor
            </Button>
          </span>
        {/if}
      </div>
    {/if}

    {#if problems.length}
      <p class="err" role="alert">{problems.slice(0, 3).join(". ")}{problems.length > 3 ? ` (and ${problems.length - 3} more)` : ""}</p>
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}

    <div class="actions">
      {#if !multi}
        <Button onclick={() => step(-1)} disabled={busy || loading || position <= 0}>Previous</Button>
        <Button onclick={() => step(1)} disabled={busy || loading || position < 0 || position >= order.length - 1}>Next</Button>
      {/if}
      <span class="spacer"></span>
      <Button onclick={() => metadataEditor.close()} disabled={busy}>Cancel</Button>
      <Button variant="primary" onclick={() => save()} disabled={!canSave}>
        {busy ? "Saving…" : edits.length > 1 && !multi ? `Save ${edits.length} tracks` : "Save"}
      </Button>
    </div>
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
    background: rgba(0, 0, 0, 0.6);
  }
  .dialog {
    width: 640px;
    max-width: 94vw;
    max-height: 92vh;
    overflow: auto;
    padding: 18px 20px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 16px;
  }
  .sub {
    margin: 0 0 12px;
    color: var(--muted);
    min-height: 1.2em;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 14px;
  }
  @media (max-width: 520px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
  .f {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .f.wide {
    grid-column: 1 / -1;
  }
  .f span,
  .lbl {
    font-size: 12px;
    color: var(--muted);
  }
  input {
    width: 100%;
    box-sizing: border-box;
    padding: 5px 8px;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    font: inherit;
  }
  input:focus {
    outline: none;
    border-color: var(--accent);
  }
  input::placeholder {
    color: var(--muted);
    font-style: italic;
  }
  input.bad {
    border-color: var(--bad);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-top: 14px;
  }
  .ro {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  .err {
    margin: 12px 0 0;
    padding: 6px 10px;
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 16px;
  }
  .spacer {
    flex: 1;
  }
</style>
