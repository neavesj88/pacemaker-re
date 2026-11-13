<script lang="ts">
  import { onMount } from "svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { plural, tickKey } from "./cleanupLogic";
  import { cleanup } from "./cleanupState.svelte";
  import DiffText from "./DiffText.svelte";
  import TagWriteFields from "./TagWriteFields.svelte";
  import { STATUS_WORDS, fieldLabel, tagWrite } from "./tagwriteState.svelte";

  // Writes the library's tags into the audio files themselves, for other software to read. It
  // reads the files first and lists every file and field it would change; only ticked lines are
  // written, and only after a confirmation that names every file. Off unless the user comes here.
  const PAGE = 300;
  let shown = $state(PAGE);
  let showFormats = $state(false);

  onMount(() => void tagWrite.loadFormats());

  let s = $derived(cleanup.settings);
  const plans = $derived(tagWrite.plans);
  const toWrite = $derived((plans ?? []).filter((p) => p.status === "write"));
  const others = $derived((plans ?? []).filter((p) => p.status !== "write"));
  const nItems = $derived(tagWrite.items.length);
  const nFields = $derived(tagWrite.items.reduce((n, it) => n + Object.keys(it).length - 1, 0));
  const isOn = (id: number, f: string) => !tagWrite.unticked[tickKey(id, f)];

  function check() {
    shown = PAGE;
    void tagWrite.checkTracks(cleanup.ids, s?.write_fields ?? []);
  }
</script>

