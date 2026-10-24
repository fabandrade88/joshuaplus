<script lang="ts">
  import { app, type FolderKey } from "../app.svelte";
  import { formatTime } from "../format";
  import { i18n } from "../i18n/index.svelte";
  import { STEM_COLORS } from "../stems";
  import FolderForm from "./FolderForm.svelte";
  import Icon from "./Icon.svelte";
  import SongTable from "./SongTable.svelte";

  let creating = $state(false);

  const recent = $derived([...app.songs].sort((a, b) => b.createdUnix - a.createdUnix).slice(0, 6));
  const unfiled = $derived(app.songsIn(null));

  function stats(key: FolderKey): { count: number; minutes: string } {
    const songs = app.songsIn(key);
    return { count: songs.length, minutes: formatTime(songs.reduce((t, s) => t + s.durationSecs, 0)) };
  }

  async function create(name: string, color: Parameters<typeof app.createFolder>[1]): Promise<void> {
    if (await app.createFolder(name, color)) creating = false;
  }
</script>

<div class="home">
  <header class="head">
    <div>
      <h1>{i18n.t("home.title")}</h1>
      <p>{i18n.t("home.subtitle")}</p>
    </div>
    <div class="head-actions">
      <button class="btn" onclick={() => app.pickFiles()}>
        <Icon name="drop" size={16} />{i18n.t("folder.import")}
      </button>
      <button class="btn primary" onclick={() => (creating = true)}>
        <Icon name="plus" size={16} />{i18n.t("home.newFolder")}
      </button>
    </div>
  </header>

  <div class="grid">
    {#if creating}
      <div class="card form-card">
        <FolderForm submitLabel={i18n.t("home.create")} onsubmit={create} oncancel={() => (creating = false)} />
      </div>
    {/if}

    {#each app.folders as folder (folder.id)}
      {@const s = stats(folder.id)}
      <div class="card folder" style={`--c:var(--folder-${folder.color})`}>
        <div class="tab"></div>
        <button class="card-main" onclick={() => app.openFolder(folder.id)}>
          <span class="badge"><Icon name="folder" size={16} /></span>
          <span class="text">
            <span class="card-name" title={folder.name}>{folder.name}</span>
            <span class="meta tabular">{i18n.songs(s.count)} · {s.minutes}</span>
          </span>
        </button>
        <button
          class="quick-play"
          title={i18n.t("folder.playAll")}
          aria-label={`${i18n.t("folder.playAll")}: ${folder.name}`}
          disabled={s.count === 0}
          onclick={() => app.playFolder(folder.id)}
        >
          <Icon name="play" size={12} fill />
        </button>
      </div>
    {/each}

    <div class="card special">
      <button class="card-main" onclick={() => app.openFolder("all")}>
        <span class="badge bars" aria-hidden="true">
          {#each ["vocals", "drums", "bass", "guitar"] as const as stem (stem)}
            <i style={`background:${STEM_COLORS[stem]}`}></i>
          {/each}
        </span>
        <span class="text">
          <span class="card-name">{i18n.t("home.allSongs")}</span>
          <span class="meta tabular">{i18n.songs(app.songs.length)} · {stats("all").minutes}</span>
        </span>
      </button>
    </div>

    {#if unfiled.length > 0}
      <div class="card special">
        <button class="card-main" onclick={() => app.openFolder(null)}>
          <span class="badge hollow"><Icon name="music" size={16} /></span>
          <span class="text">
            <span class="card-name">{i18n.t("home.unfiled")}</span>
            <span class="meta tabular">{i18n.songs(unfiled.length)} · {stats(null).minutes}</span>
          </span>
        </button>
      </div>
    {/if}
  </div>

  {#if app.folders.length === 0 && !creating}
    <p class="hint">{i18n.t("home.empty")}</p>
  {/if}

  <section class="recent">
    <h2 class="section-title">{i18n.t("home.recent")}</h2>
    {#if recent.length > 0}
      <SongTable songs={recent} />
    {:else}
      <p class="hint">{i18n.t("home.noSongs")}</p>
    {/if}
  </section>
</div>

<style>
  .home {
    flex: 1;
    overflow-y: auto;
    padding: 32px 40px 48px;
    display: flex;
    flex-direction: column;
    gap: 24px;
    background:
      radial-gradient(900px 320px at 20% -10%, color-mix(in srgb, var(--accent) 8%, transparent), transparent 70%),
      var(--bg);
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    letter-spacing: -0.03em;
  }
  .head p {
    margin: 4px 0 0;
    color: var(--text-dim);
  }
  .head-actions {
    display: flex;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 10px;
  }
  .card {
    position: relative;
    display: flex;
    align-items: center;
    border-radius: var(--radius);
    background: var(--bg-panel);
    border: 1px solid var(--line);
    overflow: hidden;
    transition: border-color 0.15s, background 0.15s;
  }
  .card:hover {
    border-color: var(--line-strong);
    background: var(--bg-hover);
  }
  .folder .tab {
    position: absolute;
    inset: 0 auto 0 0;
    width: 3px;
    background: var(--c);
  }
  .card-main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    text-align: left;
  }
  .badge {
    flex: none;
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--c, var(--accent)) 18%, transparent);
    color: var(--c, var(--accent));
  }
  .badge.hollow {
    background: var(--bg-active);
    color: var(--text-dim);
  }
  .badge.bars {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    padding: 8px;
    background: var(--bg-active);
  }
  .badge.bars i {
    flex: 1;
    border-radius: 2px;
  }
  .badge.bars i:nth-child(1) { height: 60%; }
  .badge.bars i:nth-child(2) { height: 100%; }
  .badge.bars i:nth-child(3) { height: 45%; }
  .badge.bars i:nth-child(4) { height: 80%; }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .card-name {
    font-size: var(--fs-body);
    font-weight: 800;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .quick-play {
    flex: none;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    margin-right: 12px;
    border-radius: 50%;
    background: var(--bg-active);
    color: var(--text-dim);
    opacity: 0;
    transition: opacity 0.15s, background 0.15s;
  }
  .card:hover .quick-play,
  .quick-play:focus-visible {
    opacity: 1;
  }
  .quick-play:hover {
    background: var(--c, var(--accent));
    color: var(--accent-ink);
  }
  .quick-play:disabled {
    display: none;
  }
  .special {
    background: var(--bg-raised);
  }
  .form-card {
    grid-column: span 2;
    padding: 14px;
    border-color: var(--accent);
  }
  .hint {
    margin: 0;
    color: var(--text-faint);
  }
  .recent {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  @media (max-width: 1180px), (max-height: 760px) {
    .home {
      padding: 20px 24px 32px;
      gap: 18px;
    }
  }
</style>
