<script lang="ts">
  // Everything exported so far (audio mixes and videos), with where it lives
  // and quick ways to play it or find it on disk.
  import { onMount } from "svelte";
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import type { ExportFile } from "../types";
  import Icon from "./Icon.svelte";

  onMount(() => {
    void app.refreshExports();
    // Files can be added or removed in Finder while the view is open.
    const refresh = () => void app.refreshExports();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  });

  const files = $derived(app.exports?.files ?? []);
  const groups = $derived(
    [
      { kind: "audio", title: i18n.t("exports.audio"), icon: "music", items: files.filter((f) => f.kind === "audio") },
      { kind: "video", title: i18n.t("exports.video"), icon: "video", items: files.filter((f) => f.kind === "video") },
    ] as const,
  );
  const dir = $derived(app.exports?.dir ?? app.info?.exportDir ?? "");

  function when(f: ExportFile): string {
    if (!f.modifiedUnix) return "";
    return new Intl.DateTimeFormat(i18n.locale, { dateStyle: "medium", timeStyle: "short" }).format(
      new Date(f.modifiedUnix * 1000),
    );
  }

  function size(bytes: number): string {
    const mb = bytes / (1024 * 1024);
    return mb >= 10 ? `${Math.round(mb)} MB` : `${mb.toFixed(1)} MB`;
  }
</script>

<div class="exports">
  <header class="head">
    <div class="title">
      <h1>{i18n.t("exports.title")}</h1>
      <p>{i18n.t("exports.intro")}</p>
    </div>
    <div class="folder">
      <Icon name="folder" size={15} />
      <code title={dir}>{dir}</code>
      <button class="btn" onclick={() => app.openExportDir()}>{i18n.t("exports.openFolder")}</button>
      <button class="btn ghost" onclick={() => app.chooseExportDir()}>{i18n.t("exports.changeFolder")}</button>
    </div>
  </header>

  {#if files.length === 0}
    <p class="empty">{i18n.t("exports.empty")}</p>
  {:else}
    {#each groups as g (g.kind)}
      {#if g.items.length > 0}
        <section aria-label={g.title}>
          <h2 class="section-title">{g.title} <span class="tabular">· {g.items.length}</span></h2>
          <ul>
            {#each g.items as f (f.path)}
              <li>
                <span class="kind" data-kind={f.kind}><Icon name={g.icon} size={16} /></span>
                <span class="name" title={f.name}>{f.name}</span>
                <span class="meta tabular">{when(f)} · {size(f.size)}</span>
                <span class="actions">
                  <button class="btn small" onclick={() => app.openExport(f.path)}>
                    <Icon name="play" size={12} />
                    {i18n.t("exports.open")}
                  </button>
                  <button class="btn small ghost" onclick={() => app.revealExport(f.path)}>{i18n.t("exports.reveal")}</button>
                </span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/each}
  {/if}

  <p class="note">{i18n.t("exports.takesNote")}</p>
</div>

<style>
  .exports {
    flex: 1;
    overflow-y: auto;
    padding: 20px 24px 28px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .exports > * {
    flex: none;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-h1);
  }
  .title p {
    margin: 4px 0 0;
    color: var(--text-dim);
    font-size: var(--fs-body);
  }
  .folder {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    color: var(--text-dim);
  }
  .folder code {
    flex: 1 1 220px;
    min-width: 0;
    font-size: var(--fs-small);
    overflow-wrap: anywhere;
  }
  .btn.ghost {
    background: transparent;
  }
  .btn.small {
    padding: 5px 10px;
    font-size: var(--fs-small);
    gap: 5px;
  }
  .empty,
  .note {
    margin: 0;
    font-size: var(--fs-body);
    line-height: 1.45;
    color: var(--text-dim);
  }
  .empty {
    padding: 28px 16px;
    text-align: center;
    border: 1px dashed var(--line-strong);
    border-radius: var(--radius);
  }
  .note {
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h2 {
    margin: 0;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: var(--radius-s);
    background: var(--bg-raised);
  }
  li:hover {
    background: var(--bg-hover);
  }
  .kind {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: var(--radius);
    color: var(--accent);
    background: var(--bg-active);
  }
  .kind[data-kind="video"] {
    color: var(--danger);
    background: var(--bg-active);
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    font-size: var(--fs-small);
    color: var(--text-faint);
    white-space: nowrap;
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  @media (max-width: 1100px) {
    li {
      grid-template-columns: auto minmax(0, 1fr) auto;
    }
    .kind {
      grid-row: 1 / span 2;
    }
    .meta {
      grid-column: 2;
      grid-row: 2;
    }
    .actions {
      grid-row: 1 / span 2;
      grid-column: 3;
    }
  }
</style>
