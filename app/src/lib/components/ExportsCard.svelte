<script lang="ts">
  // Sidebar status of exports running in the background: the audio mixdown
  // and the video studio's clips (with a live thumbnail of the clip being
  // encoded — it also keeps WebKit from pausing that video).
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import { studio } from "../video/studio.svelte";
  import Icon from "./Icon.svelte";

  const exporting = $derived(studio.phase === "exporting");
  const finished = $derived(!exporting && (studio.saved.length > 0 || studio.exportError !== null));
  const stageText = $derived(
    studio.exportStage === "audio"
      ? i18n.t("video.stage.audio")
      : studio.exportStage === "saving"
        ? i18n.t("video.stage.saving")
        : i18n.t("video.stage.encoding", { p: Math.round(studio.progress * 100) }),
  );

  /** Shows the element being encoded inside this card. If the card goes
   *  away mid-export, the element is parked visibly instead of removed. */
  function host(node: HTMLElement, video: HTMLVideoElement | null) {
    const place = (v: HTMLVideoElement | null) => {
      if (v && v.parentElement !== node) {
        v.classList.remove("compose-video-parked");
        node.append(v);
      }
    };
    place(video);
    return {
      update: place,
      destroy() {
        const v = node.querySelector("video");
        if (v && studio.phase === "exporting") {
          v.classList.add("compose-video-parked");
          document.body.append(v);
        }
      },
    };
  }
</script>

{#if app.exportProgress !== null}
  <div class="card">
    <div class="row">
      <Icon name="export" size={13} />
      <span class="name">{i18n.t("export.running")}</span>
      <span class="pct tabular">{Math.round(app.exportProgress * 100)}%</span>
    </div>
    <div class="bar"><div style={`width:${app.exportProgress * 100}%`}></div></div>
  </div>
{/if}

{#if exporting || finished}
  <div class="card video" class:err={!!studio.exportError}>
    <div class="row">
      <Icon name="video" size={13} />
      <span class="name">{i18n.t("video.exportingShort", { n: studio.exportFormats.length })}</span>
      {#if exporting}
        <button class="icon-btn small" title={i18n.t("jobs.cancel")} aria-label={i18n.t("jobs.cancel")} onclick={() => studio.cancel()}>
          <Icon name="close" size={12} />
        </button>
      {:else}
        <button class="icon-btn small" title={i18n.t("jobs.dismiss")} aria-label={i18n.t("jobs.dismiss")} onclick={() => studio.dismissExport()}>
          <Icon name="close" size={12} />
        </button>
      {/if}
    </div>
    {#if exporting}
      <div class="thumb" use:host={studio.composeVideo}></div>
      <span class="stage">{stageText}</span>
      <div class="bar"><div style={`width:${studio.progress * 100}%`}></div></div>
    {:else if studio.exportError}
      <span class="stage err">{studio.exportError}</span>
    {:else}
      <span class="stage ok">✓ {i18n.t("video.exported", { n: studio.saved.length })}</span>
      <div class="links">
        <button class="link" onclick={() => app.revealExport(studio.saved[0])}>{i18n.t("export.show")}</button>
        <button class="link" onclick={() => (app.videoOpen = true)}>{i18n.t("video.openStudio")}</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0 12px 8px;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--bg-raised);
    border: 1px solid var(--line);
  }
  .card.video {
    border-color: color-mix(in srgb, var(--danger) 35%, var(--line));
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-dim);
  }
  .name {
    flex: 1;
    min-width: 0;
    font-size: var(--fs-body);
    font-weight: 700;
    line-height: 1.3;
    color: var(--text);
    overflow-wrap: break-word;
  }
  .pct {
    font-size: var(--fs-small);
  }
  .thumb {
    border-radius: 8px;
    overflow: hidden;
    background: #000;
    aspect-ratio: 16 / 9;
  }
  @media (max-height: 760px) {
    .thumb {
      width: 60%;
    }
  }
  .thumb :global(video) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .stage {
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .stage.ok {
    color: var(--ok);
  }
  .stage.err {
    color: var(--danger);
  }
  .bar {
    height: 3px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 0.25s;
  }
  .links {
    display: flex;
    gap: 12px;
  }
  .link {
    padding: 0;
    font-size: var(--fs-small);
    color: var(--accent);
    text-decoration: underline;
    background: none;
  }
  /* Fallback when the sidebar card is not mounted: small, but visible. */
  :global(.compose-video-parked) {
    position: fixed;
    left: 12px;
    bottom: 12px;
    z-index: 60;
    width: 160px;
    border-radius: 8px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 50%);
  }
</style>
