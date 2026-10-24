<script lang="ts">
  // Shown in the sidebar while browsing other screens with a song loaded:
  // the song keeps playing; one click returns to it.
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import { barBeatAt } from "../timeline";
  import Icon from "./Icon.svelte";

  const project = $derived(app.song?.project);
  const playing = $derived(app.transport.playing || app.transport.countingIn);
  const bb = $derived(project ? barBeatAt(project.analysis, app.transport.positionSecs) : null);
  const progress = $derived(
    project && project.durationSecs > 0 ? (app.transport.positionSecs / project.durationSecs) * 100 : 0,
  );
</script>

{#if project && app.view.kind !== "song"}
  <div class="mini" class:playing>
    <button class="back" title={i18n.t("mini.return")} onclick={() => app.showSong()}>
      <span class="art">
        {#if app.song?.cover}<img src={app.song.cover} alt="" />{:else}<Icon name="music" size={16} />{/if}
        {#if playing}<span class="eq" aria-hidden="true"><i></i><i></i><i></i></span>{/if}
      </span>
      <span class="text">
        <span class="title">{project.title}</span>
        <span class="sub tabular">
          {playing ? i18n.t("mini.playing") : i18n.t("mini.paused")}{bb ? ` · ${i18n.t("song.bar")} ${bb.bar}` : ""}
        </span>
      </span>
    </button>
    <button
      class="toggle"
      aria-label={playing ? i18n.t("transport.pause") : i18n.t("transport.play")}
      title={playing ? i18n.t("transport.pause") : i18n.t("transport.play")}
      disabled={!app.canPlay}
      onclick={() => app.togglePlay()}
    >
      <Icon name={playing ? "pause" : "play"} size={14} fill />
    </button>
    <div class="bar"><div style={`width:${progress}%`}></div></div>
  </div>
{/if}

<style>
  .mini {
    position: relative;
    margin: 8px 12px 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 10px;
    border-radius: var(--radius);
    background: var(--bg-panel);
    border: 1px solid var(--line-strong);
    overflow: hidden;
  }
  .mini.playing {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--line-strong));
  }
  .back {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
  }
  .art {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 7px;
    overflow: hidden;
    background: var(--bg-active);
    color: var(--accent);
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .eq {
    position: absolute;
    inset: auto 0 0 0;
    height: 14px;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    gap: 2px;
    background: linear-gradient(transparent, rgb(0 0 0 / 70%));
  }
  .eq i {
    width: 3px;
    background: var(--accent);
    animation: eq 0.8s ease-in-out infinite alternate;
  }
  .eq i:nth-child(2) {
    animation-delay: -0.3s;
  }
  .eq i:nth-child(3) {
    animation-delay: -0.6s;
  }
  @keyframes eq {
    from {
      height: 3px;
    }
    to {
      height: 11px;
    }
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .toggle {
    flex: none;
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-ink);
  }
  .toggle:disabled {
    opacity: 0.4;
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: var(--line);
  }
  .bar div {
    height: 100%;
    background: var(--accent);
  }
</style>
