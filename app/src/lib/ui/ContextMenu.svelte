<script lang="ts">
  export type MenuItem = { label: string; onclick: () => void; disabled?: boolean; danger?: boolean };

  // A right-click menu at viewport position (x, y). Mount inside {#if}; it closes on any click
  // outside, Escape, or after an item runs.
  let { x, y, items, onclose }: { x: number; y: number; items: MenuItem[]; onclose: () => void } = $props();
</script>

<svelte:window onclick={onclose} onkeydown={(e) => e.key === "Escape" && onclose()} oncontextmenu={onclose} />

<ul class="menu" role="menu" style:left="{x}px" style:top="{y}px">
  {#each items as item (item.label)}
    <li role="none">
      <button
        role="menuitem"
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={() => {
          item.onclick();
          onclose();
        }}
      >{item.label}</button>
    </li>
  {/each}
</ul>

<style>
  .menu {
    position: fixed;
    z-index: 40;
    min-width: 160px;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  button {
    display: block;
    width: 100%;
    padding: 4px 10px;
    background: none;
    border: none;
    border-radius: 4px;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--hover);
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .danger {
    color: var(--bad);
  }
</style>
