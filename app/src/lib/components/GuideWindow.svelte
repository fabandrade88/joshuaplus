<script lang="ts">
  // The play-along window: the chord now and the ones coming, and the lyric
  // line being sung with the next ones, big enough to read from the
  // instrument. It floats over the song screen; drag it by its title bar,
  // resize it from the corner (the text grows with it). Its place is
  // remembered.
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import { barBeatAt, chordAt, lineAt, toLrc } from "../timeline";
  import Icon from "./Icon.svelte";

  /** Lines show a little early, so the singer can read ahead. */
  const LEAD_SECS = 0.3;
  const KEY = "joshua.guideWindow";
  const MIN_W = 360;
  const MIN_H = 180;

  const pos = $derived(app.transport.positionSecs);
  const project = $derived(app.song?.project);

  // ── Chords ──
  const chordList = $derived(app.chordsOn ? (app.chords?.chords ?? []) : []);
  const chordIdx = $derived(chordAt(chordList, pos));
  const upcoming = $derived.by(() => {
    const from = chordIdx >= 0 ? chordIdx + 1 : chordList.findIndex((c) => c.start > pos);
    return from >= 0 ? chordList.slice(from, from + 3) : [];
  });
  /** Beats until the next chord change. */
  const beatsToNext = $derived.by(() => {
    const next = upcoming[0];
    const a = project?.analysis;
    if (!next || !a || a.beats.length === 0) return null;
    const n = a.beats.filter((b) => b > pos + 0.02 && b <= next.start + 0.02).length;
    return n > 0 ? n : null;
  });
  const bb = $derived(project ? barBeatAt(project.analysis, pos) : null);
  const bpb = $derived(project?.analysis.beatsPerBar ?? 4);

  // ── Lyrics ──
  const lyrics = $derived(app.lyricsOn ? app.lyrics : null);
  const synced = $derived(lyrics?.synced ?? []);
  const lineIdx = $derived(synced.length ? lineAt(synced, pos + LEAD_SECS, lyrics?.offsetSecs ?? 0) : -1);
  const line = $derived(lineIdx >= 0 ? (synced[lineIdx]?.text ?? "") : "");
  const nextLines = $derived(
    synced
      .slice(lineIdx + 1)
      .filter((l) => l.text.trim())
      .slice(0, 2)
      .map((l) => l.text),
  );

  let editing = $state(false);
  let draft = $state("");
  function edit(): void {
    draft = app.lyrics ? (app.lyrics.synced.length ? toLrc(app.lyrics.synced) : app.lyrics.plain) : "";
    editing = true;
  }
  async function save(): Promise<void> {
    await app.saveLyrics(draft);
    editing = false;
  }

  // ── Placement ──
  type Box = { x: number; y: number; w: number; h: number };
  function defaultBox(): Box {
    const w = Math.min(900, Math.round(window.innerWidth * 0.6));
    const h = Math.min(380, Math.round(window.innerHeight * 0.42));
    return { x: Math.round((window.innerWidth - w) / 2 + 120), y: window.innerHeight - h - 32, w, h };
  }
  function fit(b: Box): Box {
    const w = Math.max(MIN_W, Math.min(b.w, window.innerWidth - 16));
    const h = Math.max(MIN_H, Math.min(b.h, window.innerHeight - 16));
    return { w, h, x: Math.max(8, Math.min(b.x, window.innerWidth - w - 8)), y: Math.max(8, Math.min(b.y, window.innerHeight - h - 8)) };
  }
  function load(): Box {
    try {
      const b = JSON.parse(localStorage.getItem(KEY) ?? "null") as Box | null;
      if (b && [b.x, b.y, b.w, b.h].every(Number.isFinite)) return fit(b);
    } catch {
      /* no stored place */
    }
    return fit(defaultBox());
  }
  function store(): void {
    try {
      localStorage.setItem(KEY, JSON.stringify(box));
    } catch {
      /* not remembered */
    }
  }

  let box = $state<Box>(load());
  let win: HTMLElement | undefined = $state();
  let drag: { dx: number; dy: number } | null = null;

  function grab(e: PointerEvent): void {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    drag = { dx: e.clientX - box.x, dy: e.clientY - box.y };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function moveTo(e: PointerEvent): void {
    if (!drag) return;
    box = fit({ ...box, x: e.clientX - drag.dx, y: e.clientY - drag.dy });
  }
  function drop(): void {
    if (!drag) return;
    drag = null;
    store();
  }

  // The native resize handle changes the element's size; keep the box in step.
  $effect(() => {
    if (!win) return;
    const el = win;
    const ro = new ResizeObserver(() => {
      const w = Math.round(el.offsetWidth);
      const h = Math.round(el.offsetHeight);
      if (w !== box.w || h !== box.h) {
        box = { ...box, w, h };
        store();
      }
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  function reset(): void {
    box = fit(defaultBox());
    store();
  }

  function close(): void {
    if (app.chordsOn) app.toggleChords();
    if (app.lyricsOn) app.toggleLyrics();
  }

  const both = $derived(app.chordsOn && app.lyricsOn);
</script>

<svelte:window onresize={() => (box = fit(box))} />

<section
  bind:this={win}
  class="guide"
  class:both
  style={`left:${box.x}px;top:${box.y}px;width:${box.w}px;height:${box.h}px`}
  aria-label={i18n.t("guide.label")}
>
  <header role="toolbar" tabindex="-1" onpointerdown={grab} onpointermove={moveTo} onpointerup={drop} onpointercancel={drop} ondblclick={reset} title={i18n.t("guide.moveHint")}>
    <span class="title">
      {#if app.chordsOn}<Icon name="chords" size={13} /> {i18n.t("guide.chords")}{#if app.chords}<em>· {i18n.t("guide.key", { key: app.chords.key })}</em>{/if}{/if}
      {#if both}<span class="dot">·</span>{/if}
      {#if app.lyricsOn}<Icon name="lyrics" size={13} /> {i18n.t("guide.lyrics")}{/if}
    </span>
    {#if app.lyricsOn && app.lyrics?.synced.length}
      <div class="nudge" title={i18n.t("guide.offsetHint")}>
        <button aria-label={i18n.t("guide.earlier")} onclick={() => app.nudgeLyrics(-0.5)}>−</button>
        <span class="tabular">{(app.lyrics.offsetSecs > 0 ? "+" : "") + app.lyrics.offsetSecs.toFixed(1)} s</span>
        <button aria-label={i18n.t("guide.later")} onclick={() => app.nudgeLyrics(0.5)}>+</button>
      </div>
    {/if}
    {#if app.lyricsOn && app.lyrics}
      <button class="icon" title={i18n.t("guide.edit")} aria-label={i18n.t("guide.edit")} onclick={edit}><Icon name="edit" size={14} /></button>
    {/if}
    <button class="icon" title={i18n.t("guide.close")} aria-label={i18n.t("guide.close")} onclick={close}><Icon name="close" size={14} /></button>
  </header>

  <div class="content">
    {#if app.chordsOn}
      <div class="chords">
        {#if app.chordsStatus === "loading"}
          <p class="status">{i18n.t("guide.chordsLoading")}</p>
        {:else if app.chordsStatus === "failed"}
          <p class="status">{i18n.t("guide.chordsFailed")}</p>
        {:else}
          <div class="now-chord">
            <span class="chord-now">{chordIdx >= 0 ? (chordList[chordIdx]?.name ?? "–") : "–"}</span>
            <div class="beats" aria-hidden="true">
              {#each Array.from({ length: bpb }, (_, i) => i + 1) as b (b)}
                <i class:on={!app.transport.countingIn && bb?.beat === b} class:one={b === 1}></i>
              {/each}
            </div>
          </div>
          <div class="next-chords">
            {#each upcoming as c, i (c.start)}
              <span class="chord-next" style={`opacity:${1 - i * 0.25}`}>{c.name}</span>
            {/each}
            {#if beatsToNext !== null}<span class="count">{i18n.t("guide.inBeats", { n: beatsToNext })}</span>{/if}
          </div>
        {/if}
      </div>
    {/if}

    {#if app.lyricsOn}
      <div class="lyrics">
        {#if editing}
          <div class="editor">
            <textarea bind:value={draft} spellcheck="false" placeholder={i18n.t("guide.pastePlaceholder")} aria-label={i18n.t("guide.paste")}></textarea>
            <div class="actions">
              <span class="note">{i18n.t("guide.pasteNote")}</span>
              <button onclick={() => (editing = false)}>{i18n.t("guide.cancel")}</button>
              <button class="primary" onclick={save}>{i18n.t("guide.save")}</button>
            </div>
          </div>
        {:else if app.lyricsStatus === "loading"}
          <p class="status">{i18n.t("guide.lyricsLoading")}</p>
        {:else if app.lyricsStatus === "offline" || app.lyricsStatus === "none" || !lyrics}
          <p class="status">
            {app.lyricsStatus === "offline" ? i18n.t("guide.lyricsOffline") : i18n.t("guide.lyricsNone", { title: project?.title ?? "" })}
            <button class="link" onclick={() => app.loadLyrics(true)}>{i18n.t("guide.retry")}</button>
            <button class="link" onclick={edit}>{i18n.t("guide.paste")}</button>
          </p>
        {:else if lyrics.instrumental && synced.length === 0 && !lyrics.plain.trim()}
          <p class="status">{i18n.t("guide.instrumental")}</p>
        {:else if synced.length}
          <p class="line-now" class:empty={!line.trim()}>{line.trim() || "♪"}</p>
          {#each nextLines as l, i (i)}
            <p class="line-next" class:far={i > 0}>{l}</p>
          {/each}
        {:else}
          <pre class="plain">{lyrics.plain}</pre>
        {/if}
        {#if !editing && lyrics}
          <span class="credit">{lyrics.source === "lrclib" ? i18n.t("guide.fromLrclib") : i18n.t("guide.fromUser")}{#if !synced.length && lyrics.plain.trim()} · {i18n.t("guide.notSynced")}{/if}</span>
        {/if}
      </div>
    {/if}
  </div>
</section>

<style>
  .guide {
    position: fixed;
    z-index: 40;
    display: flex;
    flex-direction: column;
    min-width: 360px;
    min-height: 180px;
    resize: both;
    overflow: hidden;
    border-radius: var(--radius);
    background: var(--bg-raised);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow), 0 18px 48px rgb(0 0 0 / 45%);
  }
  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 6px 0 12px;
    border-bottom: 1px solid var(--line);
    background: var(--bg-panel);
    cursor: grab;
    user-select: none;
    touch-action: none;
  }
  header:active {
    cursor: grabbing;
  }
  .title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: auto;
    min-width: 0;
    font-size: var(--fs-small);
    font-weight: 600;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
  }
  .title em {
    font-style: normal;
    color: var(--accent);
  }
  .dot {
    color: var(--text-faint);
  }
  .icon,
  .nudge button {
    display: grid;
    place-items: center;
    min-width: 24px;
    height: 24px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .icon:hover,
  .nudge button:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .nudge {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 1px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line);
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .nudge span {
    min-width: 46px;
    text-align: center;
  }

  /* The text follows the window size. */
  .content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2cqh;
    padding: 2cqh 4cqw 3cqh;
    container-type: size;
  }
  .chords {
    display: flex;
    align-items: center;
    gap: 5cqw;
    min-height: 0;
  }
  .both .chords {
    padding-bottom: 2cqh;
    border-bottom: 1px solid var(--line);
  }
  .now-chord {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1.5cqh;
  }
  .chord-now {
    font-family: var(--mono);
    font-weight: 700;
    line-height: 1;
    color: var(--accent);
    font-size: clamp(36px, min(42cqh, 20cqw), 220px);
  }
  .both .chord-now {
    font-size: clamp(30px, min(28cqh, 14cqw), 160px);
  }
  .beats {
    display: flex;
    gap: 6px;
  }
  .beats i {
    width: clamp(10px, 2.4cqw, 22px);
    height: clamp(6px, 1.4cqh, 10px);
    border-radius: 2px;
    background: var(--bg-active);
  }
  .beats i.on {
    background: var(--text-dim);
  }
  .beats i.on.one {
    background: var(--accent);
  }
  .next-chords {
    display: flex;
    align-items: baseline;
    gap: 3cqw;
    min-width: 0;
    flex-wrap: wrap;
  }
  .chord-next {
    font-family: var(--mono);
    font-weight: 600;
    line-height: 1;
    color: var(--text);
    font-size: clamp(20px, min(20cqh, 9cqw), 110px);
  }
  .both .chord-next {
    font-size: clamp(18px, min(13cqh, 7cqw), 80px);
  }
  .count {
    font-size: clamp(12px, min(5cqh, 2.4cqw), 22px);
    color: var(--text-faint);
  }
  .lyrics {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 1cqh;
    min-height: 0;
    flex: 1;
    justify-content: center;
  }
  .lyrics p {
    margin: 0;
  }
  .line-now {
    font-weight: 700;
    line-height: 1.2;
    color: var(--text);
    font-size: clamp(22px, min(18cqh, 6.4cqw), 96px);
  }
  .both .line-now {
    font-size: clamp(20px, min(12cqh, 5.2cqw), 72px);
  }
  .line-now.empty {
    color: var(--text-faint);
  }
  .line-next {
    line-height: 1.25;
    color: var(--text-dim);
    font-size: clamp(15px, min(10cqh, 4cqw), 56px);
  }
  .both .line-next {
    font-size: clamp(14px, min(7.5cqh, 3.4cqw), 42px);
  }
  .line-next.far {
    color: var(--text-faint);
  }
  .plain {
    flex: 1;
    margin: 0;
    min-height: 0;
    overflow-y: auto;
    font-family: var(--font);
    line-height: 1.45;
    white-space: pre-wrap;
    color: var(--text);
    font-size: clamp(14px, min(6cqh, 2.6cqw), 32px);
  }
  .credit {
    align-self: flex-end;
    font-size: 11px;
    color: var(--text-faint);
  }
  .status {
    font-size: var(--fs-body);
    color: var(--text-dim);
  }
  .link {
    margin-left: 8px;
    font-size: var(--fs-body);
    color: var(--accent);
    text-decoration: underline;
  }
  .editor {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  textarea {
    flex: 1;
    min-height: 60px;
    width: 100%;
    resize: none;
    padding: 8px;
    border-radius: var(--radius-s);
    background: var(--bg-panel);
    border: 1px solid var(--line-strong);
    color: var(--text);
    font-family: var(--mono);
    font-size: var(--fs-small);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .note {
    margin-right: auto;
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .actions button {
    height: 26px;
    padding: 0 12px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line-strong);
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .actions button.primary {
    color: var(--accent-ink);
    background: var(--accent);
    border-color: var(--accent);
  }
</style>
