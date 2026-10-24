<script lang="ts">
  import { app } from "../app.svelte";
  import { formatBpm, formatTime } from "../format";
  import { i18n } from "../i18n/index.svelte";
  import type { MessageKey } from "../i18n/en";
  import { engineOf } from "../stems";
  import { barBeatAt } from "../timeline";
  import { studio } from "../video/studio.svelte";
  import Icon from "./Icon.svelte";

  const project = $derived(app.song?.project);
  const pos = $derived(app.transport.positionSecs);
  const bb = $derived(project ? barBeatAt(project.analysis, pos) : null);
  const playing = $derived(app.transport.playing || app.transport.countingIn);
  const engine = $derived(project ? engineOf(project.separator) : "studio");
  const folder = $derived(app.folderById(project?.folder));
  const index = $derived(app.playlistIndex);
  const total = $derived(app.playlist.length);
  const listName = $derived(
    app.browsing === "all"
      ? i18n.t("home.allSongs")
      : app.browsing === null
        ? i18n.t("home.unfiled")
        : (app.folderById(app.browsing)?.name ?? ""),
  );
  const cover = $derived(app.song?.cover ?? null);
  const meta = $derived(project?.metadata ?? {});
  const metaRows = $derived(
    (
      [
        ["meta.artist", meta.artist],
        ["meta.album", meta.album],
        ["meta.albumArtist", meta.albumArtist],
        ["meta.year", meta.year],
        ["meta.genre", meta.genre],
        ["meta.track", meta.track],
        ["meta.composer", meta.composer],
        ["meta.label", meta.label],
      ] as [MessageKey, string | null | undefined][]
    ).filter((r): r is [MessageKey, string] => !!r[1]),
  );

  let editingTitle = $state(false);
  let titleDraft = $state("");
  let infoOpen = $state(false);

  function startTitleEdit(): void {
    if (!project) return;
    titleDraft = project.title;
    editingTitle = true;
  }

  function commitTitle(): void {
    editingTitle = false;
    if (project && titleDraft.trim() && titleDraft.trim() !== project.title) void app.renameSong(project.id, titleDraft);
  }

  function focusSelect(node: HTMLInputElement): void {
    node.focus();
    node.select();
  }

  const progress = $derived(project && project.durationSecs > 0 ? (pos / project.durationSecs) * 100 : 0);
</script>

<svelte:window
  onclick={(e) => {
    if (infoOpen && !(e.target as HTMLElement).closest(".info-wrap")) infoOpen = false;
  }}
  onkeydown={(e) => {
    if (e.key === "Escape") infoOpen = false;
  }}
/>

