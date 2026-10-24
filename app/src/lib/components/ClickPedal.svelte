<script lang="ts">
  import { app } from "../app.svelte";
  import { formatBpm, formatDb } from "../format";
  import { i18n } from "../i18n/index.svelte";
  import type { MessageKey } from "../i18n/en";
  import { barBeatAt } from "../timeline";
  import type { ClickSound } from "../types";
  import Knob from "./Knob.svelte";

  const SOUNDS: ClickSound[] = ["classic", "woodblock", "beep", "cowbell", "sticks", "drums"];
  const BARS = [0, 1, 2, 4];

  const project = $derived(app.song?.project);
  const analysis = $derived(project?.analysis);
  const m = $derived(project?.metronome);
  const beat = $derived(
    analysis && (app.transport.playing || app.transport.countingIn)
      ? (barBeatAt(analysis, app.transport.positionSecs)?.beat ?? 0)
      : 0,
  );
  const countInValue = $derived(m && m.countIn ? m.countInBars : 0);

  // Typing the tempo: click the BPM readout, type, Enter (Esc cancels).
  let editingBpm = $state(false);
  let bpmInput = $state("");

  function startBpmEdit(): void {
    if (!analysis) return;
    bpmInput = formatBpm(analysis.bpm);
    editingBpm = true;
  }

  function commitBpm(): void {
    editingBpm = false;
    const bpm = Number(bpmInput.replace(",", "."));
    if (!analysis || !Number.isFinite(bpm) || Math.abs(bpm - analysis.bpm) < 0.005) return;
    if (bpm < 20 || bpm > 400) return app.notify(i18n.t("tempo.bpmRange"));
    void app.editTempo({ kind: "setBpm", value: { bpm, anchor: app.transport.positionSecs } });
  }

  function focusSelect(node: HTMLInputElement): void {
    node.focus();
    node.select();
  }

  function setCountIn(bars: number): void {
    void app.updateMetronome(bars === 0 ? { countIn: false } : { countIn: true, countInBars: bars });
  }
</script>

