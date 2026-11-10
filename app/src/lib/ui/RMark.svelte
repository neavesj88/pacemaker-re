<script lang="ts">
  // The Pacemaker Revival dot R (branding/r_logo.py): 12 dots, L2 magenta ramp top to bottom.
  let { size = 20 }: { size?: number } = $props();
  const ramp = ["#e070ff", "#f060ff", "#ff50ff", "#ff50e8", "#ff50d0", "#ff48a8"];
  const dots: [number, number][] = [
    [0, 0], [1, 0], [2, 0], [0, 1], [2.55, 1], [0, 2], [1, 2], [2, 2], [0, 3], [1.8, 3], [0, 4], [2.75, 4.15],
  ];
  function colour(row: number): string {
    return ramp[Math.round((Math.min(row, 4) / 4) * (ramp.length - 1))];
  }
</script>

<svg width={size * 0.78} height={size} viewBox="-0.45 -0.45 3.65 5.05" aria-label="Pacemaker Revival" role="img">
  {#each dots as [x, y]}
    <circle cx={x} cy={y} r="0.4" fill={colour(y)} />
  {/each}
</svg>

<style>
  svg {
    display: inline-block;
    vertical-align: middle;
    flex: none;
  }
</style>
