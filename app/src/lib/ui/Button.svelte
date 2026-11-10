<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  // variant: default (outlined) | primary (magenta ramp) | link (accent text) | ghost (no border)
  let {
    variant = "default",
    small = false,
    active = false,
    children,
    ...rest
  }: HTMLButtonAttributes & {
    variant?: "default" | "primary" | "link" | "ghost";
    small?: boolean;
    active?: boolean;
    children?: Snippet;
  } = $props();
</script>

<button class="btn {variant}" class:small class:active {...rest}>{@render children?.()}</button>

<style>
  .btn {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 5px 10px;
    font: inherit;
    cursor: pointer;
  }
  .btn:hover {
    border-color: var(--line-hi);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .primary {
    background: var(--accent-ramp);
    color: #000;
    font-weight: 600;
    border-color: var(--accent);
  }
  .small {
    padding: 2px 8px;
    font-size: 12px;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    text-align: left;
  }
  .ghost {
    background: none;
    border-color: transparent;
  }
  .active {
    background: var(--accent-soft);
    color: var(--accent);
  }
</style>
