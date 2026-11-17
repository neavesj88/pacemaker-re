<script lang="ts">
  import type { VocalMode } from "$lib/api/device";
  import Button from "$lib/ui/Button.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { vocal } from "$lib/state/vocal.svelte";
  import HelpLink from "../help/HelpLink.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";

  // The Vocal filter replaces the Wah effect of Revival firmware 180096073 and later. It is
  // switched on by a flag file on the card, which the device reads when it starts. The mix editor
  // reads the same flag, so a preview or an export of a mix sounds as the device will play it.
  const OPTIONS: { mode: VocalMode; title: string; text: string }[] = [
    { mode: "off", title: "Off", text: "The stock Wah effect." },
    { mode: "vocal", title: "Vocal", text: "Wah pad right keeps the vocals, left cuts them." },
    { mode: "inverted", title: "Vocal inverted", text: "Wah pad right cuts the vocals, left keeps them." },
  ];
  const MIN_FIRMWARE = 180096073;

  let pick = $state<VocalMode | null>(null);
  // null while the flag could not be read: no option is marked, and choosing one writes the file anew
  const shown = $derived<VocalMode | null>(pick ?? (vocal.state ? vocal.state.mode : vocal.error ? null : "off"));
  const busy = $derived(pick !== null || device.blocked);

  // read when the card is opened and whenever the library is re-read
  $effect(() => {
    const root = session.library?.root;
    if (root) void vocal.load(root);
  });

  async function choose(mode: VocalMode) {
    const lib = session.library;
    if (!lib || mode === shown) return;
    pick = mode;
    try {
      await vocal.set(lib.root, mode);
      const name = OPTIONS.find((o) => o.mode === mode)!.title;
      toast.show(`Vocal filter: ${name}. The Pacemaker reads it when it next starts.`, "ok");
    } catch (e) {
      toast.error("Couldn't set the vocal filter.", e);
    } finally {
      pick = null;
    }
  }
</script>

<Card title="Vocal filter">
  {#snippet aside()}<HelpLink id="device-page" anchor="vocal-filter" />{/snippet}
  {#if vocal.state === null && !vocal.error}
    <p class="muted">Reading the drive…</p>
  {:else}
    {#if vocal.error}
      <div class="warn" role="status">
        Could not read the setting: {vocal.error} Choosing an option rewrites the file.
        <Button small onclick={() => vocal.reload()}>Try again</Button>
      </div>
    {/if}
    <fieldset disabled={busy} title={device.blocked && pick === null ? "A job is still writing to the device" : undefined}>
      <legend class="sr">Vocal filter on the Wah effect</legend>
      {#each OPTIONS as o (o.mode)}
        <label class="opt" class:on={shown === o.mode}>
          <input type="radio" name="vocal-mode" value={o.mode} checked={shown === o.mode} onchange={() => choose(o.mode)} />
          <span class="t">{o.title}</span>
          <span class="muted d">{o.text}</span>
        </label>
      {/each}
    </fieldset>
    {#if vocal.state?.kill_switch}
      <div class="warn" role="status">
        revival-no-preload is on this card: stock Wah plays on the device and in mix previews.
      </div>
    {/if}
    {#if vocal.state?.firmware_too_old}
      <div class="warn" role="status">
        Firmware is older than {MIN_FIRMWARE}: stock Wah plays on the device and in mix previews until updated.
      </div>
    {/if}
    <div class="note">
      Needs Revival firmware {MIN_FIRMWARE} or later. Applies at the next boot.
    </div>
  {/if}
</Card>

<style>
  fieldset {
    border: 0;
    margin: 0;
    padding: 0;
    min-width: 0;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .opt {
    display: grid;
    grid-template-columns: auto 1fr;
    column-gap: 8px;
    padding: 6px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    margin-bottom: 6px;
    cursor: pointer;
  }
  .opt:hover {
    border-color: var(--line-hi);
  }
  .opt.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .opt input {
    grid-row: 1 / span 2;
    align-self: center;
    margin: 0;
    accent-color: var(--accent);
  }
  .t {
    color: var(--text);
  }
  .d {
    font-size: 12px;
  }
  fieldset:disabled .opt {
    cursor: default;
    opacity: 0.6;
  }
  .warn {
    margin: 8px 0;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--bad-bg);
    color: var(--bad-text);
    font-size: 12px;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    margin-top: 10px;
    padding-top: 10px;
  }
</style>
