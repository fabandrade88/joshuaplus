<script lang="ts">
  // The clip's sound, editable while reviewing a video: the song's tracks and
  // the takes with volume, mute and room. It is the song's own mix (the
  // console changes with it); every change re-renders the preview sound and
  // the export uses exactly what was heard.
  import { app } from "../app.svelte";
  import { formatDb } from "../format";
  import { i18n } from "../i18n/index.svelte";
  import { STEM_COLORS, stemLabelKey } from "../stems";
  import type { StemKind } from "../types";
  import { studio } from "../video/studio.svelte";
  import StemIcon from "./StemIcon.svelte";

  let { disabled = false }: { disabled?: boolean } = $props();

  const MIN_DB = -40;
  const MAX_DB = 6;

  const project = $derived(app.song?.project);
  const takes = $derived(project?.takes ?? []);

  type Row = {
    key: string;
    kind: StemKind | "song" | "take";
    name: string;
    color: string;
    gainDb: number;
    muted: boolean;
    room?: number;
    set: (patch: { gainDb?: number; muted?: boolean; room?: number }) => void;
  };

  const songRows = $derived.by((): Row[] => {
    if (!project) return [];
    const out: Row[] = [
      {
        key: "song",
        kind: "song",
        name: i18n.t("video.mixWholeSong"),
        color: "var(--accent)",
        gainDb: project.songGainDb ?? 0,
        muted: project.songMuted ?? false,
        set: (p) => void app.updateSongBus(p.gainDb ?? null, p.muted ?? null),
      },
    ];
    project.tracks.forEach((t, i) =>
      out.push({
        key: t.stem,
        kind: t.stem,
        name: i18n.t(stemLabelKey(t.stem)),
        color: STEM_COLORS[t.stem],
        gainDb: t.gainDb,
        muted: t.muted,
        set: (p) => void app.updateTrack(i, p),
      }),
    );
    return out;
  });

  const takeRows = $derived.by((): Row[] => {
    const out: Row[] = [];
    for (const t of takes) {
      out.push({
        key: t.id,
        kind: "take",
        name: t.name,
        color: "var(--danger)",
        gainDb: t.gainDb,
        muted: t.muted,
        room: t.room ?? 0.3,
        set: (p) => void app.updateTake(t.id, p),
      });
    }
    return out;
  });

  function change(row: Row, patch: { gainDb?: number; muted?: boolean; room?: number }): void {
    row.set(patch);
    studio.mixChanged();
  }

  const dbFromInput = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);
</script>

{#if project}
  <section class="mix" aria-labelledby="video-mix-title">
    <header>
      <h3 id="video-mix-title">{i18n.t("video.mixTitle")}</h3>
      <p>{i18n.t("video.mixHint")}</p>
    </header>
    {@render group(i18n.t("console.song"), songRows)}
    {#if takeRows.length > 0}
      {@render group(i18n.t("record.takes"), takeRows)}
    {/if}
  </section>
{/if}

{#snippet group(title: string, rows: Row[])}
  <div class="group">
    <h4 class="section-title">{title}</h4>
    <ul>
      {#each rows as row (row.key)}
        <li class:off={row.muted} class:song={row.kind === "song"} style={`--c:${row.color}`}>
          <span class="icon"><StemIcon kind={row.kind} size={15} /></span>
          <span class="name" title={row.name}>{row.name}</span>
          <button
            class="mute"
            class:on={row.muted}
            aria-pressed={row.muted}
            title={i18n.t("track.mute")}
            {disabled}
            onclick={() => change(row, { muted: !row.muted })}>M</button
          >
          <input
            type="range"
            min={MIN_DB}
            max={MAX_DB}
            step="0.5"
            value={Math.max(MIN_DB, row.gainDb)}
            aria-label={`${row.name} · ${i18n.t("track.volume")}`}
            {disabled}
            oninput={(e) => {
              const db = dbFromInput(e);
              change(row, { gainDb: db <= MIN_DB ? -96 : db });
            }}
            ondblclick={() => change(row, { gainDb: 0 })}
          />
          <span class="db tabular">{formatDb(row.gainDb)}</span>
          {#if row.room !== undefined}
            <label class="room" title={i18n.t("track.roomHint")}>
              <span>{i18n.t("track.room")}</span>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={row.room}
                aria-label={`${row.name} · ${i18n.t("track.room")}`}
                {disabled}
                oninput={(e) => change(row, { room: dbFromInput(e) })}
              />
              <span class="tabular">{Math.round(row.room * 100)}%</span>
            </label>
          {/if}
        </li>
      {/each}
    </ul>
  </div>
{/snippet}

<style>
  .mix {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
  }
  header h3 {
    margin: 0;
    font-size: var(--fs-emph);
  }
  header p {
    margin: 2px 0 0;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h4 {
    margin: 4px 0 0;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(330px, 1fr));
    gap: 4px 12px;
    align-items: start;
  }
  li {
    display: grid;
    grid-template-columns: 20px minmax(60px, 1fr) 28px minmax(80px, 1.4fr) 52px;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius-s);
    background: var(--bg-raised);
  }
  li.song {
    grid-column: 1 / -1;
  }
  li.off {
    opacity: 0.55;
  }
  .icon {
    display: grid;
    place-items: center;
    color: var(--c);
  }
  .name {
    font-size: var(--fs-small);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mute {
    width: 28px;
    height: 22px;
    border-radius: 6px;
    font-size: var(--fs-label);
    font-weight: 800;
    color: var(--text-dim);
    background: var(--bg-hover);
  }
  .mute.on {
    color: #fff;
    background: var(--danger);
  }
  input[type="range"] {
    width: 100%;
    min-width: 0;
    accent-color: var(--c);
  }
  .db {
    font-family: var(--mono);
    font-size: 11px; /* dense readout, like the console strips */
    color: var(--text-dim);
    text-align: right;
  }
  .room {
    grid-column: 2 / -1;
    display: grid;
    grid-template-columns: auto minmax(80px, 1fr) 40px;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .room input {
    accent-color: var(--accent);
  }
  .room .tabular {
    font-family: var(--mono);
    font-size: 11px; /* dense readout */
    text-align: right;
  }
</style>
