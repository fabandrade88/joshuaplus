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
    border-radius: var(--radius-s);
    background: var(--bg-panel);
    border: 1px solid var(--line);
  }
  .cap {
    position: absolute;
    inset: -1px -1px auto;
    height: 2px;
    border-radius: 0;
    background: var(--c);
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
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    font-family: var(--mono);
    font-size: var(--fs-label);
    max-width: 100%;
    padding: 0 4px;
    border-radius: var(--radius-s);
    border: 1px solid currentColor;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--c);
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
    line-height: 1.25;
    text-align: center;
    color: var(--text-dim);
    /* Up to two lines ("Volume on the interface"), then an ellipsis. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .note {
    width: 100%;
    padding: 4px 2px;
    border-radius: var(--radius-s);
    font-size: var(--fs-small);
    line-height: 1.2;
    hyphens: auto;
    overflow-wrap: break-word;
    color: var(--accent);
    border: 1px solid var(--accent);
  }
  .arm {
    width: 30px;
    height: 24px;
    border-radius: var(--radius-s);
    font-size: var(--fs-label);
    color: color-mix(in srgb, var(--danger) 70%, var(--text-dim));
    border: 1px solid var(--line-strong);
  }
  .arm.on {
    background: var(--danger);
    border-color: var(--danger);
    color: var(--accent-ink);
  }
  .note:hover {
    background: var(--bg-hover);
  }
</style>
