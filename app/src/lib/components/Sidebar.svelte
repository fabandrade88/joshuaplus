<script lang="ts">
  import ExportsCard from "./ExportsCard.svelte";
  import { app, type FolderKey } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import type { MessageKey } from "../i18n/en";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";
  import MiniPlayer from "./MiniPlayer.svelte";

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
  </nav>

  <section class="block folders">
    <h2 class="section-title">{i18n.t("home.title")}</h2>
    <ul>
      {#each app.folders as folder (folder.id)}
        <li>
          <button class="nav-item" class:active={isActive(folder.id)} onclick={() => app.openFolder(folder.id)}>
            <span class="dot" style={`--c:var(--folder-${folder.color})`}></span>
            <span class="name" title={folder.name}>{folder.name}</span>
            <span class="count tabular">{app.songsIn(folder.id).length}</span>
          </button>
        </li>
      {/each}
      {#if unfiledCount > 0}
        <li>
          <button class="nav-item muted" class:active={isActive(null)} onclick={() => app.openFolder(null)}>
            <span class="dot hollow"></span>
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
      {#if app.drums?.connected || app.inputs.length > 0}<span class="live-dot" aria-hidden="true"></span>{/if}
    </button>
    <button class="nav-item" onclick={() => (app.settingsOpen = true)}>
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
    font-weight: 600;
    text-align: left;
  }
  .nav-item:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--bg-active);
    color: var(--text);
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
    font-size: 11.5px;
    color: var(--text-faint);
    font-weight: 600;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 3px;
    background: var(--c);
    flex: none;
    margin: 0 3px;
  }
  .dot.hollow {
    background: none;
    border: 1.5px solid var(--text-faint);
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
    border: 1.5px dashed var(--line-strong);
    border-radius: var(--radius);
    color: var(--accent);
    transition: border-color 0.15s, background 0.15s;
  }
  .drop.active {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .drop-title {
    font-weight: 700;
    color: var(--text);
  }
  .drop-sub {
    font-size: 11px;
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
    font-size: 11.5px;
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
    border-top: 1px solid var(--line);
    padding: 8px 10px;
  }
  .live-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ok);
    box-shadow: 0 0 6px var(--ok);
  }
  /* Short windows: never let the processing queue squeeze the folder list to
     nothing; the queue scrolls instead. */
  .folders {
    min-height: 150px;
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
