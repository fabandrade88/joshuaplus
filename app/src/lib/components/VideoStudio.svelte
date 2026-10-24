<script lang="ts">
  // Video studio overlay: camera preview with format guides, filming, sync
  // check and export to YouTube / Shorts·Reels·TikTok / Instagram formats.
  import { onDestroy, onMount } from "svelte";
  import { app } from "../app.svelte";
  import { formatTime } from "../format";
  import type { MessageKey } from "../i18n/en";
  import { i18n } from "../i18n/index.svelte";
  import { FORMATS, coverCrop, type FormatId } from "../video/formats";
  import { studio } from "../video/studio.svelte";
  import Icon from "./Icon.svelte";

  let live = $state<HTMLVideoElement | null>(null);
  let review = $state<HTMLVideoElement | null>(null);
  let frameW = $state(0);
  let frameH = $state(0);

  const LABELS: Record<FormatId, MessageKey> = {
    youtube: "video.fmt.youtube",
    vertical: "video.fmt.vertical",
    square: "video.fmt.square",
    portrait: "video.fmt.portrait",
  };

  $effect(() => {
    if (live) live.srcObject = studio.stream;
  });

  onMount(() => {
    if (studio.supported) void studio.openCamera();
  });
  onDestroy(() => void studio.close());

  async function close(): Promise<void> {
    await studio.close();
    app.videoOpen = false;
  }

  /** Crop guides (in % of the preview) for the chosen formats. */
  const guides = $derived(
    studio.selectedFormats
      .filter((f) => f.id !== "youtube")
      .map((f) => {
        const c = coverCrop(frameW || 16, frameH || 9, f.width, f.height);
        return { id: f.id, left: (c.sx / (frameW || 16)) * 100, width: (c.sw / (frameW || 16)) * 100, top: (c.sy / (frameH || 9)) * 100, height: (c.sh / (frameH || 9)) * 100 };
      }),
  );
  const busy = $derived(studio.phase === "starting" || studio.phase === "saving" || studio.phase === "exporting");
  /** While filming, the studio shrinks to a corner so the app stays visible. */
  const mini = $derived(studio.phase === "starting" || studio.phase === "recording");
</script>

