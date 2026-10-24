<script lang="ts">
  // Sidebar notice: a newer release of Joshua+ is out. Opens the release
  // page (notes and installers); "Not now" hides it until the next version.
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";
</script>

{#if app.update}
  {@const update = app.update}
  <div class="card" role="status">
    <div class="row">
      <span class="badge"><Icon name="spark" size={13} /></span>
      <span class="title">{i18n.t("update.available", { v: update.version })}</span>
    </div>
    <p>{i18n.t("update.hint", { current: app.info?.version ?? "" })}</p>
    <div class="actions">
      <button class="btn small primary" onclick={() => openUrl(update.url)}>{i18n.t("update.open")} ↗</button>
      <button class="link" onclick={() => app.skipUpdate()}>{i18n.t("update.later")}</button>
    </div>
  </div>
{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0 12px 8px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--bg-panel);
    border: 1px solid var(--line-strong);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .badge {
    display: grid;
    place-items: center;
    color: var(--accent);
  }
  .title {
    font-size: var(--fs-body);
    font-weight: 600;
    line-height: 1.3;
  }
  p {
    margin: 0;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 12px;
  }
  .btn.small {
    padding: 5px 10px;
    font-size: var(--fs-small);
  }
  .link {
    padding: 0;
    font-size: var(--fs-small);
    color: var(--text-dim);
    text-decoration: underline;
    background: none;
  }
</style>
