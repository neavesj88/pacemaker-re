<script lang="ts">
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";

  // The stack sits above the player bar, whatever height the owner dragged it to: it measures the
  // bar (when there is one) and keeps following it, so a message never covers the player controls.
  const GAP = 8;
  let lift = $state(0);
  // In the mix editor the messages dock just above the transport row, so they cover neither the clip
  // panel nor the picker's rows, scrollbar and helper text (only the foot of the timeline, for the few
  // seconds they stay); null = not there, use the player-bar position.
  let dock = $state<{ bottom: number; left: number } | null>(null);
  // cards whose Details are open: they stay until closed, so a long error can be read
  let open = $state<Record<number, boolean>>({});
  function toggle(id: number) {
    open[id] = !open[id];
    if (open[id]) toast.hold(id);
  }
  function leave(id: number) {
    if (!open[id]) toast.release(id);
  }

  $effect(() => {
    void toast.items.length;
    // the player bar mounts when a library opens, which may be after the message appeared
    void session.library;
    const bar = document.querySelector<HTMLElement>("footer.player");
    if (!bar) {
      lift = 0;
      return;
    }
    const measure = () => (lift = Math.ceil(bar.getBoundingClientRect().height));
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(bar);
    return () => ro.disconnect();
  });

  $effect(() => {
    void toast.items.length;
    dock = null;
    if (nav.view !== "mixes") return;
    const row = document.querySelector<HTMLElement>(".transport");
    if (!row) return;
    const measure = () => {
      const r = row.getBoundingClientRect();
      dock = r.width > 0 ? { bottom: Math.max(GAP, Math.round(innerHeight - r.top + 4)), left: Math.round(r.left + r.width / 2) } : null;
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(row);
    addEventListener("resize", measure);
    return () => {
      ro.disconnect();
      removeEventListener("resize", measure);
    };
  });
</script>

<!-- In the mix editor the clip panel fills the lower right and the picker the bottom, so the messages sit above the transport row there -->
<div class="toasts" class:centre={nav.view === "mixes"} aria-live="polite" style="bottom: {dock && nav.view === 'mixes' ? dock.bottom : lift + GAP}px; {dock && nav.view === 'mixes' ? `left: ${dock.left}px; max-width: min(560px, calc(100vw - 28px))` : ''}">
  {#each toast.items as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === "bad" ? "alert" : "status"} onmouseenter={() => toast.hold(t.id)} onmouseleave={() => leave(t.id)} onfocusin={() => toast.hold(t.id)} onfocusout={() => leave(t.id)}>
      <div class="body">
        <span class="text">{t.text}{#if t.count > 1} <span class="times">×{t.count}</span>{/if}</span>
        {#if t.details && open[t.id]}<span class="details">{t.details}</span>{/if}
      </div>
      {#if t.action}
        {@const act = t.action}
        <button class="act" onclick={() => { act.run(); toast.dismiss(t.id); }}>{act.label}</button>
      {/if}
      {#if t.details}
        <button class="act quiet" aria-expanded={!!open[t.id]} onclick={() => toggle(t.id)}>{open[t.id] ? "Less" : "Details"}</button>
      {/if}
      <button class="close" aria-label="Dismiss this message" title="Dismiss" onclick={() => toast.dismiss(t.id)}>×</button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 14px;
    /* just above the player bar, whatever height it has (Player.svelte publishes --player-h) */
    bottom: calc(var(--player-h, 48px) + 8px);
    z-index: 50;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: min(560px, calc(100vw - 28px));
    pointer-events: none;
  }
  .toasts.centre {
    right: auto;
    left: 50%;
    transform: translateX(-50%);
    width: max-content;
    align-items: stretch;
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 6px 8px 6px 12px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
    pointer-events: auto;
  }
  .times {
    color: var(--muted);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  /* a stray long string cannot cover the timeline: three lines, then an ellipsis (Details has the rest) */
  .text {
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .details {
    white-space: pre-line;
    overflow-wrap: anywhere;
    max-height: 9em;
    overflow-y: auto;
    color: var(--muted);
    font-size: 0.92em;
    user-select: text;
  }
  .act {
    flex: none;
    background: none;
    border: 1px solid currentColor;
    border-radius: 4px;
    color: inherit;
    font: inherit;
    padding: 0 8px;
    cursor: pointer;
  }
  .act.quiet {
    border-color: transparent;
    text-decoration: underline;
    padding: 0 4px;
    opacity: 0.8;
  }
  .close {
    flex: none;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    line-height: 1;
    padding: 0 4px;
    cursor: pointer;
    opacity: 0.7;
  }
  .close:hover,
  .close:focus-visible {
    opacity: 1;
  }
  .ok {
    border-color: var(--ok);
  }
  .bad {
    background: var(--bad-bg);
    border-color: var(--bad);
    color: var(--bad-text);
  }
</style>
