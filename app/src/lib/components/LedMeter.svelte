<script lang="ts">
  import { peakToMeter } from "../format";

  // Segmented LED ladder (green → amber → red), like hardware consoles.
  let { level = 0, segments = 18 }: { level?: number; segments?: number } = $props();
  const lit = $derived(Math.round(peakToMeter(level) * segments));
</script>

<div class="leds" role="meter" aria-valuemin="0" aria-valuemax={segments} aria-valuenow={lit}>
  {#each Array.from({ length: segments }, (_, i) => segments - 1 - i) as i (i)}
    <i class:on={i < lit} class:warn={i >= segments * 0.72} class:hot={i >= segments * 0.9}></i>
  {/each}
</div>

<style>
  .leds {
    display: flex;
    flex-direction: column;
    gap: 2px;
    height: 100%;
    padding: 10px 0;
    width: 6px;
  }
  i {
    flex: 1;
    border-radius: 1px;
    background: #1a2a22;
  }
  i.warn {
    background: #33290f;
  }
  i.hot {
    background: #3a1517;
  }
  i.on {
    background: var(--ok);
    box-shadow: 0 0 4px var(--ok);
  }
  i.on.warn {
    background: var(--accent);
    box-shadow: 0 0 4px var(--accent);
  }
  i.on.hot {
    background: var(--danger);
    box-shadow: 0 0 4px var(--danger);
  }
</style>
