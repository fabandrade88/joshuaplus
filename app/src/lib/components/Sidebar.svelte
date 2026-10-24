<script lang="ts">
  import ExportsCard from "./ExportsCard.svelte";
  import UpdateCard from "./UpdateCard.svelte";
  import FolderGlyph from "./FolderGlyph.svelte";
  import { glyphsFor } from "../folderGlyph";
  import { app, type FolderKey } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import type { MessageKey } from "../i18n/en";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";
  import MiniPlayer from "./MiniPlayer.svelte";

  const glyphs = $derived(glyphsFor(app.folders));
  const unfiledCount = $derived(app.songsIn(null).length);
  const dropTarget = $derived(app.folderById(app.dropFolder)?.name ?? null);

  function isActive(key: FolderKey): boolean {
    return app.view.kind === "folder" && app.view.folder === key;
  }

  function stageLabel(stage: string | null): string {
    return stage ? i18n.t(`stage.${stage}` as MessageKey) : i18n.t("jobs.queued");
  }
</script>

<aside class="sidebar">
  <header class="brand"><Logo /></header>

  <nav class="nav">
    <button class="nav-item" class:active={app.view.kind === "home"} onclick={() => app.goHome()}>
      <Icon name="home" size={16} />
      <span>{i18n.t("nav.home")}</span>
    </button>
    <button class="nav-item" class:active={isActive("all")} onclick={() => app.openFolder("all")}>
      <Icon name="music" size={16} />
      <span>{i18n.t("home.allSongs")}</span>
      <span class="count tabular">{app.songs.length}</span>
    </button>
    <button class="nav-item" class:active={app.view.kind === "exports"} onclick={() => app.openExports()}>
      <Icon name="export" size={16} />
      <span>{i18n.t("nav.exports")}</span>
      {#if app.exports}<span class="count tabular">{app.exports.files.length}</span>{/if}
    </button>
  </nav>

  <section class="block folders">
    <h2 class="section-title">{i18n.t("home.title")}</h2>
    <ul>
      {#each app.folders as folder (folder.id)}
        <li>
          <button class="nav-item" class:active={isActive(folder.id)} onclick={() => app.openFolder(folder.id)}>
            <FolderGlyph id={folder.id} glyph={glyphs.get(folder.id)} color={`var(--folder-${folder.color})`} />
            <span class="name" title={folder.name}>{folder.name}</span>
            <span class="count tabular">{app.songsIn(folder.id).length}</span>
          </button>
        </li>
      {/each}
      {#if unfiledCount > 0}
        <li>
          <button class="nav-item muted" class:active={isActive(null)} onclick={() => app.openFolder(null)}>
            <FolderGlyph id={null} color={null} />
            <span class="name">{i18n.t("home.unfiled")}</span>
            <span class="count tabular">{unfiledCount}</span>
          </button>
        </li>
      {/if}
    </ul>
  </section>

  <MiniPlayer />

  <section class="drop" class:active={app.dragging} aria-live="polite">
    <Icon name="drop" size={18} />
    <div>
      <div class="drop-title">{app.dragging ? i18n.t("drop.active") : i18n.t("drop.title")}</div>
      <div class="drop-sub">
        {#if dropTarget}<span class="target">→ {dropTarget}</span>{:else}{i18n.t("drop.formats")}{/if}
      </div>
    </div>
  </section>

  <UpdateCard />
  <ExportsCard />
  {#if app.jobs.length > 0}
    <section class="block">
      <h2 class="section-title">{i18n.t("jobs.title")}</h2>
      <ul class="jobs">
        {#each app.jobs as job (job.id)}
          <li class="job" data-status={job.status}>
            <div class="job-row">
              <span class="job-name" title={job.fileName}>{job.fileName}</span>
              {#if job.status === "queued" || job.status === "running"}
                <button class="icon-btn small" title={i18n.t("jobs.cancel")} onclick={() => app.cancelJob(job.id)}>
                  <Icon name="close" size={14} />
                </button>
              {:else if job.status === "failed"}
                <button class="icon-btn small" title={i18n.t("jobs.dismiss")} onclick={() => app.dismissJob(job.id)}>
                  <Icon name="close" size={14} />
                </button>
              {/if}
            </div>
            {#if job.status === "running" || job.status === "queued"}
              <div class="job-stage">
                {stageLabel(job.stage)}{job.stage === "separating" || job.stage === "downloadingModel"
                  ? ` · ${Math.round(job.progress * 100)}%`
                  : ""}
              </div>
              <div class="progress"><div style={`width:${job.status === "queued" ? 0 : job.progress * 100}%`}></div></div>
            {:else if job.status === "done"}
              <button class="job-stage ok" onclick={() => job.songId && app.openSong(job.songId)}>✓ {i18n.t("jobs.done")}</button>
            {:else}
              <div class="job-stage err">
                {job.error ? i18n.error(job.error.code) : i18n.t(`jobs.${job.status}` as MessageKey)}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <footer class="foot">
    <button class="nav-item" class:active={app.view.kind === "gear"} onclick={() => app.openGear()}>
      <Icon name="plug" size={16} />
      <span>{i18n.t("nav.gear")}</span>
      {#if app.drums?.connected || app.liveInputs.length > 0}<span class="live-dot" aria-hidden="true"></span>{/if}
    </button>
    <button class="nav-item" class:active={app.view.kind === "settings"} onclick={() => app.openSettings()}>
      <Icon name="settings" size={16} />
      <span>{i18n.t("settings.title")}</span>
    </button>
    <button class="nav-item" class:active={app.view.kind === "help"} onclick={() => (app.view = { kind: "help" })}>
      <Icon name="help" size={16} />
      <span>{i18n.t("nav.help")}</span>
    </button>
    <button class="nav-item" class:active={app.view.kind === "about"} onclick={() => (app.view = { kind: "about" })}>
      <Icon name="music" size={16} />
      <span>{i18n.t("nav.about")}</span>
    </button>
  </footer>
</aside>

<style>
  .sidebar {
    width: var(--sidebar-w);
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    border-right: 1px solid var(--line);
    min-height: 0;
    /* On short windows with export cards and a queue the column scrolls,
       and the footer navigation stays pinned at the bottom. */
    overflow-y: auto;
    overflow-x: hidden;
  }
  .brand {
    padding: 20px 20px 14px;
  }
  .nav {
    padding: 0 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav-item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
    font-weight: 500;
    text-align: left;
  }
  .nav-item:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--bg-active);
    color: var(--text);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .nav-item.muted {
    font-weight: 500;
  }
  .name {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .nav-item > span:nth-child(2):not(.name) {
    flex: 1;
  }
  .count {
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .block {
    padding: 14px 10px 4px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
  }
  .block .section-title {
    padding: 0 10px;
  }
  .folders {
    flex: 1;
    overflow: hidden;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
  }
  .drop {
    margin: 8px 12px;
    padding: 12px;
    display: flex;
    gap: 10px;
    align-items: center;
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius);
    color: var(--accent);
    transition: border-color 0.15s, background 0.15s;
  }
  .drop.active {
    border-color: var(--accent);
    background: var(--bg-hover);
  }
  .drop-title {
    font-weight: 600;
    color: var(--text);
  }
  .drop-sub {
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .target {
    color: var(--accent);
    font-weight: 600;
  }
  .jobs {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 220px;
    padding: 0 2px;
  }
  .job {
    background: var(--bg-panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 8px 10px;
  }
  .job-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .job-name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .job-stage {
    font-size: var(--fs-small);
    color: var(--text-dim);
    margin-top: 2px;
  }
  .job-stage.ok {
    color: var(--ok);
    font-weight: 600;
  }
  .job-stage.err {
    color: var(--danger);
  }
  .progress {
    height: 3px;
    background: var(--bg);
    border-radius: 2px;
    margin-top: 6px;
    overflow: hidden;
  }
  .progress div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .job[data-status="running"] .progress div {
    min-width: 6%;
  }
  .small {
    width: 24px;
    height: 24px;
  }
  .foot {
    position: sticky;
    bottom: 0;
    z-index: 1;
    margin-top: auto;
    border-top: 1px solid var(--line);
    padding: 8px 10px;
    background: var(--bg-raised);
  }
  .live-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ok);
  }
  /* Short windows: never let the processing queue squeeze the folder list to
     nothing; the queue scrolls instead. */
  .folders {
    min-height: 150px;
  }
  .brand,
  .nav,
  .drop {
    flex-shrink: 0;
  }
  .block:not(.folders) {
    flex-shrink: 1;
  }
  @media (max-height: 760px) {
    .brand {
      padding: 14px 16px 10px;
    }
    .drop {
      margin: 4px 12px;
      padding: 8px 10px;
    }
    .block {
      padding-top: 8px;
    }
    .foot .nav-item {
      padding-top: 6px;
      padding-bottom: 6px;
    }
  }
</style>
