<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { app } from "../app.svelte";
  import { formatDb } from "../format";
  import { i18n } from "../i18n/index.svelte";
  import type { MessageKey } from "../i18n/en";
  import type { DrumPad, InputDeviceView, InputView } from "../types";
  import Icon from "./Icon.svelte";
  import Knob from "./Knob.svelte";
  import LedMeter from "./LedMeter.svelte";

  // Kit laid out roughly as a drummer sees it.
  const KIT: DrumPad[][] = [
    ["crash1", "tom1", "tom2", "crash2", "ride", "rideBell"],
    ["hiHatOpen", "hiHatClosed", "snare", "snareRim", "tom3", "tom4"],
    ["hiHatPedal", "kick", "snareCrossStick", "splash", "china", "aux"],
  ];
  const LIGHT_MS = 220;

  let devices = $state<InputDeviceView[]>([]);
  let selected = $state<DrumPad | null>(null);
  let now = $state(performance.now());

  async function refreshDevices(): Promise<void> {
    devices = await api.listAudioInputs();
  }

  onMount(() => {
    void refreshDevices();
    let raf = 0;
    const loop = () => {
      now = performance.now();
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  });

  const drums = $derived(app.drums);
  const brand = $derived(drums?.brands.find((b) => b.id === drums.brand));
  const notesByPad = $derived.by(() => {
    const out: Partial<Record<DrumPad, number[]>> = {};
    for (const [note, pad] of Object.entries(drums?.map.notes ?? {})) (out[pad] ??= []).push(Number(note));
    return out;
  });
  const outRate = $derived(app.info?.audio?.sampleRate ?? 0);

  function routingOptions(input: InputView): { value: string; label: string }[] {
    const n = Math.max(1, input.channels);
    const opts = Array.from({ length: n }, (_, c) => ({
      value: `m${c}`,
      label: `${i18n.t("gear.channel")} ${c + 1}`,
    }));
    for (let c = 0; c + 1 < n; c += 2) {
      opts.push({ value: `s${c}`, label: i18n.t("gear.pair", { a: c + 1, b: c + 2 }) });
    }
    return opts;
  }

  function setRouting(input: InputView, value: string): void {
    const stereo = value.startsWith("s");
    void app.updateInput(input.index, { stereo, firstChannel: Number(value.slice(1)) });
  }

  function padLight(pad: DrumPad): number {
    const h = app.padHits[pad];
    if (!h) return 0;
    const age = now - h.at;
    return age > LIGHT_MS ? 0 : (1 - age / LIGHT_MS) * (0.35 + 0.65 * (h.velocity / 127));
  }
</script>

<div class="gear">
  <header class="head">
    <div class="head-text">
      <h1>{i18n.t("gear.title")}</h1>
      <p>{i18n.t("gear.subtitle")}</p>
    </div>
    <div class="summary">
      <span class="pill" style="--c: var(--ok)"><Icon name="volume" size={12} />{app.inputs.length} {i18n.t("gear.inputs")}</span>
      <span class="pill" class:dim={!drums?.connected} style="--c: var(--stem-drums)">
        <Icon name="click" size={12} /><span class="pill-text">{drums?.connected ?? i18n.t("gear.noPort")}</span>
      </span>
    </div>
  </header>

  <section class="panel" style="--c: var(--ok)" aria-labelledby="gear-audio">
    <div class="panel-head">
      <span class="badge"><Icon name="volume" size={15} /></span>
      <div class="titles">
        <h2 id="gear-audio">{i18n.t("gear.audio")}</h2>
        <p class="hint">{i18n.t("gear.audioHint")}</p>
      </div>
      <select
        class="add"
        value=""
        aria-label={i18n.t("gear.addInput")}
        onfocus={refreshDevices}
        onchange={(e) => {
          const id = e.currentTarget.value;
          e.currentTarget.value = "";
          if (id) void app.addInput(id);
        }}
      >
        <option value="">{i18n.t("gear.addInput")}</option>
        {#each devices as d (d.id)}
          <option value={d.id}>{d.name} · {d.channels} ch · {d.sampleRate / 1000} kHz</option>
        {/each}
      </select>
    </div>

    {#if app.inputs.length === 0}
      <p class="empty">{i18n.t("gear.noInputs")}</p>
    {:else}
      <div class="inputs">
        {#each app.inputs as input (input.index)}
          <div class="input" class:off={!!input.error} class:live={input.monitor && input.monitorOk}>
            <div class="meter"><LedMeter level={app.inputLevels[input.index] ?? 0} segments={10} /></div>
            <div class="input-main">
              <div class="input-title">
                <Icon name="volume" size={13} />
                <span class="name" title={input.name}>{input.name}</span>
                {#if input.sampleRate}<span class="meta tabular">{input.channels} ch · {input.sampleRate / 1000} kHz</span>{/if}
                <button
                  class="icon-btn remove"
                  title={i18n.t("gear.remove")}
                  aria-label={`${i18n.t("gear.remove")}: ${input.name}`}
                  onclick={() => app.removeInput(input.index)}
                >
                  <Icon name="trash" size={13} />
                </button>
              </div>
              {#if input.error}
                <span class="tag warn">{i18n.t("gear.disconnected")}</span>
              {:else}
                <div class="input-controls">
                  <select
                    value={`${input.stereo ? "s" : "m"}${input.firstChannel}`}
                    aria-label={`${i18n.t("gear.channel")} · ${input.name}`}
                    onchange={(e) => setRouting(input, e.currentTarget.value)}
                  >
                    {#each routingOptions(input) as o (o.value)}
                      <option value={o.value}>{o.label}</option>
                    {/each}
                  </select>
                  {#if input.directMonitor}
                    <span class="toggle direct" title={i18n.t("gear.directMonitorHint")}>{i18n.t("gear.directMonitor")}</span>
                  {:else}
                    <label class="toggle" class:disabled={!input.monitorOk}>
                      <input
                        type="checkbox"
                        checked={input.monitor}
                        disabled={!input.monitorOk}
                        onchange={(e) => app.updateInput(input.index, { monitor: e.currentTarget.checked })}
                      />
                      {i18n.t("gear.monitor")}
                    </label>
                  {/if}
                  <span class="knob">
                    <Knob
                      value={input.gainDb}
                      min={-40}
                      max={12}
                      reset={0}
                      size={26}
                      label={i18n.t("track.volume")}
                      display={`${formatDb(input.gainDb)} dB`}
                      onchange={(db) => app.updateInput(input.index, { gainDb: Math.round(db * 10) / 10 })}
                    />
                  </span>
                </div>
                {#if input.directMonitor}
                  <span class="note-text">{i18n.t("gear.directMonitorHint")}</span>
                {:else if !input.monitorOk}
                  <span class="note-text">{i18n.t("gear.monitorUnavailable", { out: outRate })}</span>
                {/if}
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>

  <section class="panel" style="--c: var(--stem-drums)" aria-labelledby="gear-drums">
    <div class="panel-head">
      <span class="badge"><Icon name="click" size={15} /></span>
      <div class="titles">
        <h2 id="gear-drums">{i18n.t("gear.drums")}</h2>
        <p class="hint">{i18n.t("gear.drumsHint")}</p>
      </div>
      <button class="btn small" onclick={() => app.refreshDrums()}>{i18n.t("gear.refresh")}</button>
    </div>

    {#if drums}
      <div class="row">
        <label class="field">
          <span>{i18n.t("gear.midiPort")}</span>
          <select value={drums.portId ?? ""} onchange={(e) => app.setMidiPort(e.currentTarget.value || null)}>
            <option value="">{i18n.t("gear.noPort")}</option>
            {#each drums.ports as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          <span>{i18n.t("gear.brand")}</span>
          <select value={drums.brand} onchange={(e) => app.setDrumBrand(e.currentTarget.value)}>
            {#each drums.brands as b (b.id)}
              <option value={b.id}>{b.name}</option>
            {/each}
          </select>
        </label>
        <div class="status">
          {#if drums.connected}<span class="tag ok">● {i18n.t("gear.connected", { name: drums.connected })}</span>{/if}
          {#if drums.error}<span class="tag warn">{drums.error}</span>{/if}
          {#if brand}<span class="models" title={brand.models}>{brand.models}</span>{/if}
        </div>
      </div>
      {#if app.drumsMidiOnly}
        <p class="tip alert"><Icon name="volume" size={12} />{i18n.t("gear.midiOnly", { name: drums.connected ?? "" })}</p>
      {:else if brand?.usbAudio}<p class="tip"><Icon name="volume" size={12} />{i18n.t("gear.usbAudio")}</p>{/if}

      <div class="kit-wrap">
        <div class="kit-head">
          <h3>{i18n.t("gear.kit")}</h3>
          <p class="hint">{i18n.t("gear.kitHint")}</p>
          {#if app.lastNote}
            <span class="note tabular">{i18n.t("gear.lastNote", { note: app.lastNote.note, vel: app.lastNote.velocity })}</span>
          {/if}
        </div>

        <div class="kit">
          {#each KIT as row, r (r)}
            {#each row as pad (pad)}
              {@const light = padLight(pad)}
              <button
                class="pad"
                class:sel={selected === pad}
                class:learning={drums.learning === pad}
                class:cymbal={pad.startsWith("crash") || pad.startsWith("ride") || pad === "splash" || pad === "china" || pad.startsWith("hiHat")}
                aria-pressed={selected === pad}
                title={i18n.t(`pad.${pad}` as MessageKey)}
                style={`--light:${light}`}
                onclick={() => (selected = selected === pad ? null : pad)}
              >
                <span class="pad-name">{i18n.t(`pad.${pad}` as MessageKey)}</span>
                <span class="pad-notes tabular">{(notesByPad[pad] ?? []).join(" · ") || "—"}</span>
              </button>
            {/each}
          {/each}
        </div>

        <div class="learn-bar">
          <div class="hh">
            <span>{i18n.t("gear.hihat")}</span>
            <div class="hh-bar" role="meter" aria-label={i18n.t("gear.hihat")} aria-valuemin={0} aria-valuemax={127} aria-valuenow={app.hiHatOpenness}>
              <div style={`width:${(app.hiHatOpenness / 127) * 100}%`}></div>
            </div>
          </div>
          {#if drums.learning}
            <span class="learning-msg" role="status">{i18n.t("gear.learning", { pad: i18n.t(`pad.${drums.learning}` as MessageKey) })}</span>
            <button class="btn small" onclick={() => app.drumLearn(null)}>{i18n.t("gear.cancel")}</button>
          {:else}
            <button class="btn small primary" disabled={!selected || !drums.connected} onclick={() => app.drumLearn(selected)}>
              {i18n.t("gear.learn")}{selected ? `: ${i18n.t(`pad.${selected}` as MessageKey)}` : ""}
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </section>
</div>

<style>
  .gear {
    flex: 1;
    overflow-y: auto;
    padding: 16px 24px 28px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .gear > * {
    flex: none;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    flex-wrap: wrap;
    padding: 10px 16px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 7%, var(--bg-raised));
    border: 1px solid color-mix(in srgb, var(--accent) 30%, var(--line));
  }
  .head-text {
    min-width: 0;
    flex: 1 1 320px;
  }
  .head h1 {
    margin: 0;
    font-size: 18px;
    letter-spacing: -0.02em;
    color: var(--accent);
  }
  .head p {
    margin: 2px 0 0;
    color: var(--text-dim);
    font-size: 12px;
  }
  .summary {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    min-width: 0;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 260px;
    padding: 3px 9px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 700;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--c) 30%, transparent);
  }
  .pill-text {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pill.dim {
    filter: grayscale(1);
    opacity: 0.6;
  }
  .hint {
    margin: 0;
    color: var(--text-faint);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .panel {
    position: relative;
    overflow: hidden;
    padding: 12px 14px 12px 18px;
    border-radius: var(--radius);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .panel::before {
    content: "";
    position: absolute;
    inset: 0 auto 0 0;
    width: 3px;
    background: var(--c);
  }
  .panel-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  .panel-head h2 {
    margin: 0;
    font-size: 14px;
  }
  .badge {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 16%, transparent);
  }
  h3 {
    margin: 0;
    font-size: 12px;
    white-space: nowrap;
    color: color-mix(in srgb, var(--c) 65%, var(--text));
  }
  select {
    height: 28px;
    min-width: 0;
    padding: 0 8px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line-strong);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
  }
  select:focus {
    border-color: var(--c, var(--accent));
  }
  .add {
    flex: 0 1 220px;
    border-color: color-mix(in srgb, var(--c) 45%, var(--line-strong));
    color: var(--c);
    font-weight: 600;
  }
  .empty {
    margin: 0;
    padding: 10px;
    border-radius: var(--radius-s);
    border: 1.5px dashed var(--line-strong);
    color: var(--text-faint);
    font-size: 12.5px;
    text-align: center;
  }
  .inputs {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 8px;
  }
  .input {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px 8px 8px 10px;
    border-radius: var(--radius-s);
    background: var(--bg-panel);
    border: 1px solid var(--line);
    transition: border-color 0.15s;
  }
  .input.live {
    border-color: color-mix(in srgb, var(--ok) 45%, var(--line));
  }
  .input.off {
    opacity: 0.6;
  }
  .meter {
    height: 52px;
    flex: none;
  }
  .input-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .input-title {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--ok);
  }
  .name {
    min-width: 0;
    color: var(--text);
    font-size: 12.5px;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    margin-left: auto;
    font-size: 10px;
    font-family: var(--mono);
    color: var(--text-faint);
    white-space: nowrap;
  }
  .remove {
    flex: none;
    width: 24px;
    height: 24px;
  }
  .input-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .input-controls select {
    flex: 0 1 120px;
  }
  .knob {
    margin-left: auto;
    display: flex;
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 9px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-dim);
    background: var(--bg-active);
    white-space: nowrap;
    cursor: pointer;
  }
  .toggle.direct {
    cursor: help;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .toggle:has(input:checked) {
    color: var(--ok);
    background: color-mix(in srgb, var(--ok) 15%, transparent);
  }
  .toggle input {
    accent-color: var(--ok);
    margin: 0;
  }
  .toggle.disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .note-text {
    font-size: 10.5px;
    line-height: 1.35;
    color: var(--text-faint);
  }
  .tag {
    align-self: flex-start;
    padding: 2px 8px;
    border-radius: 6px;
    font-size: 11px;
    white-space: nowrap;
    color: var(--text-dim);
    background: var(--bg-active);
  }
  .tag.ok {
    color: var(--ok);
    background: color-mix(in srgb, var(--ok) 14%, transparent);
    font-weight: 700;
  }
  .tag.warn {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    white-space: normal;
  }
  .tip {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0;
    padding: 6px 10px;
    border-radius: var(--radius-s);
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--ok);
    background: color-mix(in srgb, var(--ok) 10%, transparent);
  }
  .tip :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .tip.alert {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .note {
    flex: none;
    font-size: 10.5px;
    color: var(--text-faint);
    margin: 0;
    font-family: var(--mono);
    white-space: nowrap;
  }
  .row {
    display: flex;
    align-items: flex-end;
    gap: 8px 10px;
    flex-wrap: wrap;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 0 1 220px;
    min-width: 160px;
  }
  .field span {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: color-mix(in srgb, var(--c) 70%, var(--text-faint));
  }
  .status {
    flex: 1 1 200px;
    min-width: 0;
    display: flex;
    gap: 4px 8px;
    flex-wrap: wrap;
    align-items: center;
    min-height: 28px;
  }
  .models {
    min-width: 0;
    font-size: 11px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .kit-wrap {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }
  .kit-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }
  .kit-head .hint {
    flex: 1;
    min-width: 0;
  }
  .kit {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 4px;
    max-width: 860px;
    padding: 6px;
    border-radius: var(--radius-s);
    background: var(--bg);
    border: 1px solid var(--line);
  }
  .pad {
    position: relative;
    min-width: 0;
    height: 36px;
    padding: 0 6px;
    border-radius: 7px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0;
    line-height: 1.2;
    background: color-mix(in srgb, var(--stem-drums) calc(var(--light) * 75%), var(--bg-panel));
    border: 1px solid var(--line-strong);
    box-shadow: 0 0 calc(var(--light) * 16px) color-mix(in srgb, var(--stem-drums) calc(var(--light) * 100%), transparent);
    transition: border-color 0.12s;
  }
  .pad:hover {
    border-color: var(--text-faint);
  }
  .pad.cymbal {
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent-strong) calc(var(--light) * 75%), #1a1914);
    border-color: color-mix(in srgb, var(--accent) 25%, var(--line-strong));
  }
  .pad.sel {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .pad.learning {
    animation: pulse 0.6s ease-in-out infinite alternate;
    outline: 2px solid var(--accent);
  }
  @keyframes pulse {
    to {
      opacity: 0.55;
    }
  }
  .pad-name {
    max-width: 100%;
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pad-notes {
    font-size: 9.5px;
    font-family: var(--mono);
    color: var(--text-faint);
  }
  .learn-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 860px;
  }
  .hh {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11.5px;
    color: var(--text-dim);
    margin-right: auto;
    white-space: nowrap;
  }
  .hh-bar {
    width: 120px;
    height: 5px;
    border-radius: 3px;
    background: var(--bg);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .hh-bar div {
    height: 100%;
    background: var(--stem-drums);
  }
  .learning-msg {
    font-size: 12px;
    color: var(--accent);
    font-weight: 700;
  }
  .small {
    flex: none;
    height: 28px;
    padding: 0 12px;
    font-size: 12px;
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  @media (max-width: 1100px) {
    .gear {
      padding: 12px 16px 20px;
    }
    .kit-head {
      flex-wrap: wrap;
    }
    .kit-head .hint {
      order: 3;
      flex-basis: 100%;
    }
  }
</style>
