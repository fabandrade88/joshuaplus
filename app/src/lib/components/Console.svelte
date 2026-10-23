<script lang="ts">
  // The mixing desk. The song's stems form one group that can be collapsed
  // into a single "Song" strip, leaving room for recording channels later.
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import { STEM_COLORS, stemLabelKey } from "../stems";
  import ChannelStrip from "./ChannelStrip.svelte";
  import ClickPedal from "./ClickPedal.svelte";
  import Icon from "./Icon.svelte";

  const project = $derived(app.song?.project);
  const anySolo = $derived(project?.tracks.some((t) => t.solo) ?? false);
  const masterLevel = $derived(Math.max(app.masterMeter[0], app.masterMeter[1]));
</script>

{#if project}
  <section class="console">
    <div class="group" class:collapsed={app.songCollapsed}>
      <header>
        <span class="group-name">{i18n.t("console.song")}</span>
        <button class="toggle" onclick={() => app.toggleSongCollapsed()}>
          <Icon name={app.songCollapsed ? "plus" : "back"} size={13} />
          {app.songCollapsed ? i18n.t("console.expand") : i18n.t("console.collapse")}
        </button>
      </header>
      <div class="strips">
        <ChannelStrip
          kind="song"
          name={i18n.t("console.song")}
          color="var(--accent)"
          wide
          gainDb={project.songGainDb ?? 0}
          muted={project.songMuted ?? false}
          level={app.songMeter}
          onchange={(p) => app.updateSongBus(p.gainDb ?? null, p.muted ?? null)}
        />
        {#if !app.songCollapsed}
          <div class="divider"></div>
          {#each project.tracks as track, i (track.stem)}
            <ChannelStrip
              kind={track.stem}
              name={i18n.t(stemLabelKey(track.stem))}
              color={STEM_COLORS[track.stem]}
              gainDb={track.gainDb}
              muted={track.muted}
              solo={track.solo}
              pan={track.pan}
              silenced={anySolo && !track.solo}
              level={app.meters[i] ?? 0}
              onchange={(patch) => app.updateTrack(i, patch)}
            />
          {/each}
        {/if}
      </div>
    </div>

    {#if app.inputs.length > 0}
      <div class="group">
        <header>
          <span class="group-name">{i18n.t("gear.inputs")}</span>
          <button class="toggle" onclick={() => app.openGear()}><Icon name="plug" size={13} /></button>
        </header>
        <div class="strips">
          {#each app.inputs as input (input.index)}
            <ChannelStrip
              kind="other"
              name={input.name}
              color="var(--ok)"
              gainDb={input.gainDb}
              muted={!input.monitor || !input.monitorOk}
              level={app.inputLevels[input.index] ?? 0}
              onchange={(p) => {
                if (p.gainDb !== undefined) void app.updateInput(input.index, { gainDb: p.gainDb });
                if (p.muted !== undefined && input.monitorOk) void app.updateInput(input.index, { monitor: !p.muted });
              }}
            />
          {/each}
        </div>
      </div>
    {/if}

    <div class="spacer"></div>
    <ClickPedal />
    <ChannelStrip
      kind="master"
      name={i18n.t("console.master")}
      color="var(--text)"
      wide
      muteable={false}
      gainDb={project.masterGainDb}
      muted={false}
      level={masterLevel}
      onchange={(p) => p.gainDb !== undefined && app.setMasterGain(p.gainDb)}
    />
  </section>
{/if}

<style>
  .console {
    flex: 1;
    min-height: 410px;
    display: flex;
    align-items: stretch;
    gap: 10px;
    padding: 12px 12px 14px;
    overflow-x: auto;
    border-top: 1px solid var(--line);
    background:
      linear-gradient(180deg, rgb(255 255 255 / 2%), transparent 30%),
      #0d0f15;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px;
    border-radius: 16px;
    background: #12151d;
    border: 1px solid var(--line);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 4px;
  }
  .group-name {
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--accent);
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-faint);
    white-space: nowrap;
  }
  .toggle:hover {
    color: var(--text);
  }
  .strips {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 5px;
  }
  .divider {
    width: 1px;
    margin: 6px 2px;
    background: var(--line-strong);
  }
  .spacer {
    flex: 1;
  }
</style>
