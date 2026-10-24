<script lang="ts">
  import { dbToFader, faderToDb, formatDb } from "../format";

  // Vertical console fader with a coloured cap and dB scale.
  let {
    db,
    color,
    label,
    dim = false,
    onchange,
  }: { db: number; color: string; label: string; dim?: boolean; onchange: (db: number) => void } = $props();

  const pos = $derived(dbToFader(db));
  const TICKS = [6, 0, -6, -12, -24, -48];
  let track: HTMLDivElement | undefined = $state();

  function fromPointer(e: PointerEvent): void {
    if (!track) return;
    const r = track.getBoundingClientRect();
    onchange(faderToDb(1 - (e.clientY - r.top) / r.height));
  }
  function down(e: PointerEvent): void {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    fromPointer(e);
  }
  function move(e: PointerEvent): void {
    if (e.buttons & 1) fromPointer(e);
  }
  function key(e: KeyboardEvent): void {
    const step = e.shiftKey ? 0.1 : 1;
    if (e.key === "ArrowUp") onchange(Math.min(6, db + step));
    else if (e.key === "ArrowDown") onchange(Math.max(-96, db - step));
    else return;
    e.preventDefault();
  }
</script>

<div class="fader" class:dim>
  <div class="scale" aria-hidden="true">
    {#each TICKS as t (t)}
      <span style={`bottom:${dbToFader(t) * 100}%`}>{t > 0 ? `+${t}` : t}</span>
    {/each}
  </div>
  <div
    class="track"
    bind:this={track}
    role="slider"
    tabindex="0"
    aria-label={label}
    aria-valuemin={-96}
    aria-valuemax={6}
    aria-valuenow={db}
    aria-valuetext={`${formatDb(db)} dB`}
    onpointerdown={down}
    onpointermove={move}
    ondblclick={() => onchange(0)}
    onkeydown={key}
  >
    <div class="slot"></div>
    <div class="cap" style={`bottom:${pos * 100}%;--c:${color}`}><i></i></div>
  </div>
</div>

<style>
  .fader {
    display: flex;
    gap: 4px;
    height: 100%;
    transition: opacity 0.15s;
  }
  .dim {
    opacity: 0.45;
  }
  .scale {
    position: relative;
    width: 18px;
    margin: 10px 0;
  }
  .scale span {
    position: absolute;
    right: 0;
    translate: 0 50%;
    font-size: 11px; /* fader scale, beside a 76 px strip */
    font-family: var(--mono);
    color: var(--text-faint);
  }
  .track {
    position: relative;
    width: 28px;
    margin: 10px 0;
    cursor: ns-resize;
    touch-action: none;
  }
  .slot {
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    width: 2px;
    translate: -50% 0;
    border-radius: 0;
    background: var(--line-strong);
  }
  .cap {
    position: absolute;
    left: 50%;
    width: 24px;
    height: 12px;
    translate: -50% 50%;
    border-radius: 1px;
    background: var(--bg-active);
    border: 1px solid var(--line-strong);
    display: grid;
    place-items: center;
  }
  .cap i {
    width: 16px;
    height: 2px;
    border-radius: 0;
    background: var(--c);
  }
</style>
