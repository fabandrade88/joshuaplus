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
  import VideoMix from "./VideoMix.svelte";

  let live = $state<HTMLVideoElement | null>(null);
  let review = $state<HTMLVideoElement | null>(null);
  let frameW = $state(0);
  let frameH = $state(0);

  const OVERLAY_PARTS = ["cover", "title", "artist"] as const;

  const LABELS: Record<FormatId, MessageKey> = {
    youtube: "video.fmt.youtube",
    vertical: "video.fmt.vertical",
    square: "video.fmt.square",
    portrait: "video.fmt.portrait",
  };

  $effect(() => {
    if (live) live.srcObject = studio.stream;
  });

  // The camera clip is silent: every play of the review video gets the mix.
  $effect(() => {
    if (review) return studio.attachReview(review);
  });

  /** Starts the export in the background and closes the studio; progress
   *  shows in the sidebar. (Starts inside the click so audio may play.) */
  function exportAndClose(): void {
    void studio.export();
    app.videoOpen = false;
  }

  onMount(() => {
    if (!studio.supported) return;
    // A clip of another song that was already exported: start fresh.
    if (studio.phase === "review" && studio.clipFromOtherSong && studio.saved.length > 0) studio.discardClip();
    // Reopened while an export runs or a clip waits: no camera needed yet
    // (discarding the clip opens it).
    if (studio.phase === "idle") void studio.openCamera();
    return studio.watchCameras();
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
  /** The window can close at any time except mid-take: an export keeps
   *  running in the background (its progress shows in the sidebar). */
  const canClose = $derived(studio.phase !== "recording" && studio.phase !== "starting" && studio.phase !== "saving");
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
  <h3 id="overlay-heading">{i18n.t("video.overlay")}</h3>
  <div class="overlay-opts" role="group" aria-labelledby="overlay-heading">
    {#each OVERLAY_PARTS as part (part)}
      <label class="opt" class:on={studio.overlay[part]}>
        <input
          type="checkbox"
          checked={studio.overlay[part]}
          disabled={studio.phase === "exporting"}
          onchange={(e) => studio.setOverlay(part, e.currentTarget.checked)}
        />
        {i18n.t(`video.overlay.${part}`)}
      </label>
    {/each}
  </div>
{/snippet}

<svelte:window onkeydown={(e) => e.key === "Escape" && canClose && void close()} />

<div class="backdrop" class:mini role="presentation">
  <div class="studio" class:mini role="dialog" aria-modal={!mini} tabindex="-1" aria-label={i18n.t("video.title")}>
    <header>
      <span class="badge"><Icon name="video" size={18} /></span>
      <div>
        <h2>{i18n.t("video.title")}</h2>
        <p>{i18n.t("video.subtitle")}</p>
      </div>
      <button class="icon-btn" title={i18n.t("common.close")} aria-label={i18n.t("common.close")} disabled={!canClose} onclick={() => close()}>
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
            <video bind:this={review} src={studio.clipUrl} controls playsinline onpointerdown={() => studio.unlockAudio()}></video>
            {#if studio.reviewAudio === "loading"}<p class="sound-state">{i18n.t("video.soundLoading")}</p>{/if}
            {#if studio.reviewAudio === "error"}<p class="sound-state err">{i18n.t("video.soundError")}</p>{/if}
            {#if studio.clipFromOtherSong && studio.phase === "review" && studio.clip}
              {@const from = studio.clip.song}
              <div class="other-song" role="status">
                <p>{i18n.t("video.otherSong", { title: from.title })}</p>
                <div class="other-actions">
                  <button class="btn" onclick={() => app.openSong(from.id)}>{i18n.t("video.backToSong", { title: from.title })}</button>
                  <button class="btn" onclick={() => studio.discardClip()}>{i18n.t("video.discardForThis")}</button>
                </div>
              </div>
            {:else}
              <VideoMix disabled={studio.phase === "exporting"} />
            {/if}
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
              {#if studio.phase === "idle" && (studio.overlay.cover || studio.overlay.title || studio.overlay.artist)}
                <!-- A faint sketch of what the export burns in. -->
                <div class="overlay-mock" aria-hidden="true">
                  {#if studio.overlay.cover}
                    {#if app.song?.cover}<img src={app.song.cover} alt="" />{:else}<span class="mock-cover"></span>{/if}
                  {/if}
                  {#if studio.overlay.title || studio.overlay.artist}
                    <span class="mock-text">
                      <i></i>
                      {#if studio.overlay.title}<b>{app.song?.project.title ?? ""}</b>{/if}
                      {#if studio.overlay.artist}<small>{app.song?.project.metadata?.artist ?? ""}</small>{/if}
                    </span>
                  {/if}
                </div>
              {/if}
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
              <button class="btn primary" disabled={studio.selectedFormats.length === 0 || studio.clipFromOtherSong} onclick={exportAndClose}>
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
            <div class="field">
              <label for="studio-camera">{i18n.t("video.camera")}</label>
              <div class="camera-row">
                <select
                  id="studio-camera"
                  value={studio.cameraId ?? ""}
                  disabled={studio.phase !== "idle" || studio.cameras.length === 0}
                  onchange={(e) => studio.chooseCamera(e.currentTarget.value)}
                >
                  {#if studio.cameras.length === 0}<option value="">{i18n.t("video.noCameras")}</option>{/if}
                  {#each studio.cameras as c (c.id)}
                    <option value={c.id}>{c.label}</option>
                  {/each}
                </select>
                <button
                  class="icon-btn"
                  title={i18n.t("video.findCameras")}
                  aria-label={i18n.t("video.findCameras")}
                  disabled={studio.phase !== "idle"}
                  onclick={() => studio.openCamera(studio.cameraId)}
                >
                  <Icon name="loop" size={15} />
                </button>
              </div>
            </div>
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
  }
  .studio {
    width: min(1240px, 100%);
    max-height: 100%;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px;
    overflow: auto;
    border-radius: var(--radius);
    background: var(--bg-panel);
    border: 1px solid var(--line);
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
    font-size: var(--fs-title);
  }
  header p {
    margin: 2px 0 0;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    border-radius: var(--radius);
    color: var(--danger);
    background: var(--bg-active);
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
    border-radius: var(--radius);
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
    border-radius: var(--radius-s);
    pointer-events: none;
  }
  .guide span {
    position: absolute;
    top: 6px;
    left: 6px;
    font-size: var(--fs-small);
    font-weight: 600;
    padding: 2px 6px;
    border-radius: var(--radius);
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
    border-radius: var(--radius);
    font-family: var(--mono);
    font-size: var(--fs-small);
    font-weight: 600;
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
  .sound-state {
    margin: 6px 0 0;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .sound-state.err {
    color: var(--danger);
  }
  .cam-error,
  .error {
    color: var(--accent);
    font-size: var(--fs-body);
  }
  .cam-error {
    position: absolute;
    inset: auto 16px 16px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: rgb(0 0 0 / 65%);
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h3 {
    margin: 4px 0 0;
    font-size: var(--fs-label);
    font-weight: 600;
    color: var(--accent);
  }
  .hint,
  .note {
    margin: 0;
    font-size: var(--fs-small);
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
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .facts {
    line-height: 1.45;
  }
  .facts li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .facts li :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .rec {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 48px;
    border-radius: var(--radius);
    font-size: var(--fs-body);
    font-weight: 600;
    color: #fff;
    background: var(--danger);
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
    font-size: var(--fs-label);
    font-weight: 600;
    color: var(--text-faint);
  }
  .other-song {
    margin-top: 12px;
    padding: 12px 14px;
    border-radius: var(--radius);
    border: 1px solid var(--line-strong);
    background: var(--bg-active);
  }
  .other-song p {
    margin: 0 0 10px;
    font-size: var(--fs-body);
  }
  .other-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .camera-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .camera-row select {
    flex: 1;
    min-width: 0;
  }
  .formats {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  /* One row per format: long names ("Shorts · Reels · TikTok 9:16") get the
     full width instead of wrapping in a half-width card. */
  .fmt {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) auto;
    column-gap: 8px;
    align-items: center;
    min-height: 34px;
    padding: 5px 10px 5px 8px;
    border-radius: var(--radius);
    cursor: pointer;
    background: var(--bg-active);
    border: 1px solid transparent;
  }
  .fmt:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .fmt.on {
    border-color: color-mix(in srgb, var(--accent) 60%, transparent);
    background: var(--bg-active);
  }
  .fmt input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .shape {
    justify-self: center;
    height: 16px;
    max-width: 28px;
    border-radius: 3px;
    border: 2px solid var(--text-faint);
  }
  .fmt.on .shape {
    border-color: var(--accent);
  }
  .fmt-name {
    min-width: 0;
    font-size: var(--fs-small);
    font-weight: 600;
    line-height: 1.3;
  }
  .fmt-size {
    font-size: var(--fs-small);
    color: var(--text-faint);
    white-space: nowrap;
  }
  .overlay-opts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .opt {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 28px;
    padding: 0 10px 0 8px;
    border-radius: var(--radius);
    font-size: var(--fs-small);
    font-weight: 600;
    color: var(--text-dim);
    background: var(--bg-active);
    border: 1px solid var(--line-strong);
    cursor: pointer;
  }
  .opt.on {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    background: var(--bg-active);
  }
  .opt input {
    margin: 0;
    accent-color: var(--accent);
  }
  .opt:has(input:disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .overlay-mock {
    position: absolute;
    left: 4%;
    bottom: 6%;
    max-width: 60%;
    display: flex;
    align-items: flex-end;
    gap: 8px;
    opacity: 0.6;
    pointer-events: none;
  }
  .overlay-mock img,
  .mock-cover {
    width: 44px;
    height: 44px;
    flex: none;
    border-radius: var(--radius-s);
    object-fit: cover;
    background: rgb(255 255 255 / 25%);
  }
  .mock-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    color: #fff;
  }
  .mock-text i {
    width: 18px;
    height: 3px;
    margin-bottom: 4px;
    background: var(--accent);
  }
  .mock-text b,
  .mock-text small {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mock-text b {
    font-size: var(--fs-emph);
  }
  .mock-text small {
    font-size: var(--fs-small);
    opacity: 0.8;
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
    font-size: var(--fs-small);
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
    font-size: var(--fs-small);
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
    font-size: var(--fs-small);
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
  .mini .overlay-opts,
  .mini .facts,
  .mini .hint,
  .mini .field,
  .mini .guide span {
    display: none;
  }
  .mini h2 {
    font-size: var(--fs-body);
  }
  .mini .badge {
    width: 28px;
    height: 28px;
    border-radius: var(--radius);
  }
  .mini .body {
    grid-template-columns: 1fr;
    gap: 8px;
  }
  .mini .live {
    border-radius: var(--radius);
  }
  .mini .rec {
    height: 38px;
    font-size: var(--fs-body);
  }
  .mini .rec-badge {
    top: 8px;
    right: 8px;
    font-size: var(--fs-small);
  }
  /* Keep the 16:9 preview short enough that the controls beside it stay on
     screen on small windows (960×600 minimum). */
  .studio:not(.mini) .stage video,
  .studio:not(.mini) .live {
    width: min(100%, calc((100vh - 190px) * 16 / 9));
    margin-inline: auto;
  }
  /* Reviewing: a smaller picture leaves the video's sound mix on screen. */
  .studio:not(.mini) .stage > video {
    width: min(100%, calc((100vh - 190px) * 0.5 * 16 / 9));
  }
  @media (max-width: 1100px), (max-height: 760px) {
    .backdrop:not(.mini) {
      padding: 12px;
    }
    .studio:not(.mini) {
      gap: 10px;
      padding: 14px;
    }
    .body {
      grid-template-columns: minmax(0, 1fr) 260px;
      gap: 14px;
    }
  }
  @media (max-width: 760px) {
    .body {
      grid-template-columns: 1fr;
    }
  }
</style>
