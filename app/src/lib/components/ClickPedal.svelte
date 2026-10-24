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
    width: 232px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border-radius: 16px;
    background:
      linear-gradient(160deg, rgb(255 255 255 / 6%), transparent 40%),
      linear-gradient(180deg, #2a2418, #1a160f);
    border: 1px solid #4a3d22;
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 8%),
      0 10px 30px rgb(0 0 0 / 40%);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .brand {
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--accent);
  }
  .redetect {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    background: #322a19;
    color: var(--text-dim);
    font-weight: 800;
  }
  .redetect:hover {
    color: var(--text);
  }
  .tag {
    font-size: 10px;
    color: var(--text-faint);
  }
  .leds {
    display: flex;
    justify-content: center;
    gap: 10px;
  }
  .leds i {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #3a301c;
    box-shadow: inset 0 1px 2px rgb(0 0 0 / 60%);
  }
  .leds i.on {
    background: var(--ok);
    box-shadow: 0 0 10px var(--ok);
  }
  .leds i.on.one {
    background: var(--accent);
    box-shadow: 0 0 12px var(--accent);
  }
  .display {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 8px;
    border-radius: 10px;
    background: #0b0c08;
    box-shadow: inset 0 2px 6px rgb(0 0 0 / 80%);
  }
  .bpm {
    font-family: var(--mono);
    font-size: 34px;
    font-weight: 700;
    line-height: 1;
    color: var(--accent);
    text-shadow: 0 0 12px color-mix(in srgb, var(--accent) 50%, transparent);
    display: flex;
    align-items: baseline;
    gap: 4px;
  }
  .bpm {
    cursor: text;
    border-radius: 6px;
    padding: 0 4px;
  }
  .bpm:hover {
    background: rgb(255 176 32 / 8%);
  }
  .bpm-input {
    width: 110px;
    height: 40px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: #000;
    color: var(--accent);
    font-family: var(--mono);
    font-size: 30px;
    font-weight: 700;
    text-align: center;
    user-select: text;
  }
  .bpm small {
    font-size: 10px;
    color: color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .mini {
    min-width: 30px;
    height: 26px;
    padding: 0 6px;
    border-radius: 6px;
    background: #322a19;
    color: var(--text-dim);
    font-size: 11.5px;
    font-weight: 800;
  }
  .mini:hover,
  .one-here:hover {
    color: var(--text);
    background: #3d3420;
  }
  .nudge {
    display: flex;
    gap: 6px;
  }
  .one-here {
    flex: 1;
    height: 26px;
    border-radius: 6px;
    background: #322a19;
    color: var(--text-dim);
    font-size: 11.5px;
    font-weight: 700;
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
    flex: 0 0 64px;
  }
  .field > span {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: #8f7f5c;
  }
  select {
    height: 28px;
    padding: 0 6px;
    border-radius: 6px;
    border: 1px solid #4a3d22;
    background: #15120c;
    color: var(--text);
    font: inherit;
    font-size: 12px;
    min-width: 0;
  }
  .segmented {
    display: flex;
    padding: 2px;
    border-radius: 8px;
    background: #15120c;
    border: 1px solid #4a3d22;
  }
  .segmented button {
    flex: 1;
    height: 24px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 700;
    color: var(--text-dim);
  }
  .grow {
    flex: 1;
  }
  .segmented button:disabled {
    opacity: 0.6;
  }
  .tap {
    position: relative;
    width: 58px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 900;
    letter-spacing: 0.08em;
    color: var(--accent);
    background: #322a19;
    border: 1px solid #5a4824;
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
    border-radius: 8px;
    background: var(--ok);
    color: #04140c;
    font-size: 10px;
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
    width: 54px;
    height: 54px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: radial-gradient(circle at 35% 30%, #d9dce4, #7d8293 60%, #4b4f5c);
    box-shadow:
      0 4px 10px rgb(0 0 0 / 50%),
      inset 0 -3px 6px rgb(0 0 0 / 35%);
  }
  .switch:active {
    transform: translateY(1px) scale(0.98);
  }
  .lamp {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #3b1a1a;
  }
  .switch.on .lamp {
    background: #ff4d4d;
    box-shadow: 0 0 10px #ff4d4d;
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
      width: 44px;
      height: 44px;
    }
  }
</style>
