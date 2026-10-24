<script lang="ts">
  import Icon from "./Icon.svelte";
  // The song as a chart of bars (8 per line, like a lead sheet). Click a bar
  // to jump to it; drag across bars to loop them — loops always snap to bars.
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";

  type Cell = { label: string; start: number; end: number; beats: number[]; intro: boolean };

  const song = $derived(app.song);
  const duration = $derived(song?.project.durationSecs ?? 0);

  /** Loudest unmuted stem per peak bucket (0..1). */
  const energy = $derived.by(() => {
    if (!song) return [] as number[];
    const audible = song.project.tracks.map((t) => !t.muted);
    const n = song.peaks[0]?.length ?? 0;
    return Array.from({ length: n }, (_, i) =>
      song.peaks.reduce((m, p, k) => (audible[k] ? Math.max(m, p[i] ?? 0) : m), 0) / 255,
    );
  });

  function energyBetween(a: number, b: number): number {
    const n = energy.length;
    if (n === 0 || duration <= 0) return 0;
    const i0 = Math.max(0, Math.floor((a / duration) * n));
    const i1 = Math.min(n, Math.max(i0 + 1, Math.ceil((b / duration) * n)));
    let sum = 0;
    for (let i = i0; i < i1; i++) sum += energy[i] ?? 0;
    return sum / (i1 - i0);
  }

  const cells = $derived.by((): Cell[] => {
    if (!song) return [];
    const a = song.project.analysis;
    const bpb = Math.max(1, a.beatsPerBar);
    const out: Cell[] = [];
    const downIdx = a.beats.map((_, i) => i).filter((i) => i % bpb === a.downbeatPhase % bpb);
    if (downIdx.length === 0) {
      // No beat grid: plain 2-second cells.
      for (let t = 0, k = 1; t < duration; t += 2, k++) {
        out.push({ label: String(k), start: t, end: Math.min(duration, t + 2), beats: [energyBetween(t, t + 2)], intro: false });
      }
      return out;
    }
    const firstDown = a.beats[downIdx[0] ?? 0] ?? 0;
    if (firstDown > 0.3) {
      out.push({ label: i18n.t("barmap.intro"), start: 0, end: firstDown, beats: [energyBetween(0, firstDown)], intro: true });
    }
    downIdx.forEach((bi, k) => {
      const start = a.beats[bi] ?? 0;
      const period = (a.beats[bi + 1] ?? start + 60 / a.bpm) - start;
      const end = a.beats[downIdx[k + 1] ?? -1] ?? Math.min(duration, start + period * bpb);
      const beats = Array.from({ length: bpb }, (_, j) => {
        const s = a.beats[bi + j] ?? start + j * period;
        const e = a.beats[bi + j + 1] ?? s + period;
        return energyBetween(s, e);
      });
      out.push({ label: String(k + 1), start, end, beats, intro: false });
    });
    return out;
  });

  const peakEnergy = $derived(Math.max(0.05, ...cells.flatMap((c) => c.beats)));
  const pos = $derived(app.transport.positionSecs);
  const current = $derived(cells.findIndex((c) => pos >= c.start && pos < c.end));

  // Auto-scroll: keep the playing bar in view, unless the user scrolled
  // the map in the last few seconds.
  let grid: HTMLDivElement | undefined = $state();
  let userScrollUntil = 0;
  $effect(() => {
    const i = current;
    if (!grid || i < 0 || performance.now() < userScrollUntil || dragFrom !== null) return;
    const el = grid.querySelector<HTMLElement>(`[data-i="${i}"]`);
    if (!el) return;
    const g = grid.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    if (r.top < g.top || r.bottom > g.bottom) {
      grid.scrollTo({ top: grid.scrollTop + (r.top - g.top) - (g.height - r.height) / 2, behavior: "smooth" });
    }
  });

  let dragFrom = $state<number | null>(null);
  let dragTo = $state<number | null>(null);

  function cellAt(e: MouseEvent): number | null {
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-i]");
    return el ? Number(el.dataset.i) : null;
  }
  function down(e: PointerEvent): void {
    if (e.button !== 0) return;
    const i = cellAt(e);
    if (i === null) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragFrom = dragTo = i;
  }
  function move(e: PointerEvent): void {
    if (dragFrom === null) return;
    const i = cellAt(e);
    if (i !== null) dragTo = i;
  }
  /** Loops cells `a..=b`; jumps to the start when outside the new loop. */
  function loopCells(a: number, b: number, jump = false): void {
    const [lo, hi] = [Math.max(0, Math.min(a, b)), Math.min(cells.length - 1, Math.max(a, b))];
    const ca = cells[lo];
    const cb = cells[hi];
    if (!ca || !cb) return;
    void app.setLoopRegion(ca.start, cb.end).then(() => {
      if (jump || pos < ca.start || pos >= cb.end) void app.seek(ca.start);
    });
  }

  /** Index of the first/last cell covered by the current loop. */
  const loopSpan = $derived.by(() => {
    if (!app.loop) return null;
    const { start, end } = app.loop;
    const a = cells.findIndex((c) => c.start >= start - 0.01);
    const b = cells.findLastIndex((c) => c.end <= end + 0.01);
    return a >= 0 && b >= a ? { a, b } : null;
  });

  function up(e: PointerEvent): void {
    if (dragFrom !== null && dragTo !== null) {
      const [a, b] = [Math.min(dragFrom, dragTo), Math.max(dragFrom, dragTo)];
      if (e.shiftKey && loopSpan) loopCells(Math.min(loopSpan.a, a), Math.max(loopSpan.b, b));
      else if (a !== b) loopCells(a, b);
      else if (cells[a]) void app.seek(cells[a].start);
    }
    dragFrom = dragTo = null;
  }

  function dbl(e: MouseEvent): void {
    // Pointer capture retargets the event to the grid, so locate the bar
    // under the mouse instead of trusting `e.target`.
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-i]");
    if (!el) return;
    const i = Number(el.dataset.i);
    loopCells(i, i, true);
    if (!app.transport.playing && !app.transport.countingIn) void app.togglePlay();
  }

  /** Repeat `n` bars starting at the playing bar (or the first bar). */
  function repeatBars(n: number): void {
    const start = current >= 0 ? current : cells.findIndex((c) => !c.intro);
    loopCells(Math.max(0, start), Math.max(0, start) + n - 1);
  }

  const loopBarsCount = $derived(loopSpan && app.loopEnabled ? loopSpan.b - loopSpan.a + 1 : 0);
  const loopLabel = $derived.by(() => {
    if (!loopSpan || !app.loopEnabled) return "";
    const a = cells[loopSpan.a]?.label ?? "";
    const b = cells[loopSpan.b]?.label ?? "";
    return loopSpan.a === loopSpan.b ? i18n.t("loop.single", { a }) : i18n.t("loop.active", { a, b });
  });

  function selected(c: Cell, i: number): boolean {
    if (dragFrom !== null && dragTo !== null) return i >= Math.min(dragFrom, dragTo) && i <= Math.max(dragFrom, dragTo);
    return !!app.loop && c.start >= app.loop.start - 0.01 && c.end <= app.loop.end + 0.01;
  }
