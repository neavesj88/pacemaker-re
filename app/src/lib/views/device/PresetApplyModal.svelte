<script lang="ts">
  import { devicePresetApply, devicePresetDiff, type PresetDiff } from "$lib/api/device";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { device } from "./device.svelte";
  import { plural } from "./format";
  import { defaultTicks, groupRows, headline, idsToWrite, rowText, writeLabel } from "./presetLogic";
  import WideModal from "./WideModal.svelte";

  // A preset against the card that is open: every setting it mentions, what the card has now and
  // what the preset wants, in words. Only the ticked "will change" rows are written, and the
  // backend works the diff out again on the card as it is when you press Write, with the same
  // function that produced this list.
  let { name, onclose }: { name: string; onclose: () => void } = $props();

  let diff = $state<PresetDiff | null>(null);
  let error = $state("");
  let ticked = $state<Set<number>>(new Set());
  let writing = $state(false);

  $effect(() => {
    const root = session.library?.root;
    if (!root) return;
    devicePresetDiff(root, name).then(
      (d) => {
        diff = d;
        ticked = new Set(defaultTicks(d.rows));
      },
      (e) => (error = String(e)),
    );
  });

  const groups = $derived(groupRows(diff?.rows ?? []));
  const ids = $derived(diff ? idsToWrite(diff.rows, ticked) : []);

  function toggle(id: number) {
    const next = new Set(ticked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    ticked = next;
  }

  function setAll(on: boolean) {
    ticked = on && diff ? new Set(defaultTicks(diff.rows)) : new Set();
  }

  async function write() {
    const lib = session.library;
    if (!lib || ids.length === 0) return;
    writing = true;
    try {
      const rep = await devicePresetApply(lib.root, name, ids);
      toast.show(
        rep.changed === 0
          ? `Nothing needed changing: the card already had those values.`
          : `${plural(rep.changed, "setting")} written from "${name}". The Pacemaker reads them when it next starts.`,
        "ok",
        6000,
      );
      await device.load(lib.root);
      onclose();
    } catch (e) {
      toast.show(String(e), "bad", 8000);
      // the card may have changed under the list: look again
      error = "";
      diff = null;
      const d = await devicePresetDiff(lib.root, name).catch((x) => {
        error = String(x);
        return null;
      });
      if (d) {
        diff = d;
        ticked = new Set(defaultTicks(d.rows));
      }
    } finally {
      writing = false;
    }
  }
</script>

<WideModal title={`Apply "${name}" to this card?`} {onclose} width={720}>
  {#if error}
    <p class="bad">{error}</p>
  {:else if !diff}
    <p class="muted">Comparing with the card…</p>
  {:else}
    <p class="head" role="status">{headline(diff, ids.length)}</p>
    {#if diff.change > 1}
      <p class="sel">
        <Button small variant="link" onclick={() => setAll(true)}>Tick all changes</Button>
        <Button small variant="link" onclick={() => setAll(false)}>Tick none</Button>
      </p>
    {/if}
    {#each groups as g (g.group)}
      <section>
        <h3>{g.group}</h3>
        <ul>
          {#each g.rows as r (r.id)}
            <li class={r.status} class:off={r.status === "change" && !ticked.has(r.id)}>
              {#if r.status === "change"}
                <input id="pa-{r.id}" type="checkbox" checked={ticked.has(r.id)} disabled={writing} onchange={() => toggle(r.id)} />
              {:else}
                <span class="nobox" aria-hidden="true"></span>
              {/if}
              <label for={r.status === "change" ? `pa-${r.id}` : undefined}>{r.title}</label>
              <span class="what">{rowText(r)}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
    <p class="muted note">
      This rewrites .Pacemaker/settings.dat on {session.library?.root}. Only the ticked values change; every other item
      in the file is written back exactly as it was. The app keeps a safety copy of the card's music.db and settings.dat
      before its first write of a session. Do not write while the device is running: eject, then power it on.
    </p>
  {/if}
  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button
      variant="primary"
      disabled={!diff || ids.length === 0 || writing || device.blocked}
      title={device.blocked ? "A job is still writing to the device" : undefined}
      onclick={write}
    >
      {writeLabel(ids.length, writing)}
    </Button>
  {/snippet}
</WideModal>

<style>
  .head {
    margin: 0 0 6px;
  }
  .sel {
    display: flex;
    gap: 10px;
    margin: 0 0 6px;
  }
  section {
    margin-top: 8px;
  }
  h3 {
    margin: 0 0 3px;
    font-size: 12px;
    color: var(--muted);
    font-weight: 600;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  li {
    display: grid;
    grid-template-columns: 22px minmax(120px, 1fr) minmax(0, 1.6fr);
    gap: 8px;
    align-items: baseline;
    padding: 4px 8px;
  }
  li:nth-child(odd) {
    background: var(--row);
  }
  .nobox {
    display: inline-block;
    width: 13px;
  }
  .what {
    overflow-wrap: anywhere;
  }
  li.change .what {
    color: var(--accent);
  }
  li.off .what {
    color: var(--muted);
    text-decoration: line-through;
  }
  li.same {
    color: var(--muted);
  }
  li.absent,
  li.unknown,
  li.invalid {
    color: var(--muted);
  }
  li.absent .what,
  li.unknown .what,
  li.invalid .what {
    color: var(--bad-text);
    font-size: 12px;
  }
  .bad {
    color: var(--bad-text);
  }
  .note {
    font-size: 12px;
    margin: 12px 0 0;
  }
</style>
