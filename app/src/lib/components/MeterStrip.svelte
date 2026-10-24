<script lang="ts">
  // A console strip that only shows activity: a level meter, a tag and an
  // optional note button. Used where a fader would do nothing (a MIDI kit, or
  // an input the interface already monitors directly).
  import type { StemKind } from "../types";
  import LedMeter from "./LedMeter.svelte";
  import StemIcon from "./StemIcon.svelte";

  let {
    kind,
    name,
    color,
    tag,
    level = 0,
    caption = "",
    note = undefined,
    noteTitle = "",
    onnote = undefined,
    armed = undefined,
    onarm = undefined,
    armTitle = "",
  }: {
    kind: StemKind;
    name: string;
    color: string;
    tag: string;
    level?: number;
    /** Small text under the meter (e.g. the last pad played). */
    caption?: string;
    note?: string | undefined;
    noteTitle?: string;
    onnote?: (() => void) | undefined;
    /** Record-arm state; shows an arm button. */
    armed?: boolean | undefined;
    onarm?: (() => void) | undefined;
    armTitle?: string;
  } = $props();
</script>

<div class="strip" class:idle={level === 0} style={`--c:${color}`}>
  <div class="cap"></div>
  <div class="head">
    <span class="icon"><StemIcon {kind} size={17} /></span>
    <span class="name" title={name}>{name}</span>
    <span class="tag" title={tag}>{tag}</span>
  </div>
  <div class="meter-zone">
    <LedMeter {level} />
  </div>
  {#if caption}<span class="caption">{caption}</span>{/if}
  {#if note}
    <button class="note" title={noteTitle} onclick={() => onnote?.()}>{note}</button>
  {/if}
  {#if armed !== undefined}
    <button class="arm" class:on={armed} aria-pressed={armed} title={armTitle} onclick={() => onarm?.()}>●</button>
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
    background: var(--c);
    box-shadow: 0 0 10px var(--c);
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
    color: var(--c);
  }
  .name {
    max-width: 100%;
    font-size: var(--fs-small);
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    font-family: var(--mono);
    font-size: var(--fs-label);
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    max-width: 100%;
    padding: 1px 5px;
    border-radius: 999px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 14%, transparent);
  }
  .meter-zone {
    flex: 1;
    min-height: 120px;
    display: flex;
    justify-content: center;
  }
  .caption {
    max-width: 100%;
    font-size: var(--fs-small);
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .note {
    width: 100%;
    padding: 4px 2px;
    border-radius: 6px;
    font-size: var(--fs-small);
    font-weight: 700;
    line-height: 1.2;
    hyphens: auto;
    overflow-wrap: break-word;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .arm {
    width: 30px;
    height: 24px;
    border-radius: 6px;
    font-size: var(--fs-label);
    font-weight: 800;
    color: color-mix(in srgb, var(--danger) 70%, var(--text-dim));
    background: #222838;
    box-shadow: inset 0 -2px 0 rgb(0 0 0 / 35%);
  }
  .arm.on {
    background: var(--danger);
    color: #fff;
    box-shadow: 0 0 10px color-mix(in srgb, var(--danger) 60%, transparent);
  }
  .note:hover {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
</style>