</script>

{#if song}
  <section class="map">
    <div class="toolbar">
      <span class="label">{i18n.t("loop.repeat")}</span>
      <div class="seg" role="group" aria-label={i18n.t("loop.repeat")}>
        {#each [1, 2, 4, 8] as n (n)}
          <button
            class:on={loopBarsCount === n}
            title={i18n.t("loop.barsHint", { n })}
            aria-pressed={loopBarsCount === n}
            onclick={() => repeatBars(n)}>{n}</button
          >
        {/each}
        <button class="off" disabled={!app.loop} onclick={() => app.clearLoop()}>{i18n.t("loop.off")}</button>
      </div>
      {#if loopLabel}<span class="status"><Icon name="loop" size={12} /> {loopLabel}</span>{/if}
      <span class="hint" title={i18n.t("barmap.hint")}>{i18n.t("barmap.hint")}</span>
    </div>
    <div
      bind:this={grid}
      onwheel={() => (userScrollUntil = performance.now() + 4000)}
      class="grid"
      role="grid"
      tabindex="-1"
      aria-label={i18n.t("song.bar")}
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={() => (dragFrom = dragTo = null)}
      ondblclick={dbl}
    >
      {#each cells as cell, i (i)}
        {@const isCurrent = i === current}
        <div
          class="cell"
          class:intro={cell.intro}
          class:current={isCurrent}
          class:selected={selected(cell, i)}
          class:looping={selected(cell, i) && app.loopEnabled && dragFrom === null}
          class:phrase={!cell.intro && (Number(cell.label) - 1) % 4 === 0}
          role="gridcell"
          data-i={i}
        >
          <span class="label tabular">{cell.label}</span>
          <div class="beats">
            {#each cell.beats as e, j (j)}
              <i style={`height:${12 + (e / peakEnergy) * 88}%`} class:first={j === 0 && !cell.intro}></i>
            {/each}
          </div>
          {#if isCurrent}
            <div class="progress" style={`width:${((pos - cell.start) / Math.max(0.001, cell.end - cell.start)) * 100}%`}></div>
          {/if}
        </div>
      {/each}
    </div>
  </section>
{/if}

<style>
  .map {
    flex: none;
    padding: 10px 24px;
    border-bottom: 1px solid var(--line);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
    min-width: 0;
  }
  .label {
    font-size: var(--fs-label);
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .seg {
    display: flex;
    padding: 2px;
    border-radius: 7px;
    background: var(--bg-panel);
    border: 1px solid var(--line);
  }
  .seg button {
    min-width: 28px;
    height: 22px;
    padding: 0 7px;
    border-radius: 5px;
    font-size: var(--fs-small);
    font-weight: 700;
    color: var(--text-dim);
  }
  .seg button:hover:not(:disabled) {
    color: var(--text);
  }
  .seg button.on {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .seg .off:disabled {
    opacity: 0.4;
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: var(--fs-small);
    font-weight: 700;
    color: var(--accent);
    white-space: nowrap;
  }
  .hint {
    margin-left: auto;
    font-size: var(--fs-small);
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .grid {
    /* Three rows visible; the playing bar is kept in view automatically. */
    max-height: 132px;
    overflow-y: auto;
    scroll-behavior: smooth;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(52px, 1fr));
    grid-auto-rows: 40px;
    gap: 4px;
    user-select: none;
    touch-action: none;
  }
  .cell {
    position: relative;
    overflow: hidden;
    border-radius: 6px;
    background: var(--bg-panel);
    border: 1px solid var(--line);
    padding: 3px 5px;
    display: flex;
    flex-direction: column;
    cursor: pointer;
    transition: border-color 0.1s, background 0.1s;
  }
  .cell:hover {
    border-color: var(--line-strong);
    background: var(--bg-hover);
  }
  .cell.phrase {
    border-left: 2px solid var(--line-strong);
  }
  .cell.intro {
    background: repeating-linear-gradient(135deg, var(--bg-panel) 0 6px, var(--bg-raised) 6px 12px);
  }
  .label {
    font-family: var(--mono);
    font-size: 11px; /* bar number in a 52 px cell */
    line-height: 1.1;
    font-weight: 700;
    color: var(--text-faint);
  }
  .beats {
    flex: 1;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    padding-top: 2px;
  }
  .beats i {
    flex: 1;
    border-radius: 2px 2px 0 0;
    background: color-mix(in srgb, var(--text) 18%, transparent);
  }
  .beats i.first {
    background: color-mix(in srgb, var(--text) 30%, transparent);
  }
  .selected {
    border-color: color-mix(in srgb, var(--accent) 50%, transparent);
    background: color-mix(in srgb, var(--accent) 7%, var(--bg-panel));
  }
  .looping {
    border-color: var(--accent);
  }
  .looping .beats i {
    background: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .current {
    border-color: var(--text);
    background: var(--bg-active);
    box-shadow: 0 0 0 1px var(--text), 0 6px 20px rgb(0 0 0 / 40%);
  }
  .current .label {
    color: var(--text);
  }
  .current .beats i {
    background: var(--accent);
  }
  .progress {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 3px;
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent);
  }
  /* Short windows: two rows of bars leave the console its height. */
  @media (max-height: 760px) {
    .map {
      padding-top: 6px;
      padding-bottom: 6px;
    }
    .toolbar {
      margin-bottom: 5px;
    }
    .grid {
      max-height: 84px;
      scroll-snap-type: y mandatory;
    }
    .cell {
      scroll-snap-align: start;
    }
  }
</style>