{#if analysis && m}
  <section class="pedal" aria-label={i18n.t("click.title")}>
    <header>
      <span class="brand">{i18n.t("click.title")}</span>
      {#if analysis.variableTempo}<span class="tag">{i18n.t("tempo.variable")}</span>{/if}
      <button class="redetect" title={i18n.t("tempo.redetectHint")} disabled={app.analyzing} onclick={() => app.reanalyzeTempo("auto")}>
        {app.analyzing ? "…" : "↻"}
      </button>
    </header>

    <div class="leds" aria-hidden="true">
      {#each Array.from({ length: analysis.beatsPerBar }, (_, i) => i + 1) as b (b)}
        <i class:on={beat === b} class:one={b === 1}></i>
      {/each}
    </div>

    <div class="display">
      <button class="mini" title={i18n.t("tempo.half")} onclick={() => app.editTempo({ kind: "half" })}>÷2</button>
      {#if editingBpm}
        <input
          class="bpm-input tabular"
          inputmode="decimal"
          aria-label={i18n.t("tempo.typeBpm")}
          bind:value={bpmInput}
          use:focusSelect
          onkeydown={(e) => {
            if (e.key === "Enter") commitBpm();
            else if (e.key === "Escape") editingBpm = false;
          }}
          onblur={commitBpm}
        />
      {:else}
        <button class="bpm tabular" title={i18n.t("tempo.typeBpm")} onclick={startBpmEdit}>
          {formatBpm(analysis.bpm)}<small>BPM</small>
        </button>
      {/if}
      <button class="mini" title={i18n.t("tempo.double")} onclick={() => app.editTempo({ kind: "double" })}>×2</button>
    </div>
    {#if app.typedBpmReminder}
      <div class="typed" title={i18n.t("tempo.typedHint")}>
        <span>{i18n.t("tempo.typed", { bpm: formatBpm(app.typedBpmReminder) })}</span>
        <button class="link" onclick={() => app.useTypedBpm()}>{i18n.t("tempo.useTyped")}</button>
        <button class="forget" title={i18n.t("tempo.forgetTyped")} aria-label={i18n.t("tempo.forgetTyped")} onclick={() => app.forgetTypedBpm()}>×</button>
      </div>
    {/if}

    <div class="row mode-row">
      <div class="segmented grow" role="radiogroup" aria-label={i18n.t("tempo.mode")} title={i18n.t("tempo.modeHint")}>
        <button
          role="radio"
          aria-checked={!analysis.variableTempo}
          class:on={!analysis.variableTempo}
          disabled={app.analyzing}
          onclick={() => analysis.variableTempo && app.reanalyzeTempo("constant")}>{i18n.t("tempo.constant")}</button
        >
        <button
          role="radio"
          aria-checked={analysis.variableTempo}
          class:on={analysis.variableTempo}
          disabled={app.analyzing}
          onclick={() => !analysis.variableTempo && app.reanalyzeTempo("variable")}>{i18n.t("tempo.variableShort")}</button
        >
      </div>
      <button class="tap" class:active={app.taps.length > 0} title={i18n.t("tempo.tapHint")} onclick={() => app.tap()}>
        {i18n.t("tempo.tap")}{#if app.taps.length > 0}<small>{app.taps.length}</small>{/if}
      </button>
    </div>

    <div class="nudge">
      <button class="mini" title={i18n.t("tempo.earlier")} onclick={() => app.editTempo({ kind: "nudge", value: -0.01 })}>◀</button>
      <button
        class="one-here"
        title={i18n.t("tempo.downbeatHint")}
        onclick={() => app.editTempo({ kind: "downbeatAt", value: app.transport.positionSecs })}
      >
        {i18n.t("tempo.downbeat")}
      </button>
      <button class="mini" title={i18n.t("tempo.later")} onclick={() => app.editTempo({ kind: "nudge", value: 0.01 })}>▶</button>
    </div>

    <div class="row">
      <label class="field">
        <span>{i18n.t("click.sound")}</span>
        <select value={m.sound} onchange={(e) => app.updateMetronome({ sound: e.currentTarget.value as ClickSound })}>
          {#each SOUNDS as s (s)}
            <option value={s}>{i18n.t(`click.sound.${s}` as MessageKey)}</option>
          {/each}
        </select>
      </label>
      <label class="field sig">
        <span title={i18n.t("tempo.signature")}>{i18n.t("song.bar")}</span>
        <select
          value={analysis.beatsPerBar}
          onchange={(e) => app.editTempo({ kind: "beatsPerBar", value: Number(e.currentTarget.value) })}
        >
          {#each [2, 3, 4, 5, 6, 7] as n (n)}
            <option value={n}>{n}/4</option>
          {/each}
        </select>
      </label>
    </div>

    <div class="field">
      <span title={i18n.t("click.barsHint")}>{i18n.t("transport.countIn")}</span>
      <div class="segmented" role="radiogroup" aria-label={i18n.t("click.barsHint")}>
        {#each BARS as b (b)}
          <button role="radio" aria-checked={countInValue === b} class:on={countInValue === b} onclick={() => setCountIn(b)}>
            {b === 0 ? i18n.t("click.off") : b}
          </button>
        {/each}
      </div>
    </div>

    <footer>
      <Knob
        value={m.gainDb}
        min={-40}
        max={6}
        reset={-6}
        size={40}
        color="var(--stem-click)"
        label={`${i18n.t("click.title")} · ${i18n.t("track.volume")}`}
        display={`${formatDb(m.gainDb)} dB`}
        onchange={(db) => app.updateMetronome({ gainDb: Math.round(db * 10) / 10 })}
      />
      <button
        class="switch"
        class:on={!m.muted}
        aria-pressed={!m.muted}
        title={i18n.t("track.mute")}
        onclick={() => app.updateMetronome({ muted: !m.muted })}
      >
        <span class="lamp"></span>
      </button>
    </footer>
  </section>
{/if}

<style>
  .pedal {
    width: 248px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border-radius: var(--radius-s);
    background: var(--bg-panel);
    border: 1px solid var(--line);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .brand {
    font-size: var(--fs-label);
    font-weight: 600;
    color: var(--accent);
  }
  .redetect {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .redetect:hover {
    color: var(--text);
  }
  .tag {
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .leds {
    display: flex;
    justify-content: center;
    gap: 10px;
  }
  .leds i {
    width: 14px;
    height: 6px;
    border-radius: 0;
    background: var(--line-strong);
  }
  .leds i.on {
    background: var(--ok);
  }
  .leds i.on.one {
    background: var(--accent);
  }
  .display {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 8px;
    border-radius: var(--radius-s);
    background: var(--bg);
    border: 1px solid var(--line);
  }
  .bpm {
    font-family: var(--mono);
    font-size: 30px;
    font-weight: 500;
    line-height: 1;
    color: var(--accent);
    display: flex;
    align-items: baseline;
    gap: 4px;
  }
  .bpm {
    cursor: text;
    border-radius: var(--radius-s);
    padding: 0 4px;
  }
  .bpm:hover {
    background: var(--bg-hover);
  }
  .bpm-input {
    width: 110px;
    height: 40px;
    border: 1px solid var(--accent);
    border-radius: var(--radius-s);
    background: var(--bg);
    color: var(--accent);
    font-family: var(--mono);
    font-size: 26px;
    font-weight: 500;
    text-align: center;
    user-select: text;
  }
  .bpm small {
    font-size: var(--fs-label);
    color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .typed {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: -2px;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .typed span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .typed .link {
    color: var(--accent);
    text-decoration: underline;
  }
  .typed .forget {
    color: var(--text-faint);
  }
  .typed .forget:hover {
    color: var(--text);
  }
  .mini {
    min-width: 30px;
    height: 26px;
    padding: 0 6px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line-strong);
    color: var(--text-dim);
    font-size: var(--fs-small);
  }
  .mini:hover,
  .one-here:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .nudge {
    display: flex;
    gap: 6px;
  }
  .one-here {
    flex: 1;
    height: 26px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line-strong);
    color: var(--text-dim);
    font-size: var(--fs-small);
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }
  .field.sig {
    flex: 0 0 76px;
  }
  .field > span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--fs-label);
    color: var(--text-faint);
  }
  select {
    height: 28px;
    padding: 0 6px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line-strong);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-small);
    min-width: 0;
  }
  .segmented {
    display: flex;
    padding: 1px;
    border-radius: var(--radius-s);
    background: var(--bg);
    border: 1px solid var(--line-strong);
  }
  .segmented button {
    flex: 1;
    min-width: 0;
    padding: 0 6px;
    white-space: nowrap;
    height: 24px;
    border-radius: 1px;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  /* Constant / variable: size to the words ("Follow band" is longer). */
  .grow button {
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .segmented button:disabled {
    opacity: 0.6;
  }
  .tap {
    position: relative;
    flex: none;
    width: 52px;
    border-radius: var(--radius-s);
    font-size: var(--fs-small);
    color: var(--accent);
    border: 1px solid var(--accent);
  }
  .tap:active,
  .tap.active {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .tap small {
    position: absolute;
    top: -6px;
    right: -6px;
    min-width: 16px;
    height: 16px;
    border-radius: var(--radius-s);
    background: var(--ok);
    color: var(--accent-ink);
    font-size: 11px; /* tap counter inside a 16 px badge */
    line-height: 16px;
  }
  .segmented button.on {
    background: var(--accent);
    color: var(--accent-ink);
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: auto;
    padding-top: 4px;
  }
  .switch {
    width: 44px;
    height: 36px;
    border-radius: var(--radius-s);
    display: grid;
    place-items: center;
    border: 1px solid var(--line-strong);
  }
  .switch:hover {
    background: var(--bg-hover);
  }
  .switch.on {
    border-color: var(--accent);
  }
  .lamp {
    width: 14px;
    height: 6px;
    background: var(--line-strong);
  }
  .switch.on .lamp {
    background: var(--accent);
  }
  @media (max-width: 1180px) {
    .pedal {
      padding: 10px 12px;
    }
  }
  @media (max-height: 760px) {
    .pedal {
      gap: 6px;
      padding-top: 8px;
      padding-bottom: 8px;
    }
    .switch {
      width: 40px;
      height: 32px;
    }
  }
</style>
