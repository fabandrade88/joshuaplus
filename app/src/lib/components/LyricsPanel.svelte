<script lang="ts">
  // Lyrics for singing along: the line being sung and the next one, in time
  // with the song (or the whole text when the lyrics are not time-synced).
  // Found online once, then kept with the song; they can also be pasted.
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import { lineAt, toLrc } from "../timeline";
  import Icon from "./Icon.svelte";

  /** Lines show a little early, so the singer can read ahead. */
  const LEAD_SECS = 0.3;

  const lyrics = $derived(app.lyrics);
  const synced = $derived(lyrics?.synced ?? []);
  const pos = $derived(app.transport.positionSecs);
  const current = $derived(synced.length ? lineAt(synced, pos + LEAD_SECS, lyrics?.offsetSecs ?? 0) : -1);
  const line = $derived(current >= 0 ? (synced[current]?.text ?? "") : "");
  const next = $derived(synced.slice(current + 1).find((l) => l.text.trim())?.text ?? "");
  const project = $derived(app.song?.project);

  let editing = $state(false);
  let draft = $state("");

  function edit(): void {
    draft = lyrics ? (lyrics.synced.length ? toLrc(lyrics.synced) : lyrics.plain) : "";
    editing = true;
  }

  async function save(): Promise<void> {
    await app.saveLyrics(draft);
    editing = false;
  }

  function fmtOffset(s: number): string {
    return `${s > 0 ? "+" : ""}${s.toFixed(1)} s`;
  }
</script>

<section class="lyrics" aria-label={i18n.t("guide.lyrics")}>
  {#if editing}
    <div class="editor">
      <textarea bind:value={draft} rows="6" spellcheck="false" placeholder={i18n.t("guide.pastePlaceholder")} aria-label={i18n.t("guide.paste")}
      ></textarea>
      <div class="actions">
        <span class="note">{i18n.t("guide.pasteNote")}</span>
        <button onclick={() => (editing = false)}>{i18n.t("guide.cancel")}</button>
        <button class="primary" onclick={save}>{i18n.t("guide.save")}</button>
      </div>
    </div>
  {:else if app.lyricsStatus === "loading"}
    <p class="status">{i18n.t("guide.lyricsLoading")}</p>
  {:else if app.lyricsStatus === "offline"}
    <p class="status">
      {i18n.t("guide.lyricsOffline")}
      <button class="link" onclick={() => app.loadLyrics(true)}>{i18n.t("guide.retry")}</button>
      <button class="link" onclick={edit}>{i18n.t("guide.paste")}</button>
    </p>
  {:else if !lyrics || app.lyricsStatus === "none"}
    <p class="status">
      {i18n.t("guide.lyricsNone", { title: project?.title ?? "" })}
      <button class="link" onclick={() => app.loadLyrics(true)}>{i18n.t("guide.retry")}</button>
      <button class="link" onclick={edit}>{i18n.t("guide.paste")}</button>
    </p>
  {:else if lyrics.instrumental && lyrics.synced.length === 0 && !lyrics.plain.trim()}
    <p class="status">{i18n.t("guide.instrumental")}</p>
  {:else}
    <div class="body">
      {#if synced.length}
        <div class="lines" aria-live="off">
          <p class="now" class:empty={!line.trim()}>{line.trim() || "♪"}</p>
          <p class="next">{next}</p>
        </div>
      {:else}
        <pre class="plain">{lyrics.plain}</pre>
      {/if}
      <div class="tools">
        {#if synced.length}
          <div class="nudge" title={i18n.t("guide.offsetHint")}>
            <button aria-label={i18n.t("guide.earlier")} onclick={() => app.nudgeLyrics(-0.5)}>−</button>
            <span class="tabular">{fmtOffset(lyrics.offsetSecs)}</span>
            <button aria-label={i18n.t("guide.later")} onclick={() => app.nudgeLyrics(0.5)}>+</button>
          </div>
        {:else}
          <span class="note">{i18n.t("guide.notSynced")}</span>
        {/if}
        <button class="icon" title={i18n.t("guide.edit")} aria-label={i18n.t("guide.edit")} onclick={edit}><Icon name="edit" size={13} /></button>
        <span class="credit">{lyrics.source === "lrclib" ? i18n.t("guide.fromLrclib") : i18n.t("guide.fromUser")}</span>
      </div>
    </div>
  {/if}
</section>

<style>
  .lyrics {
    flex: none;
    padding: 8px 24px;
    border-bottom: 1px solid var(--line);
  }
  .body {
    display: flex;
    align-items: center;
    gap: 16px;
    min-width: 0;
  }
  .lines {
    flex: 1;
    min-width: 0;
  }
  .lines p {
    margin: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .now {
    font-size: 20px;
    font-weight: 600;
    line-height: 1.3;
    color: var(--text);
  }
  .now.empty {
    color: var(--text-faint);
  }
  .next {
    font-size: var(--fs-body);
    color: var(--text-faint);
    min-height: 1.3em;
  }
  .plain {
    flex: 1;
    margin: 0;
    max-height: 96px;
    overflow-y: auto;
    font-family: var(--font);
    font-size: var(--fs-body);
    line-height: 1.45;
    white-space: pre-wrap;
    color: var(--text);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .nudge {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border-radius: var(--radius-s);
    background: var(--bg-panel);
    border: 1px solid var(--line);
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .nudge span {
    min-width: 46px;
    text-align: center;
  }
  .nudge button,
  .icon {
    display: grid;
    place-items: center;
    min-width: 22px;
    height: 22px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
  }
  .nudge button:hover,
  .icon:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .credit,
  .note {
    font-size: var(--fs-small);
    color: var(--text-faint);
    white-space: nowrap;
  }
  .status {
    margin: 0;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .link {
    margin-left: 8px;
    font-size: var(--fs-small);
    color: var(--accent);
    text-decoration: underline;
  }
  .editor {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  textarea {
    width: 100%;
    resize: vertical;
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
  .actions .note {
    margin-right: auto;
    white-space: normal;
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
  @media (max-height: 760px) {
    .lyrics {
      padding-top: 5px;
      padding-bottom: 5px;
    }
    .now {
      font-size: 17px;
    }
    .plain {
      max-height: 60px;
    }
  }
</style>