{#snippet formatPicker()}
  <h3>{i18n.t("video.formats")}</h3>
  <div class="formats">
    {#each FORMATS as f (f.id)}
      <label class="fmt" class:on={studio.formats[f.id]}>
        <input type="checkbox" bind:checked={studio.formats[f.id]} disabled={studio.phase === "exporting" || studio.phase === "recording"} />
        <span class="shape" style={`aspect-ratio:${f.width}/${f.height}`}></span>
        <span class="fmt-name">{i18n.t(LABELS[f.id])}</span>
        <span class="fmt-size tabular">{f.width}×{f.height}</span>
      </label>
    {/each}
  </div>
  <label class="check">
    <input type="checkbox" bind:checked={studio.showTitle} disabled={studio.phase === "exporting"} />
    {i18n.t("video.showTitle")}
  </label>
{/snippet}

<svelte:window onkeydown={(e) => e.key === "Escape" && !busy && studio.phase !== "recording" && void close()} />

<div class="backdrop" class:mini role="presentation">
  <div class="studio" class:mini role="dialog" aria-modal={!mini} tabindex="-1" aria-label={i18n.t("video.title")}>
    <header>
      <span class="badge"><Icon name="video" size={18} /></span>
      <div>
        <h2>{i18n.t("video.title")}</h2>
        <p>{i18n.t("video.subtitle")}</p>
      </div>
      <button class="icon-btn" title={i18n.t("common.close")} aria-label={i18n.t("common.close")} disabled={busy || studio.phase === "recording"} onclick={() => close()}>
        <Icon name="close" size={16} />
      </button>
    </header>

    {#if !studio.supported}
      <p class="error">{i18n.t("video.unsupported")}</p>
    {:else}
      <div class="body">
        <div class="stage">
          {#if studio.phase === "review" || studio.phase === "exporting"}
            <!-- svelte-ignore a11y_media_has_caption -->
            <video bind:this={review} src={studio.clipUrl} controls playsinline></video>
          {:else}
            <div class="live">
              <!-- svelte-ignore a11y_media_has_caption -->
              <video
                bind:this={live}
                autoplay
                muted
                playsinline
                onloadedmetadata={(e) => {
                  frameW = e.currentTarget.videoWidth;
                  frameH = e.currentTarget.videoHeight;
                }}
              ></video>
              {#each guides as g (g.id)}
                <div class="guide" style={`left:${g.left}%;top:${g.top}%;width:${g.width}%;height:${g.height}%`}>
                  <span>{i18n.t(LABELS[g.id])}</span>
                </div>
              {/each}
              {#if studio.phase === "recording"}
                <div class="rec-badge"><i></i>REC {formatTime(studio.elapsed)}</div>
              {/if}
              {#if studio.cameraError}
                <div class="cam-error">{studio.cameraError}</div>
              {/if}
            </div>
          {/if}
        </div>

        <aside>
          {#if studio.phase === "review" || studio.phase === "exporting"}
            <h3>{i18n.t("video.sync")}</h3>
            <p class="hint">{i18n.t("video.syncHint")}</p>
            <div class="sync">
              <input
                type="range"
                min="-400"
                max="300"
                step="10"
                bind:value={studio.syncMs}
                disabled={studio.phase === "exporting"}
                aria-label={i18n.t("video.sync")}
              />
              <span class="tabular">{studio.syncMs > 0 ? "+" : ""}{studio.syncMs} ms</span>
            </div>
            <button class="btn" disabled={studio.phase === "exporting" || !review} onclick={() => review && studio.preview(review)}>
              <Icon name="play" size={13} fill />{i18n.t("video.preview")}
            </button>

            {@render formatPicker()}

            {#if studio.phase === "exporting"}
              <div class="progress"><div style={`width:${studio.progress * 100}%`}></div></div>
              <p class="hint">{i18n.t("video.exporting")}</p>
              <button class="btn" onclick={() => studio.cancel()}>{i18n.t("common.cancel")}</button>
            {:else}
              <button class="btn primary" disabled={studio.selectedFormats.length === 0} onclick={() => studio.export()}>
                <Icon name="export" size={14} />{i18n.t("video.export", { n: studio.selectedFormats.length })}
              </button>
              <button class="btn" onclick={() => studio.discardClip()}>{i18n.t("video.again")}</button>
            {/if}
            {#if studio.saved.length > 0}
              <ul class="saved">
                {#each studio.saved as path (path)}
                  <li>
                    <span title={path}>{path.split(/[\\/]/).pop()}</span>
                    <button class="link" onclick={() => app.revealExport(path)}>{i18n.t("export.show")}</button>
                  </li>
                {/each}
              </ul>
            {/if}
            <p class="note">{studio.container?.ext === "webm" ? i18n.t("video.webmNote") : i18n.t("video.mp4Note")}</p>
          {:else}
            {#if studio.cameras.length > 1}
              <label class="field">
                <span>{i18n.t("video.camera")}</span>
                <select value={studio.cameraId ?? ""} disabled={studio.phase !== "idle"} onchange={(e) => studio.openCamera(e.currentTarget.value)}>
                  {#each studio.cameras as c (c.id)}
                    <option value={c.id}>{c.label}</option>
                  {/each}
                </select>
              </label>
            {/if}
            {@render formatPicker()}
            <ul class="facts">
              <li><Icon name="music" size={14} />{i18n.t("video.factMusic")}</li>
              <li><Icon name="mic" size={14} />{app.armedCount > 0 ? i18n.t("video.factTakes", { n: app.armedCount }) : i18n.t("video.factNoTakes")}</li>
              <li><Icon name="click" size={14} />{i18n.t("video.factNoClick")}</li>
            </ul>
            <button
              class="rec"
              class:on={studio.phase === "recording"}
              disabled={!studio.stream || busy || !app.canPlay}
              onclick={() => (studio.phase === "recording" ? studio.stop() : studio.start())}
            >
              <Icon name={studio.phase === "recording" ? "stop" : "record"} size={18} fill />
              {studio.phase === "recording" ? i18n.t("record.stop") : studio.phase === "saving" ? i18n.t("video.saving") : i18n.t("video.record")}
            </button>
            <p class="hint">{i18n.t("video.recordHint")}</p>
          {/if}
        </aside>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(5 6 10 / 72%);
    backdrop-filter: blur(6px);
  }
  .studio {
    width: min(1240px, 100%);
    max-height: 100%;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px;
    overflow: auto;
    border-radius: 20px;
    background: var(--bg-panel);
    border: 1px solid var(--line);
    box-shadow: 0 30px 80px rgb(0 0 0 / 55%);
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  header > div {
    flex: 1;
  }
  h2 {
    margin: 0;
    font-size: 19px;
  }
  header p {
    margin: 2px 0 0;
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    border-radius: 12px;
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 18px;
  }
  .stage video,
  .live {
    width: 100%;
    aspect-ratio: 16 / 9;
    border-radius: 14px;
    background: #000;
  }
  .live {
    position: relative;
    overflow: hidden;
  }
  .live video {
    width: 100%;
    height: 100%;
    object-fit: cover;
    transform: scaleX(-1); /* mirror, like a selfie camera; exports are not mirrored */
  }
  .guide {
    position: absolute;
    border: 2px dashed rgb(255 255 255 / 55%);
    border-radius: 6px;
    pointer-events: none;
  }
  .guide span {
    position: absolute;
    top: 6px;
    left: 6px;
    font-size: 10.5px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 999px;
    background: rgb(0 0 0 / 55%);
  }
  .rec-badge {
    position: absolute;
    top: 14px;
    right: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border-radius: 999px;
    font-family: var(--mono);
    font-size: 12px;
    font-weight: 800;
    color: #fff;
    background: var(--danger);
  }
  .rec-badge i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #fff;
    animation: blink 1s steps(2) infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.2;
    }
  }
  .cam-error,
  .error {
    color: var(--accent);
    font-size: 13px;
  }
  .cam-error {
    position: absolute;
    inset: auto 16px 16px;
    padding: 10px 12px;
    border-radius: 10px;
    background: rgb(0 0 0 / 65%);
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h3 {
    margin: 4px 0 0;
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--accent);
  }
  .hint,
  .note {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-dim);
  }
  .note {
    color: var(--text-faint);
  }
  .facts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .facts li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .rec {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 48px;
    border-radius: 999px;
    font-size: 14px;
    font-weight: 800;
    color: #fff;
    background: var(--danger);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--danger) 35%, transparent);
  }
  .rec.on {
    background: #2a1014;
    color: var(--danger);
    border: 2px solid var(--danger);
  }
  .rec:disabled {
    opacity: 0.45;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-faint);
  }
  .formats {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .fmt {
    display: grid;
    grid-template-columns: auto 1fr;
    grid-template-rows: auto auto;
    column-gap: 8px;
    align-items: center;
    padding: 8px;
    border-radius: 10px;
    cursor: pointer;
    background: var(--bg-active);
    border: 1px solid transparent;
  }
  .fmt.on {
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
    background: color-mix(in srgb, var(--accent) 8%, var(--bg-active));
  }
  .fmt input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .shape {
    grid-row: span 2;
    width: 22px;
    max-height: 30px;
    border-radius: 3px;
    border: 2px solid var(--text-faint);
  }
  .fmt.on .shape {
    border-color: var(--accent);
  }
  .fmt-name {
    font-size: 12px;
    font-weight: 700;
  }
  .fmt-size {
    font-size: 10.5px;
    color: var(--text-faint);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
  }
  .sync {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .sync input {
    flex: 1;
    accent-color: var(--accent);
  }
  .sync span {
    width: 64px;
    text-align: right;
    font-size: 12px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .progress {
    height: 6px;
    border-radius: 3px;
    background: var(--line);
    overflow: hidden;
  }
  .progress div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .saved {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
  }
  .saved li {
    display: flex;
    gap: 8px;
    justify-content: space-between;
  }
  .saved span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link {
    color: var(--accent);
    text-decoration: underline;
    background: none;
    font-size: 12px;
  }
  /* Compact floating window while filming. */
  .backdrop.mini {
    inset: auto 20px 20px auto;
    padding: 0;
    background: none;
    backdrop-filter: none;
    pointer-events: none;
  }
  .studio.mini {
    width: 340px;
    gap: 8px;
    padding: 10px;
    pointer-events: auto;
    border-color: color-mix(in srgb, var(--danger) 60%, transparent);
  }
  .mini header p,
  .mini header .icon-btn,
  .mini :global(h3),
  .mini .formats,
  .mini .check,
  .mini .facts,
  .mini .hint,
  .mini .field,
  .mini .guide span {
    display: none;
  }
  .mini h2 {
    font-size: 13px;
  }
  .mini .badge {
    width: 28px;
    height: 28px;
    border-radius: 9px;
  }
  .mini .body {
    grid-template-columns: 1fr;
    gap: 8px;
  }
  .mini .live {
    border-radius: 10px;
  }
  .mini .rec {
    height: 38px;
    font-size: 13px;
  }
  .mini .rec-badge {
    top: 8px;
    right: 8px;
    font-size: 11px;
  }
  @media (max-width: 1000px) {
    .body {
      grid-template-columns: 1fr;
    }
  }
</style>
