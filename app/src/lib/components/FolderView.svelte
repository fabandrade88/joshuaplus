<script lang="ts">
  import { app, type FolderKey } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import type { FolderColor } from "../types";
  import FolderForm from "./FolderForm.svelte";
  import Icon from "./Icon.svelte";
  import SongTable from "./SongTable.svelte";

  let { folderKey }: { folderKey: FolderKey } = $props();

  let editing = $state(false);
  const folder = $derived(typeof folderKey === "string" && folderKey !== "all" ? app.folderById(folderKey) : undefined);
  const songs = $derived(app.songsIn(folderKey));
  const title = $derived(
    folder?.name ?? (folderKey === "all" ? i18n.t("home.allSongs") : i18n.t("home.unfiled")),
  );
  const inboxPath = $derived(
    app.info?.inbox ? (folder ? `${app.info.inbox}/${folder.id}` : app.info.inbox) : null,
  );

  async function save(name: string, color: FolderColor): Promise<void> {
    if (!folder) return;
    await app.updateFolder(folder.id, name, color);
    editing = false;
  }

  function remove(): void {
    if (folder && window.confirm(i18n.t("folder.deleteConfirm", { name: folder.name }))) void app.deleteFolder(folder.id);
  }
</script>

<div class="view" style={folder ? `--c:var(--folder-${folder.color})` : ""}>
  <header class="head">
    <button class="icon-btn" title={i18n.t("nav.home")} onclick={() => app.goHome()}><Icon name="back" /></button>
    {#if editing && folder}
      <div class="edit">
        <FolderForm
          name={folder.name}
          color={folder.color}
          submitLabel={i18n.t("folder.rename")}
          onsubmit={save}
          oncancel={() => (editing = false)}
        />
      </div>
    {:else}
      <span class="badge"><Icon name={folder ? "folder" : "music"} size={20} /></span>
      <div class="titles">
        <h1 title={title}>{title}</h1>
        <p class="tabular">{i18n.songs(songs.length)}</p>
      </div>
      <div class="actions">
        {#if folder}
          <button class="icon-btn" title={i18n.t("folder.rename")} onclick={() => (editing = true)}><Icon name="edit" /></button>
          <button class="icon-btn" title={i18n.t("folder.delete")} onclick={remove}><Icon name="trash" /></button>
        {/if}
        <button class="btn" onclick={() => app.pickFiles()}>
          <Icon name="plus" size={16} />{i18n.t("folder.import")}
        </button>
        <button class="btn primary" disabled={songs.length === 0} onclick={() => app.playFolder(folderKey)}>
          <Icon name="play" size={14} fill />{i18n.t("folder.playAll")}
        </button>
      </div>
    {/if}
  </header>

  {#if folderKey !== "all"}
    <div class="drop-hint">
      <Icon name="drop" size={16} />
      <span>{i18n.t("folder.dropHint")}</span>
      {#if inboxPath}<code title={inboxPath}>{i18n.t("folder.inboxHint", { path: inboxPath })}</code>{/if}
    </div>
  {/if}

  {#if songs.length > 0}
    <SongTable
      {songs}
      showFolder={!folder}
      numbered
      onreorder={folder ? (ids) => app.reorderFolder(folder.id, ids) : undefined}
    />
  {:else}
    <div class="empty">
      <Icon name="music" size={28} />
      <p>{i18n.t("folder.empty")}</p>
    </div>
  {/if}
</div>

<style>
  .view {
    flex: 1;
    overflow-y: auto;
    padding: 28px 40px 48px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 46px;
    height: 46px;
    border-radius: 13px;
    background: color-mix(in srgb, var(--c, var(--accent)) 18%, transparent);
    color: var(--c, var(--accent));
    flex: none;
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: 26px;
    letter-spacing: -0.03em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .titles p {
    margin: 0;
    color: var(--text-faint);
    font-size: var(--fs-small);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .edit {
    flex: 1;
    max-width: 420px;
  }
  .drop-hint {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 10px 14px;
    border: 1.5px dashed var(--line-strong);
    border-radius: var(--radius);
    color: var(--text-dim);
    font-size: var(--fs-small);
  }
  .drop-hint :global(svg) {
    color: var(--c, var(--accent));
  }
  code {
    font-family: var(--mono);
    font-size: var(--fs-small);
    color: var(--text-faint);
    user-select: text;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .empty {
    display: grid;
    justify-items: center;
    gap: 6px;
    padding: 60px 0;
    color: var(--text-faint);
  }
  .empty p {
    margin: 0;
  }
  @media (max-width: 1180px), (max-height: 760px) {
    .view {
      padding: 18px 24px 32px;
      gap: 14px;
    }
    .head {
      gap: 10px;
    }
    .badge {
      width: 38px;
      height: 38px;
    }
    h1 {
      font-size: 22px;
    }
  }
</style>