<div class="intro">
  <p class="muted">
    The Pacemaker reads the tags in its library, not in the audio files. This writes the library’s values into each file’s own
    tags so other software (a tag editor, a media player, another DJ program) sees the same names. It changes the files in the
    library, so it asks first and lists every file. A blank value in the library never erases a tag in the file.
  </p>
  <p class="muted">
    Only the fields you tick are replaced. Everything else in the file’s tag stays as it is: ratings and play counts, cover art,
    iTunes Sound Check and gapless data, private and custom frames, other credits. A comment is written to the plain comment only.
    An ID3v2.3 tag stays 2.3 (an old ID3v2.2 tag is upgraded to 2.4, and the list says so). After every write the copy is checked
    (same audio bytes, same tag version, untouched items still there) before it replaces the file.
  </p>
  <p class="muted">
    <Button variant="link" onclick={() => (showFormats = !showFormats)}>{showFormats ? "Hide" : "Show"} the file types it can write</Button>
    {#if showFormats}
      {#if tagWrite.formats.length}
        <span class="formats">
          {#each tagWrite.formats as f (f.ext)}<span class="chip">.{f.ext}: {f.container}, {f.tag}</span>{/each}
        </span>
      {:else}
        <span>Loading…</span>
      {/if}
      Files of any other type are listed as not supported and left alone.
    {/if}
  </p>
</div>

<TagWriteFields />

<div class="bar">
  <Button onclick={check} disabled={tagWrite.checking || cleanup.ids.length === 0 || !s || s.write_fields.length === 0}>
    {tagWrite.checking ? "Reading the files…" : `Check ${plural(cleanup.ids.length, "track")}`}
  </Button>
  <span class="muted">Reads the files; changes nothing.</span>
</div>

{#if tagWrite.error}<p class="err" role="alert">{tagWrite.error}</p>{/if}

{#if plans}
  <div class="bar" role="status">
    <span>
      {toWrite.length} to write, {tagWrite.count("same")} already match{#if tagWrite.count("unsupported")}, {tagWrite.count("unsupported")} not supported{/if}{#if tagWrite.count("missing")}, {tagWrite.count("missing")} missing{/if}{#if tagWrite.count("error")}, {tagWrite.count("error")} cannot be written{/if}.
    </span>
    <span class="sp"></span>
    <Button
      variant="primary"
      disabled={nItems === 0 || jobs.busy}
      title={jobs.busy ? "Another job is still running" : undefined}
      onclick={() => tagWrite.ask()}
    >
      Write {plural(nItems, "file")} ({plural(nFields, "field")})…
    </Button>
  </div>

  {#if toWrite.length}
    <div class="scroll">
      <table>
        <thead><tr><th class="tick"><span class="sr">Write</span></th><th>Track and file</th><th>Field</th><th>In the file now</th><th>Will be</th></tr></thead>
        <tbody>
          {#each toWrite.slice(0, shown) as p (p.track_id)}
            {#each p.changes as c, i (c.field)}
              <tr class:off={!isOn(p.track_id, c.field)} class:first={i === 0}>
                <td class="tick">
                  <input
                    type="checkbox"
                    checked={isOn(p.track_id, c.field)}
                    aria-label={`Write the ${fieldLabel(c.field)} of ${p.label} into ${p.file}`}
                    onchange={(e) => tagWrite.setTicked(p.track_id, c.field, e.currentTarget.checked)}
                  />
                </td>
                <td class="trk">
                  {#if i === 0}
                    {p.label}
                    <div class="file">{p.file} · {p.format}</div>
                    {#each p.notes as n (n)}
                      <div class="file">Note: {n}</div>
                    {/each}
                    {#if p.unsupported_fields.length}
                      <div class="file">This file type has no place for: {p.unsupported_fields.map(fieldLabel).join(", ")}</div>
                    {/if}
                  {/if}
                </td>
                <td class="fld">{fieldLabel(c.field)}</td>
                <td><DiffText old={c.old} new={c.new} side="old" /></td>
                <td><DiffText old={c.old} new={c.new} side="new" /></td>
              </tr>
            {/each}
          {/each}
        </tbody>
      </table>
      {#if toWrite.length > shown}
        <div class="more">
          Showing {shown} of {toWrite.length} files.
          <Button small onclick={() => (shown += PAGE)}>Show more</Button>
          <Button small onclick={() => (shown = toWrite.length)}>Show all</Button>
          <span class="muted">Ticks apply to every file, shown or not.</span>
        </div>
      {/if}
    </div>
  {/if}

  {#if others.length}
    <details class="others">
      <summary>{plural(others.length, "track")} with nothing to write</summary>
      <ul>
        {#each others as p (p.track_id)}
          <li><strong>{p.label}</strong>{p.file ? ` (${p.file})` : ""}: {STATUS_WORDS[p.status] ?? p.status}{p.message ? `, ${p.message}` : ""}</li>
        {/each}
      </ul>
    </details>
  {/if}
{/if}

<style>
  .intro p {
    margin: 0 0 6px;
    font-size: 12px;
    max-width: 90ch;
  }
  .formats {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0 6px;
  }
  .chip {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--green) 10%, transparent);
    color: var(--green);
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
  }
  .sp {
    flex: 1;
  }
  .err {
    color: var(--bad-text);
    background: var(--bad-bg);
    padding: 6px 10px;
    border-radius: 4px;
  }
  .scroll {
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: auto;
    max-height: calc(100vh - 460px);
    min-height: 160px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--panel);
    text-align: left;
    font-weight: 600;
    font-size: 12px;
    color: var(--muted);
    padding: 5px 8px;
    border-bottom: 1px solid var(--line);
  }
  th.tick,
  td.tick {
    width: 34px;
    text-align: center;
  }
  th:nth-child(2) {
    width: 30%;
  }
  th:nth-child(3) {
    width: 100px;
  }
  td {
    padding: 4px 8px;
    vertical-align: top;
  }
  tr.first td {
    border-top: 1px solid var(--line);
  }
  tr.off td:not(.tick) {
    opacity: 0.45;
  }
  .trk {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .file {
    font-weight: normal;
    font-size: 11px;
    color: var(--muted);
  }
  .fld {
    color: var(--muted);
  }
  .more {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 8px;
  }
  .others {
    margin-top: 10px;
    font-size: 12px;
  }
  .others ul {
    margin: 6px 0 0;
    padding-left: 18px;
    max-height: 220px;
    overflow: auto;
  }
  input[type="checkbox"] {
    padding: 0;
    accent-color: var(--accent);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
