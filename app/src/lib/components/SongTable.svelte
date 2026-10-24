<script lang="ts">
  import { app } from "../app.svelte";
  import { formatBpm, formatTime } from "../format";
  import { i18n } from "../i18n/index.svelte";
  import type { MessageKey } from "../i18n/en";
  import { moveId } from "../playlist";
  import { engineOf } from "../stems";
  import type { SongSummary } from "../types";
  import Icon from "./Icon.svelte";

  let {
    songs,
    showFolder = true,
    numbered = false,
    onreorder = undefined,
  }: {
    songs: SongSummary[];
    showFolder?: boolean;
    /** Show the playlist position column. */
    numbered?: boolean;
    /** Enables drag-to-reorder; receives the new id order. */
    onreorder?: ((ids: string[]) => void) | undefined;
  } = $props();

  // Pointer-driven reordering (HTML5 drag & drop is unreliable inside the
  // Tauri webview because native file drops are intercepted).
  let tbody: HTMLTableSectionElement | undefined = $state();
  let dragId = $state<string | null>(null);
  let dropIndex = $state<number | null>(null);

  function indexAt(clientY: number): number {
    const rows = [...(tbody?.querySelectorAll<HTMLTableRowElement>("tr[data-id]") ?? [])];
    const i = rows.findIndex((r) => {
      const b = r.getBoundingClientRect();
      return clientY < b.top + b.height / 2;
    });
    return i === -1 ? rows.length : i;
  }

  function startDrag(e: PointerEvent, id: string): void {
    if (!onreorder || e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragId = id;
    dropIndex = indexAt(e.clientY);
  }

  function moveDrag(e: PointerEvent): void {
    if (dragId) dropIndex = indexAt(e.clientY);
  }

  function endDrag(): void {
    if (dragId && dropIndex !== null && onreorder) {
      const ids = songs.map((s) => s.id);
      const from = ids.indexOf(dragId);
      const to = dropIndex > from ? dropIndex - 1 : dropIndex;
      if (from >= 0 && to !== from) onreorder(moveId(ids, dragId, to));
    }
    dragId = null;
    dropIndex = null;
  }

  function keyMove(e: KeyboardEvent, id: string): void {
    if (!onreorder || !e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
    e.preventDefault();
    const ids = songs.map((s) => s.id);
    const to = ids.indexOf(id) + (e.key === "ArrowUp" ? -1 : 1);
    if (to < 0 || to >= ids.length) return;
    onreorder(moveId(ids, id, to));
  }

  // Inline rename.
  let renaming = $state<string | null>(null);
  let draft = $state("");

  function startRename(song: SongSummary): void {
    renaming = song.id;
    draft = song.title;
  }

  function commitRename(): void {
    const id = renaming;
    renaming = null;
    if (id) void app.renameSong(id, draft);
  }

  function focusSelect(node: HTMLInputElement): void {
    node.focus();
    node.select();
  }

  function confirmDelete(song: SongSummary): void {
    if (window.confirm(i18n.t("song.deleteConfirm", { title: song.title }))) void app.deleteSong(song.id);
  }
</script>

<table>
  <thead>
    <tr>
      {#if numbered}<th class="pos">#</th>{/if}
      <th class="title-col">{i18n.t("col.title")}</th>
      <th class="num">{i18n.t("song.bpm")}</th>
      <th class="num">{i18n.t("col.duration")}</th>
      <th>{i18n.t("col.engine")}</th>
      {#if showFolder}<th>{i18n.t("col.folder")}</th>{/if}
      <th class="actions"></th>
    </tr>
  </thead>
  <tbody bind:this={tbody}>
    {#each songs as song, index (song.id)}
      {@const engine = engineOf(song.separator)}
      {@const folder = app.folderById(song.folder)}
      <tr
        data-id={song.id}
        class:loading={app.loadingId === song.id}
        class:current={app.song?.project.id === song.id}
        class:dragging={dragId === song.id}
        class:drop-before={dragId !== null && dropIndex === index}
        class:drop-after={dragId !== null && dropIndex === songs.length && index === songs.length - 1}
      >
        {#if numbered}
          <td class="pos">
            {#if onreorder}
              <button
                class="grip"
                title={i18n.t("playlist.dragHint")}
                aria-label={i18n.t("playlist.dragHint")}
                onpointerdown={(e) => startDrag(e, song.id)}
                onpointermove={moveDrag}
                onpointerup={endDrag}
                onpointercancel={endDrag}
                onkeydown={(e) => keyMove(e, song.id)}
              >
                <span class="num tabular">{index + 1}</span>
                <span class="dots" aria-hidden="true">⋮⋮</span>
              </button>
            {:else}
              <span class="num tabular">{index + 1}</span>
            {/if}
          </td>
        {/if}
        <td class="title-col">
          {#if renaming === song.id}
            <input
              class="rename"
              bind:value={draft}
              maxlength="200"
              aria-label={i18n.t("song.rename")}
              use:focusSelect
              onkeydown={(e) => {
                if (e.key === "Enter") commitRename();
                else if (e.key === "Escape") renaming = null;
              }}
              onblur={commitRename}
            />
          {:else}
            <button class="open" onclick={() => app.openSong(song.id)}>
              <span class="play" class:has-cover={!!song.cover}>
                {#if song.cover}<img src={song.cover} alt="" />{/if}
                <span class="glyph"><Icon name="play" size={12} fill /></span>
              </span>
              <span class="names">
                <span class="title" title={song.title}>{song.title}</span>
                {#if song.artist}<span class="artist">{song.artist}</span>{/if}
              </span>
            </button>
          {/if}
        </td>
        <td class="num tabular">{formatBpm(song.bpm)}</td>
        <td class="num tabular">{formatTime(song.durationSecs)}</td>
        <td><span class="engine" data-engine={engine}>{i18n.t(`engine.${engine}` as MessageKey)}</span></td>
        {#if showFolder}
          <td>
            <label class="folder" style={folder ? `--c:var(--folder-${folder.color})` : ""}>
              <span class="dot" class:hollow={!folder}></span>
              <select
                aria-label={i18n.t("folder.moveTo")}
                value={song.folder ?? ""}
                onchange={(e) => app.moveSong(song.id, e.currentTarget.value || null)}
              >
                <option value="">{i18n.t("folder.none")}</option>
                {#each app.folders as f (f.id)}
                  <option value={f.id}>{f.name}</option>
                {/each}
              </select>
            </label>
          </td>
        {/if}
        <td class="actions">
          <button class="icon-btn small" title={i18n.t("song.rename")} onclick={() => startRename(song)}>
            <Icon name="edit" size={14} />
          </button>
          <button class="icon-btn small" title={i18n.t("song.delete")} onclick={() => confirmDelete(song)}>
            <Icon name="trash" size={14} />
          </button>
        </td>
      </tr>
    {/each}
  </tbody>
</table>

<style>
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    text-align: left;
    font-size: var(--fs-label);
    font-weight: 600;
    color: var(--text-faint);
    padding: 0 12px 8px;
    border-bottom: 1px solid var(--line);
  }
  td {
    padding: 6px 12px;
    border-bottom: 1px solid var(--line);
    color: var(--text-dim);
  }
  tr:hover td {
    background: var(--bg-hover);
  }
  tr.current td {
    background: var(--bg-active);
  }
  tr.loading {
    opacity: 0.6;
  }
  tr.dragging td {
    opacity: 0.45;
  }
  tr.drop-before td {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  tr.drop-after td {
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .pos {
    width: 48px;
    text-align: center;
  }
  .grip {
    position: relative;
    width: 34px;
    height: 28px;
    border-radius: var(--radius-s);
    cursor: grab;
    touch-action: none;
  }
  .grip:active {
    cursor: grabbing;
  }
  .grip .dots {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--text-dim);
    opacity: 0;
  }
  tr:hover .grip .num,
  .grip:focus-visible .num {
    opacity: 0;
  }
  tr:hover .grip .dots,
  .grip:focus-visible .dots {
    opacity: 1;
  }
  .num {
    font-family: var(--mono);
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  tr.current .num {
    color: var(--accent);
  }
  .title-col {
    width: 50%;
    max-width: 0;
  }
  .num {
    text-align: right;
    width: 72px;
    font-family: var(--mono);
    font-size: var(--fs-small);
  }
  .actions {
    width: 72px;
    text-align: right;
    white-space: nowrap;
  }
  .rename {
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border-radius: var(--radius-s);
    border: 1px solid var(--accent);
    background: var(--bg);
    color: var(--text);
    font: inherit;
    font-weight: 600;
    user-select: text;
  }
  .names {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .artist {
    font-size: var(--fs-small);
    font-weight: 500;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .open {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-width: 0;
    text-align: left;
    color: var(--text);
    font-weight: 500;
  }
  .play {
    position: relative;
    flex: none;
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: var(--radius-s);
    overflow: hidden;
    background: var(--bg-active);
    color: var(--text-dim);
    transition: background 0.15s, color 0.15s;
  }
  .play.has-cover {
    border-radius: var(--radius-s);
  }
  .play img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .glyph {
    position: relative;
    display: grid;
    place-items: center;
  }
  .has-cover .glyph {
    opacity: 0;
    width: 100%;
    height: 100%;
    background: rgb(0 0 0 / 55%);
    color: var(--accent);
    transition: opacity 0.15s;
  }
  .open:hover .play:not(.has-cover),
  tr.current .play:not(.has-cover) {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .open:hover .has-cover .glyph,
  tr.current .has-cover .glyph {
    opacity: 1;
  }
  .title {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .engine {
    font-size: var(--fs-small);
    padding: 1px 6px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line-strong);
    color: var(--text-dim);
  }
  .engine[data-engine="studio"] {
    color: var(--accent);
  }
  .engine[data-engine="cloud"] {
    color: var(--stem-piano);
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--c);
    flex: none;
  }
  .dot.hollow {
    background: none;
    border: 1.5px solid var(--text-faint);
  }
  select {
    background: transparent;
    border: 0;
    color: var(--text-dim);
    font: inherit;
    cursor: pointer;
    min-width: 0;
    max-width: 200px;
    padding-right: 6px;
    text-overflow: ellipsis;
  }
  @media (max-width: 1100px) {
    select {
      max-width: 130px;
    }
  }
  select:hover {
    color: var(--text);
  }
  .small {
    width: 28px;
    height: 28px;
    opacity: 0;
  }
  tr:hover .small,
  .small:focus-visible {
    opacity: 1;
  }
</style>
