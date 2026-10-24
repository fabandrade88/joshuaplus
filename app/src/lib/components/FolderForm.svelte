<script lang="ts">
  import { untrack } from "svelte";
  import { i18n } from "../i18n/index.svelte";
  import type { FolderColor } from "../types";

  const COLORS: FolderColor[] = ["amber", "rose", "violet", "green", "cyan", "slate"];

  let {
    name = "",
    color = "amber",
    submitLabel,
    onsubmit,
    oncancel,
  }: {
    name?: string;
    color?: FolderColor;
    submitLabel: string;
    onsubmit: (name: string, color: FolderColor) => void;
    oncancel: () => void;
  } = $props();

  // The form edits a copy of the initial values.
  let value = $state(untrack(() => name));
  let selected = $state<FolderColor>(untrack(() => color));
  let input: HTMLInputElement | undefined = $state();

  $effect(() => input?.focus());

  function submit(e: SubmitEvent): void {
    e.preventDefault();
    if (value.trim()) onsubmit(value.trim(), selected);
  }
</script>

<form class="form" onsubmit={submit}>
  <input onkeydown={(e) => e.key === "Escape" && oncancel()} bind:this={input} bind:value maxlength="60" placeholder={i18n.t("home.folderName")} aria-label={i18n.t("home.folderName")} />
  <div class="colors" role="radiogroup">
    {#each COLORS as c (c)}
      <button
        type="button"
        role="radio"
        aria-checked={selected === c}
        aria-label={c}
        class="swatch"
        class:on={selected === c}
        style={`--c:var(--folder-${c})`}
        onclick={() => (selected = c)}
      ></button>
    {/each}
  </div>
  <div class="buttons">
    <button type="button" class="btn" onclick={oncancel}>{i18n.t("home.cancel")}</button>
    <button type="submit" class="btn primary" disabled={!value.trim()}>{submitLabel}</button>
  </div>
</form>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  input {
    height: 40px;
    padding: 0 12px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line-strong);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-body);
    user-select: text;
  }
  .colors {
    display: flex;
    gap: 8px;
  }
  .swatch {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background: var(--c);
    opacity: 0.55;
    transition: opacity 0.12s, transform 0.12s;
  }
  .swatch.on,
  .swatch:hover {
    opacity: 1;
    transform: scale(1.1);
  }
  .swatch.on {
    box-shadow: 0 0 0 2px var(--bg-panel), 0 0 0 4px var(--c);
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
