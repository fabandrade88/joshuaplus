<script lang="ts">
  // Console strip for the connected e-drum kit (MIDI). It shows hits, not
  // audio: the meter follows strike velocity and the last pad played. When the
  // module sends no audio over USB, the strip says so and links to Inputs.
  import { app } from "../app.svelte";
  import type { MessageKey } from "../i18n/en";
  import { i18n } from "../i18n/index.svelte";
  import LedMeter from "./LedMeter.svelte";
  import StemIcon from "./StemIcon.svelte";

  const DECAY_MS = 450;

  let level = $state(0);

  $effect(() => {
    const hit = app.lastNote;
    if (!hit) return;
    let raf = 0;
    const tick = () => {
      const t = (performance.now() - hit.at) / DECAY_MS;
      level = t >= 1 ? 0 : (hit.velocity / 127) * (1 - t);
      if (t < 1) raf = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(raf);
  });

  const padLabel = $derived(
    app.lastNote?.pad ? i18n.t(`pad.${app.lastNote.pad}` as MessageKey) : app.lastNote ? `#${app.lastNote.note}` : "—",
  );
</script>

<div class="strip" class:idle={level === 0}>
  <div class="cap"></div>
  <div class="head">
    <span class="icon"><StemIcon kind="drums" size={17} /></span>
    <span class="name" title={app.drums?.connected ?? ""}>{app.drums?.connected ?? ""}</span>
    <span class="tag">MIDI</span>
  </div>
  <div class="meter-zone">
    <LedMeter {level} />
  </div>
  <span class="pad" title={i18n.t("console.lastPad")}>{padLabel}</span>
  {#if app.drumsMidiOnly}
    <button class="warn" title={i18n.t("gear.midiOnly", { name: app.drums?.connected ?? "" })} onclick={() => app.openGear()}>
      {i18n.t("console.noAudio")}
    </button>
  {/if}
</div>

<style>
  .strip {
    position: relative;
    width: 76px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 12px 6px 10px;
    border-radius: 12px;
    background: linear-gradient(180deg, #171b26, #11141c);
    border: 1px solid #222838;
    box-shadow: inset 0 1px 0 rgb(255 255 255 / 4%);
  }
  .cap {
    position: absolute;
    inset: 0 10px auto;
    height: 3px;
    border-radius: 0 0 3px 3px;
    background: var(--stem-drums);
    box-shadow: 0 0 10px var(--stem-drums);
  }
  .idle .cap {
    opacity: 0.5;
    box-shadow: none;
  }
  .head {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    width: 100%;
  }
  .icon {
    color: var(--stem-drums);
  }
  .name {
    max-width: 100%;
    font-size: 11.5px;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    font-family: var(--mono);
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.08em;
    padding: 1px 6px;
    border-radius: 999px;
    color: var(--stem-drums);
    background: color-mix(in srgb, var(--stem-drums) 14%, transparent);
  }
  .meter-zone {
    flex: 1;
    min-height: 120px;
    display: flex;
    justify-content: center;
  }
  .pad {
    max-width: 100%;
    font-size: 10.5px;
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .warn {
    width: 100%;
    padding: 4px 2px;
    border-radius: 6px;
    font-size: 10px;
    font-weight: 700;
    line-height: 1.2;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .warn:hover {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
</style>