{#if project}
  <header class="deck">
    <div class="info" class:with-cover={!!cover}>
      {#if cover}<img class="cover" src={cover} alt="" />{/if}
      <div class="info-text">
      <button class="back" onclick={() => app.backFromSong()}>
        <Icon name="back" size={14} /><span class="crumb">{listName || folder?.name || i18n.t("song.back")}</span>
        {#if index >= 0}<span class="pos tabular">· {i18n.t("playlist.position", { i: index + 1, n: total })}</span>{/if}
      </button>
      <div class="title-row">
        {#if editingTitle}
          <input
            class="title-input"
            bind:value={titleDraft}
            maxlength="200"
            aria-label={i18n.t("song.rename")}
            use:focusSelect
            onkeydown={(e) => {
              if (e.key === "Enter") commitTitle();
              else if (e.key === "Escape") editingTitle = false;
            }}
            onblur={commitTitle}
          />
        {:else}
          <button class="title" title={i18n.t("song.renameHint")} onclick={startTitleEdit}>
            <h1>{project.title}</h1>
          </button>
        {/if}
        <div class="info-wrap">
          <button
            class="info-btn"
            class:on={infoOpen}
            aria-expanded={infoOpen}
            title={i18n.t("meta.info")}
            aria-label={i18n.t("meta.info")}
            onclick={() => (infoOpen = !infoOpen)}>i</button
          >
          {#if infoOpen}
            <div class="popover" role="dialog" aria-label={i18n.t("meta.info")}>
              {#if cover}<img src={cover} alt="" />{/if}
              {#if metaRows.length > 0}
                <dl>
                  {#each metaRows as [k, v] (k)}
                    <dt>{i18n.t(k)}</dt>
                    <dd>{v}</dd>
                  {/each}
                </dl>
              {:else}
                <p class="none">{i18n.t("meta.none")}</p>
              {/if}
            </div>
          {/if}
        </div>
      </div>
      <div class="chips">
        <button
          class="chip export"
          title={i18n.t("export.hint")}
          disabled={app.exportProgress !== null || app.recording}
          onclick={() => app.exportMix()}
        >
          <Icon name="export" size={12} />
          {app.exportProgress !== null ? `${Math.round(app.exportProgress * 100)}%` : i18n.t("export.audio")}
        </button>
        {#if app.lastExport}
          <button class="chip link" title={app.lastExport} onclick={() => app.revealExport()}>{i18n.t("export.show")}</button>
        {/if}
        <span class="chip tabular">{formatBpm(project.analysis.bpm)} BPM · {project.analysis.beatsPerBar}/4</span>
        <span class="chip">
          <Icon name={engine === "cloud" ? "cloud" : engine === "studio" ? "spark" : "music"} size={12} />
          {i18n.t(`engine.${engine}` as MessageKey)}
        </span>
      </div>
      <div class="up-next">
        <button
          class="auto"
          class:on={app.autoAdvance}
          aria-pressed={app.autoAdvance}
          title={i18n.t("playlist.auto")}
          onclick={() => app.toggleAutoAdvance()}
        >
          <Icon name="loop" size={12} /> Auto
        </button>
        <span class="next" title={app.nextSong?.title}>
          {app.nextSong ? i18n.t("playlist.next", { title: app.nextSong.title }) : i18n.t("playlist.end")}
        </span>
      </div>
      </div>
    </div>

    <div class="transport">
      <button class="round small" title={i18n.t("playlist.prev")} aria-label={i18n.t("playlist.prev")} onclick={() => app.playPrev()} disabled={!app.prevSong}>
        <Icon name="prev" size={16} />
      </button>
      <button class="round" title={i18n.t("transport.toStart")} onclick={() => app.seek(app.loop && app.loopEnabled ? app.loop.start : 0)} disabled={!app.canPlay}>
        <Icon name="toStart" />
      </button>
      <button
        class="play"
        class:on={playing}
        aria-label={playing ? i18n.t("transport.pause") : i18n.t("transport.play")}
        title={playing ? i18n.t("transport.pause") : i18n.t("transport.play")}
        onclick={() => app.togglePlay()}
        disabled={!app.canPlay}
      >
        <Icon name={playing ? "pause" : "play"} size={24} fill />
      </button>
      <button
        class="round"
        class:lit={app.loopEnabled}
        aria-pressed={app.loopEnabled}
        title={i18n.t("transport.loopHint")}
        onclick={() => app.toggleLoop()}
      >
        <Icon name="loop" />
      </button>
      <button class="round small" title={i18n.t("playlist.nextBtn")} aria-label={i18n.t("playlist.nextBtn")} onclick={() => app.playNext(app.transport.playing)} disabled={!app.nextSong}>
        <Icon name="next" size={16} />
      </button>
      <span class="sep"></span>
      <div class="rec-group">
        <button
          class="rec"
          class:on={app.recording}
          class:busy={app.savingTakes}
          aria-pressed={app.recording}
          title={app.recording ? i18n.t("record.stop") : app.recordMode === "video" ? i18n.t("record.openVideo") : i18n.t("record.start")}
          aria-label={app.recording ? i18n.t("record.stop") : i18n.t("record.start")}
          disabled={!app.canPlay || app.savingTakes}
          onclick={() => {
            if (app.videoOpen && studio.phase === "recording") void studio.stop(); // stop the camera too
            else if (app.recordMode === "video" && !app.recording) app.videoOpen = true;
            else void app.toggleRecording();
          }}
        >
          <Icon name={app.recording ? "stop" : app.recordMode === "video" ? "video" : "record"} size={18} fill={app.recordMode === "audio" || app.recording} />
        </button>
        <div class="mode" role="radiogroup" aria-label={i18n.t("record.mode")}>
          <button
            role="radio"
            aria-checked={app.recordMode === "audio"}
            class:on={app.recordMode === "audio"}
            title={i18n.t("record.audio")}
            disabled={app.recording}
            onclick={() => (app.recordMode = "audio")}><Icon name="mic" size={13} /></button
          >
          <button
            role="radio"
            aria-checked={app.recordMode === "video"}
            class:on={app.recordMode === "video"}
            title={i18n.t("record.video")}
            disabled={app.recording}
            onclick={() => (app.recordMode = "video")}><Icon name="video" size={13} /></button
          >
        </div>
      </div>
    </div>

    <div class="counter" class:counting={app.transport.countingIn} class:recording={app.recording}>
      <div class="cols">
        <div>
          <span class="k">{i18n.t("song.bar")}</span>
          <span class="v tabular">{app.transport.countingIn ? "–" : (bb?.bar ?? "–")}</span>
        </div>
        <div>
          <span class="k">{i18n.t("transport.beat")}</span>
          <span class="v tabular">{app.transport.countingIn ? "•" : (bb?.beat ?? "–")}</span>
        </div>
        <div>
          <span class="k">{formatTime(project.durationSecs)}</span>
          <span class="v small tabular">{formatTime(pos, true)}</span>
        </div>
      </div>
      <div class="bar"><div style={`width:${progress}%`}></div></div>
    </div>
  </header>
{/if}

<style>
  .deck {
    display: grid;
    grid-template-columns: minmax(200px, 1fr) auto minmax(260px, 340px);
    grid-template-areas: "info transport counter";
    align-items: center;
    gap: 28px;
    padding: 16px 24px;
    border-bottom: 1px solid var(--line);
    background: var(--bg-raised);
  }
  .info {
    grid-area: info;
    min-width: 0;
    display: flex;
    gap: 14px;
    align-items: center;
  }
  .info-text {
    min-width: 0;
    flex: 1;
  }
  .cover {
    width: 84px;
    height: 84px;
    flex: none;
    border-radius: 10px;
    object-fit: cover;
    box-shadow: 0 8px 24px rgb(0 0 0 / 45%);
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .title {
    min-width: 0;
    text-align: left;
    border-radius: 6px;
    cursor: text;
  }
  .title:hover h1 {
    text-decoration: underline dotted var(--text-faint);
    text-underline-offset: 4px;
  }
  .title-input {
    flex: 1;
    min-width: 0;
    height: 34px;
    margin: 2px 0 6px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px solid var(--accent);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-title);
    font-weight: 800;
    user-select: text;
  }
  .info-wrap {
    position: relative;
    flex: none;
  }
  .info-btn {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    color: var(--text-faint);
    font-size: var(--fs-small);
    font-weight: 800;
    font-family: Georgia, serif;
    font-style: italic;
  }
  .info-btn:hover,
  .info-btn.on {
    color: var(--text);
    border-color: var(--text-faint);
  }
  .popover {
    position: absolute;
    top: 28px;
    left: -12px;
    z-index: 20;
    width: 280px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--bg-panel);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
  }
  .popover img {
    width: 100%;
    aspect-ratio: 1;
    object-fit: cover;
    border-radius: 8px;
    margin-bottom: 12px;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 12px;
    margin: 0;
    font-size: var(--fs-small);
  }
  dt {
    color: var(--text-faint);
  }
  dd {
    margin: 0;
    color: var(--text);
    overflow-wrap: anywhere;
    user-select: text;
  }
  .none {
    margin: 0;
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .back {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
    white-space: nowrap;
    font-size: var(--fs-small);
    font-weight: 600;
    color: var(--text-faint);
  }
  .back:hover {
    color: var(--text);
  }
  .crumb {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pos {
    flex: none;
    color: var(--text-faint);
    font-weight: 500;
  }
  .up-next {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    min-width: 0;
  }
  .auto {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    font-size: var(--fs-small);
    font-weight: 700;
    color: var(--text-faint);
    border: 1px solid var(--line-strong);
  }
  .auto.on {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .next {
    font-size: var(--fs-small);
    color: var(--text-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .round.small {
    width: 32px;
    height: 32px;
    background: transparent;
  }
  h1 {
    margin: 2px 0 6px;
    font-size: 22px;
    font-weight: 800;
    letter-spacing: -0.02em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 6px;
  }
  .chip {
    white-space: nowrap;
  }
  .transport {
    grid-area: transport;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 14px;
    border-radius: 999px;
    background: #0d0f15;
    border: 1px solid var(--line);
  }
  .round {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    color: var(--text-dim);
    background: var(--bg-active);
  }
  .round:hover {
    color: var(--text);
  }
  .round.lit {
    color: var(--accent-ink);
    background: var(--accent);
    box-shadow: 0 0 14px color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .play {
    display: grid;
    place-items: center;
    width: 60px;
    height: 60px;
    border-radius: 50%;
    color: var(--accent-ink);
    background: var(--accent);
    box-shadow: 0 6px 24px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .play:active {
    transform: scale(0.96);
  }
  .sep {
    width: 1px;
    height: 30px;
    background: var(--line);
  }
  .rec-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .rec {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, var(--bg-active));
    border: 2px solid color-mix(in srgb, var(--danger) 55%, transparent);
  }
  .rec:hover:not(:disabled) {
    background: color-mix(in srgb, var(--danger) 24%, var(--bg-active));
  }
  .rec.on {
    color: #fff;
    background: var(--danger);
    animation: rec-pulse 1.2s ease-in-out infinite;
  }
  .rec.busy {
    animation: none;
    opacity: 0.6;
  }
  @keyframes rec-pulse {
    50% {
      box-shadow: 0 0 0 8px color-mix(in srgb, var(--danger) 20%, transparent);
    }
  }
  .mode {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .mode button {
    display: grid;
    place-items: center;
    width: 26px;
    height: 20px;
    border-radius: 6px;
    color: var(--text-faint);
    background: var(--bg-active);
  }
  .mode button.on {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 16%, var(--bg-active));
  }
  .chip.export,
  .chip.link {
    cursor: pointer;
    font-weight: 700;
  }
  .chip.export {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .chip.export:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .chip.link {
    color: var(--text-dim);
    text-decoration: underline;
  }
  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .counter {
    grid-area: counter;
    min-width: 0;
    padding: 10px 14px 8px;
    border-radius: 12px;
    background: #0b0c10;
    box-shadow: inset 0 2px 8px rgb(0 0 0 / 70%);
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr 1.2fr;
    gap: 10px;
  }
  .cols > div {
    display: flex;
    flex-direction: column;
  }
  .k {
    font-size: var(--fs-label);
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--text-faint);
  }
  .v {
    font-family: var(--mono);
    font-size: 28px;
    font-weight: 700;
    line-height: 1.1;
    color: var(--accent);
  }
  .v.small {
    font-size: var(--fs-title);
    padding-top: 7px;
    color: var(--text-dim);
  }
  .counting .v {
    animation: blink 0.4s steps(2) infinite;
  }
  .counter.recording {
    box-shadow:
      inset 0 2px 8px rgb(0 0 0 / 70%),
      0 0 0 2px color-mix(in srgb, var(--danger) 70%, transparent);
  }
  .counter.recording .bar div {
    background: var(--danger);
  }
  @keyframes blink {
    50% {
      opacity: 0.35;
    }
  }
  .bar {
    height: 3px;
    margin-top: 8px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
  }
  /* Medium windows: a tighter transport so the title keeps its room. */
  @media (max-width: 1520px) {
    .deck {
      grid-template-columns: minmax(180px, 1fr) auto minmax(200px, 260px);
      gap: 18px;
      padding: 12px 18px;
    }
    .cover {
      width: 68px;
      height: 68px;
    }
    .transport {
      gap: 9px;
      padding: 6px 10px;
    }
    .round {
      width: 34px;
      height: 34px;
    }
    .round.small {
      width: 28px;
      height: 28px;
    }
    .play {
      width: 50px;
      height: 50px;
    }
    .rec {
      width: 38px;
      height: 38px;
    }
    .rec-group {
      gap: 6px;
    }
    .v {
      font-size: var(--fs-h1);
    }
    .v.small {
      font-size: 16px;
      padding-top: 5px;
    }
  }
  /* Narrow windows (down to the 960 px minimum): the transport moves under the
     counter, so the song info keeps a readable width. */
  @media (max-width: 1180px) {
    .deck {
      grid-template-columns: minmax(0, 1fr) auto;
      grid-template-areas:
        "info counter"
        "info transport";
      row-gap: 8px;
      column-gap: 16px;
      padding: 10px 16px;
    }
    .transport {
      justify-self: stretch;
      justify-content: space-between;
    }
    .counter {
      padding: 6px 12px 6px;
    }
    .bar {
      margin-top: 5px;
    }
  }
  @media (max-height: 760px) {
    .deck {
      padding-top: 8px;
      padding-bottom: 8px;
    }
    .cover {
      width: 60px;
      height: 60px;
    }
    h1 {
      font-size: var(--fs-title);
      margin: 0 0 4px;
    }
    .up-next {
      margin-top: 4px;
    }
  }
</style>
