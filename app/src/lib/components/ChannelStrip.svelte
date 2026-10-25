<script lang="ts">
  import { formatDb } from "../format";
  import { i18n } from "../i18n/index.svelte";
  import type { StemKind } from "../types";
  import Knob from "./Knob.svelte";
  import LedMeter from "./LedMeter.svelte";
  import StemIcon from "./StemIcon.svelte";
  import VFader from "./VFader.svelte";

  type Patch = { gainDb?: number; muted?: boolean; solo?: boolean; pan?: number; room?: number; tone?: number; body?: number };

  let {
    kind,
    name,
    color,
    gainDb,
    muted,
    solo = undefined,
    pan = undefined,
    room = undefined,
    tone = undefined,
    body = undefined,
    level = 0,
    silenced = false,
    wide = false,
    muteable = true,
    armed = undefined,
    onarm = undefined,
    ondelete = undefined,
    onchange,
  }: {
    kind: StemKind | "master" | "song" | "take";
    name: string;
    color: string;
    gainDb: number;
    muted: boolean;
    solo?: boolean | undefined;
    pan?: number | undefined;
    /** Room send `0..1` (recorded takes); shows a "Room" knob. */
    room?: number | undefined;
    /** Tone match `0..1` (takes with a match); shows a "Tone" knob. */
    tone?: number | undefined;
    /** Low-end body `0..1` (takes); shows a "Body" knob. */
    body?: number | undefined;
    level?: number;
    /** Muted by another channel's solo. */
    silenced?: boolean;
    wide?: boolean;
    muteable?: boolean;
    /** Record-arm state (inputs only); shows an R button. */
    armed?: boolean | undefined;
    onarm?: (() => void) | undefined;
    /** Shows a delete button in the header (takes). */
    ondelete?: (() => void) | undefined;
    onchange: (patch: Patch) => void;
  } = $props();

  const panLabel = $derived(
    pan === undefined || Math.abs(pan) < 0.01 ? "C" : pan < 0 ? `L${Math.round(-pan * 100)}` : `R${Math.round(pan * 100)}`,
  );
</script>

<div class="strip" class:wide class:off={muted || silenced} style={`--c:${color}`}>
  <div class="cap"></div>
  <div class="head">
    <span class="icon"><StemIcon {kind} size={17} /></span>
    <span class="name" title={name}>{name}</span>
    {#if ondelete}
      <button class="del" title={i18n.t("record.deleteTake")} aria-label={i18n.t("record.deleteTake")} onclick={() => ondelete?.()}>×</button>
    {/if}
  </div>
  <div class="pan">
    {#if pan !== undefined}
      <Knob
        value={pan}
        min={-1}
        max={1}
        reset={0}
        bipolar
        size={34}
        {color}
        label={`${name} · ${i18n.t("track.pan")}`}
        display={panLabel}
        onchange={(v) => onchange({ pan: Math.abs(v) < 0.02 ? 0 : v })}
      />
      <span class="readout">{panLabel}</span>
    {/if}
  </div>
  {#if room !== undefined}
    <div class="room" title={i18n.t("track.roomHint")}>
      <Knob
        value={room}
        min={0}
        max={1}
        reset={0.3}
        size={30}
        color="var(--accent)"
        label={`${name} · ${i18n.t("track.room")}`}
        display={`${Math.round(room * 100)}%`}
        onchange={(v) => onchange({ room: Math.round(v * 100) / 100 })}
      />
      <span class="readout">{i18n.t("track.room")} {Math.round(room * 100)}%</span>
    </div>
  {/if}
  {#if tone !== undefined}
    <div class="room" title={i18n.t("track.toneHint")}>
      <Knob
        value={tone}
        min={0}
        max={1}
        reset={1}
        size={30}
        color="var(--ok)"
        label={`${name} · ${i18n.t("track.tone")}`}
        display={`${Math.round(tone * 100)}%`}
        onchange={(v) => onchange({ tone: Math.round(v * 100) / 100 })}
      />
      <span class="readout">{i18n.t("track.tone")} {Math.round(tone * 100)}%</span>
    </div>
  {/if}
  {#if body !== undefined}
    <div class="room" title={i18n.t("track.bodyHint")}>
      <Knob
        value={body}
        min={0}
        max={1}
        reset={0}
        size={30}
        color="var(--stem-drums)"
        label={`${name} · ${i18n.t("track.body")}`}
        display={`${Math.round(body * 100)}%`}
        onchange={(v) => onchange({ body: Math.round(v * 100) / 100 })}
      />
      <span class="readout">{i18n.t("track.body")} {Math.round(body * 100)}%</span>
    </div>
  {/if}
  <div class="fader-zone">
    <VFader db={gainDb} {color} dim={muted || silenced} label={`${name} · ${i18n.t("track.volume")}`} onchange={(db) => onchange({ gainDb: db })} />
    <LedMeter {level} />
  </div>
  <span class="readout db tabular">{formatDb(gainDb)}</span>
  <div class="buttons">
    {#if muteable}<button class="lit mute" class:on={muted} aria-pressed={muted} title={i18n.t("track.mute")} onclick={() => onchange({ muted: !muted })}>M</button>{/if}
    {#if solo !== undefined}
      <button class="lit solo" class:on={solo} aria-pressed={solo} title={i18n.t("track.solo")} onclick={() => onchange({ solo: !solo })}>S</button>
    {/if}
    {#if armed !== undefined}
      <button class="lit arm" class:on={armed} aria-pressed={armed} title={i18n.t("record.arm")} onclick={() => onarm?.()}>●</button>
    {/if}
  </div>
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
    border-radius: var(--radius-s);
    background: var(--bg-panel);
    border: 1px solid var(--line);
  }
  .strip.wide {
    width: 88px;
  }
  .cap {
    position: absolute;
    inset: -1px -1px auto;
    height: 2px;
    border-radius: 0;
    background: var(--c);
  }
  .off .cap {
    opacity: 0.3;
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
    color: var(--c);
  }
  .name {
    max-width: 100%;
    font-size: var(--fs-small);
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pan {
    height: 50px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
  }
  .room {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
  }
  .readout {
    font-family: var(--mono);
    font-size: 11px; /* dense strip readout */
    color: var(--text-faint);
  }
  .db {
    font-size: 11px; /* dense strip readout */
    color: var(--text-dim);
  }
  .fader-zone {
    flex: 1;
    min-height: 120px;
    display: flex;
    gap: 4px;
  }
  .buttons {
    display: flex;
    gap: 5px;
  }
  .lit {
    width: 30px;
    height: 24px;
    border-radius: var(--radius-s);
    font-size: var(--fs-label);
    color: var(--text-dim);
    border: 1px solid var(--line-strong);
  }
  .lit:hover {
    color: var(--text);
  }
  .mute.on {
    background: var(--danger);
    border-color: var(--danger);
    color: var(--accent-ink);
  }
  .arm {
    color: color-mix(in srgb, var(--danger) 70%, var(--text-dim));
  }
  .arm.on {
    background: var(--danger);
    border-color: var(--danger);
    color: var(--accent-ink);
  }
  .head {
    position: relative;
  }
  .del {
    position: absolute;
    top: -8px;
    right: -2px;
    width: 18px;
    height: 18px;
    border-radius: var(--radius-s);
    font-size: var(--fs-body);
    line-height: 1;
    color: var(--text-faint);
    background: transparent;
  }
  .del:hover {
    color: #fff;
    background: var(--danger);
  }
  .solo.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-ink);
  }
</style>
