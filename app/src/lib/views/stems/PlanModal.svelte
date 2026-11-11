<script lang="ts">
  import { stems } from "$lib/state/stems.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { actionVerb, canStart, fileName, noun, planHeadline } from "./logic";

  // The confirmation before a run: every track and what happens to it, in words, and every
  // existing file that would be replaced. It shows the plan the backend computed with the same
  // function the run uses; the run computes it again and refuses if replacing is not ticked.
  let { onclose }: { onclose: () => void } = $props();

  const plan = $derived(stems.plan!);
  let tick = $state(false);
  const ok = $derived(canStart(plan, tick));

  async function go() {
    const confirmed = tick;
    onclose();
    await stems.start(confirmed);
  }
</script>

<Modal title="Make stems: review" {onclose}>
  <p class="head">{planHeadline(plan)}.</p>
  <p class="muted small">
    Files go to <code>{plan.dest}</code> as {plan.format_label}. Nothing is written to the Pacemaker or changed in your original files.
  </p>

  <ul class="items">
    {#each plan.items as i (i.track_id)}
      <li class={i.action}>
        <span class="verb">{actionVerb(i.action)}</span>
        <span class="body">
          <strong>{i.label}</strong>
          <span class="what">{i.what}</span>
          {#if i.action === "make" || i.action === "replace" || i.action === "partial"}
            <span class="files muted">
              {#if i.write_vocals}{fileName(i.vocals_path)}{/if}{#if i.write_vocals && i.write_instrumental}<br />{/if}{#if i.write_instrumental}{fileName(i.instrumental_path)}{/if}
            </span>
          {/if}
        </span>
      </li>
    {/each}
  </ul>

  {#if plan.replace > 0}
    <div class="replace" role="alert">
      <strong>These {noun(plan.replaced_files.length, "existing file")} will be overwritten:</strong>
      <ul>
        {#each plan.replaced_files as f (f)}<li><code>{f}</code></li>{/each}
      </ul>
      <label class="tick"><input type="checkbox" bind:checked={tick} /> Replace {plan.replaced_files.length === 1 ? "this file" : `all ${plan.replaced_files.length} files`}.</label>
    </div>
  {/if}

  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!ok} onclick={go}>Make stems</Button>
  {/snippet}
</Modal>

<style>
  .head {
    margin: 0 0 4px;
  }
  .small {
    margin: 0 0 8px;
    font-size: 12px;
  }
  .items {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 300px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--deep);
  }
  .items > li {
    display: flex;
    gap: 10px;
    padding: 5px 10px;
    border-bottom: 1px solid var(--line);
    content-visibility: auto;
    contain-intrinsic-size: auto 52px;
  }
  .items > li:last-child {
    border-bottom: none;
  }
  .verb {
    flex: none;
    width: 74px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding-top: 2px;
    color: var(--green);
  }
  li.replace .verb {
    color: var(--accent);
  }
  li.skip .verb {
    color: var(--muted);
  }
  li.refuse .verb {
    color: var(--bad);
  }
  .body {
    display: flex;
    flex-direction: column;
    min-width: 0;
    font-size: 12px;
  }
  .body strong {
    overflow-wrap: anywhere;
  }
  .files {
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .replace {
    margin-top: 10px;
    padding: 8px 10px;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: var(--accent-soft);
    font-size: 12px;
  }
  .replace ul {
    margin: 4px 0;
    padding-left: 18px;
    max-height: 120px;
    overflow: auto;
  }
  .tick {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 4px;
  }
</style>
