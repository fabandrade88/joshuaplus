<script lang="ts">
  // The mixing desk. The song's stems form one group that can be collapsed
  // into a single "Song" strip, leaving room for recording channels later.
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import { STEM_COLORS, stemLabelKey } from "../stems";
  import ChannelStrip from "./ChannelStrip.svelte";
  import ClickPedal from "./ClickPedal.svelte";
  import MeterStrip from "./MeterStrip.svelte";
  import MidiStrip from "./MidiStrip.svelte";
  import Icon from "./Icon.svelte";

  const project = $derived(app.song?.project);
  const takes = $derived(project?.takes ?? []);
  const anySolo = $derived((project?.tracks.some((t) => t.solo) ?? false) || takes.some((t) => t.solo));
  const masterLevel = $derived(Math.max(app.masterMeter[0], app.masterMeter[1]));
</script>

{#if project}
  <section class="console">
    <div class="scroller">
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

      {#if app.inputs.length > 0 || app.drums?.connected}
        <div class="group">
          <header>
            <span class="group-name">{i18n.t("gear.inputs")}</span>
            <button class="toggle" onclick={() => app.openGear()}><Icon name="plug" size={13} /></button>
          </header>
          <div class="strips">
            {#each app.inputs as input (input.index)}
              {#if input.directMonitor}
                <MeterStrip
                  kind="other"
                  name={input.name}
                  color="var(--ok)"
                  tag={i18n.t("console.direct")}
                  level={app.inputLevels[input.index] ?? 0}
                  note={i18n.t("gear.directMonitor")}
                  noteTitle={i18n.t("gear.directMonitorHint")}
                  onnote={() => app.openGear()}
                  armed={input.canRecord ? input.armed : undefined}
                  armTitle={i18n.t("record.arm")}
                  onarm={() => app.toggleArm(input.index)}
                />
              {:else}
              <ChannelStrip
                kind="other"
                name={input.name}
                color="var(--ok)"
                gainDb={input.gainDb}
                muted={!input.monitor || !input.monitorOk}
                level={app.inputLevels[input.index] ?? 0}
                armed={input.canRecord ? input.armed : undefined}
                onarm={() => app.toggleArm(input.index)}
                onchange={(p) => {
                  if (p.gainDb !== undefined) void app.updateInput(input.index, { gainDb: p.gainDb });
                  if (p.muted !== undefined && input.monitorOk) void app.updateInput(input.index, { monitor: !p.muted });
                }}
              />
              {/if}
            {/each}
            {#if app.drums?.connected}<MidiStrip />{/if}
          </div>
        </div>
      {/if}

      {#if takes.length > 0}
        <div class="group takes">
          <header>
            <span class="group-name">{i18n.t("record.takes")}</span>
          </header>
          <div class="strips">
            {#each takes as take, j (take.id)}
              <ChannelStrip
                kind="take"
                name={take.name}
                color="var(--danger)"
                gainDb={take.gainDb}
                muted={take.muted}
                solo={take.solo}
                pan={take.pan}
                silenced={anySolo && !take.solo}
                level={app.takeMeter(j)}
                ondelete={() => {
                  if (window.confirm(i18n.t("record.confirmDelete", { name: take.name }))) void app.deleteTake(take.id);
                }}
                onchange={(patch) => app.updateTake(take.id, patch)}
              />
            {/each}
          </div>
        </div>
      {/if}
    </div>
    <!-- Metronome and master stay pinned; the channel groups scroll. -->
    <div class="pinned">
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
    </div>
  </section>
{/if}

<style>
  .console {
    flex: 1;
    min-height: 380px;
    display: flex;
    align-items: stretch;
    gap: 10px;
    padding: 12px 12px 14px;
    overflow: hidden;
    border-top: 1px solid var(--line);
    background:
      linear-gradient(180deg, rgb(255 255 255 / 2%), transparent 30%),
      #0d0f15;
  }
  .scroller {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: stretch;
    gap: 10px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
    scrollbar-color: var(--line-strong) transparent;
  }
  .pinned {
    flex: none;
    display: flex;
    align-items: stretch;
    gap: 10px;
    padding-left: 10px;
    border-left: 1px solid var(--line);
  }
  .group {
    flex: none;
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
  .takes .group-name {
    color: var(--danger);
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
</style>
