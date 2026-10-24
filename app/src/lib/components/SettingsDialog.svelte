<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api } from "../api";
  import { app } from "../app.svelte";
  import { i18n, LOCALES, type Locale } from "../i18n/index.svelte";
  import type { DeviceView, SeparatorChoice } from "../types";
  import Icon from "./Icon.svelte";

  const LALAL_KEY_URL = "https://www.lalal.ai/api/";

  let dialog: HTMLDialogElement | undefined = $state();
  let devices = $state<DeviceView[]>([]);
  let cloudKey = $state("");
  let connecting = $state(false);
  let minutes = $state<number | null>(null);

  $effect(() => {
    if (!dialog) return;
    if (app.settingsOpen && !dialog.open) {
      dialog.showModal();
      void api.listOutputDevices().then((d) => (devices = d));
      void app.refreshInfo();
    } else if (!app.settingsOpen && dialog.open) dialog.close();
  });

  const engines: SeparatorChoice[] = ["studio", "quick", "cloud"];
  const engineKey = {
    studio: "settings.engine.studio",
    quick: "settings.engine.quick",
    cloud: "settings.engine.cloud",
  } as const;

  async function connect(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    connecting = true;
    const m = await app.setCloudKey(cloudKey);
    connecting = false;
    if (m !== undefined) {
      minutes = m;
      cloudKey = "";
    }
  }
</script>

<dialog bind:this={dialog} onclose={() => (app.settingsOpen = false)}>
  <header>
    <h2>{i18n.t("settings.title")}</h2>
    <button class="icon-btn" title={i18n.t("settings.close")} onclick={() => (app.settingsOpen = false)}>
      <Icon name="close" />
    </button>
  </header>

  {#if app.info}
    {@const info = app.info}
    <label class="field">
      <span class="section-title">{i18n.t("settings.language")}</span>
      <select value={i18n.locale} onchange={(e) => app.setLocale(e.currentTarget.value as Locale)}>
        {#each LOCALES as l (l.id)}
          <option value={l.id}>{l.label}</option>
        {/each}
      </select>
    </label>

    <fieldset class="field">
      <legend class="section-title">{i18n.t("settings.separator")}</legend>
      {#each engines as engine (engine)}
        <label class="radio">
          <input
            type="radio"
            name="engine"
            value={engine}
            checked={info.separator === engine}
            disabled={engine === "cloud" && !info.cloudConfigured}
            onchange={() => app.setSeparator(engine)}
          />
          {i18n.t(engineKey[engine])}
        </label>
      {/each}
      <div class="status">
        {#if info.modelInstalled}
          <span class="ok">✓ {i18n.t("settings.model.installed")}</span>
        {:else if app.modelProgress !== null}
          <span>{i18n.t("settings.model.downloading", { pct: Math.round(app.modelProgress * 100) })}</span>
          <div class="bar"><div style={`width:${app.modelProgress * 100}%`}></div></div>
        {:else}
          <span>{i18n.t("settings.model.missing", { mb: info.modelSizeMb })}</span>
          <button class="btn small" onclick={() => app.downloadModel()}>{i18n.t("settings.model.download")}</button>
        {/if}
      </div>
    </fieldset>

    <section class="field cloud">
      <span class="section-title"><Icon name="cloud" size={13} /> {i18n.t("settings.cloud.title")}</span>
      <p class="note">{i18n.t("settings.cloud.body")}</p>
      {#if info.cloudConfigured}
        <div class="row">
          <span class="ok">✓ {i18n.t("settings.cloud.connected")}</span>
          {#if minutes !== null}<span class="note tabular">{i18n.t("settings.cloud.minutes", { m: Math.floor(minutes) })}</span>{/if}
          <button class="btn small" onclick={() => app.clearCloudKey()}>{i18n.t("settings.cloud.disconnect")}</button>
        </div>
      {:else}
        <form class="row" onsubmit={connect}>
          <input
            type="password"
            autocomplete="off"
            spellcheck="false"
            bind:value={cloudKey}
            placeholder={i18n.t("settings.cloud.keyPlaceholder")}
            aria-label={i18n.t("settings.cloud.keyPlaceholder")}
          />
          <button class="btn small primary" type="submit" disabled={connecting || cloudKey.trim().length < 8}>
            {i18n.t("settings.cloud.connect")}
          </button>
        </form>
        <button class="link" onclick={() => openUrl(LALAL_KEY_URL)}>{i18n.t("settings.cloud.getKey")} ↗</button>
      {/if}
    </section>

    <label class="field">
      <span class="section-title">{i18n.t("settings.audioDevice")}</span>
      <select value={info.outputDevice ?? ""} onchange={(e) => app.setOutputDevice(e.currentTarget.value || null)}>
        <option value="">{i18n.t("settings.defaultDevice")}</option>
        {#each devices as d (d.id)}
          <option value={d.id}>{d.name}</option>
        {/each}
      </select>
      {#if info.audio}
        <span class="note tabular">{info.audio.device} · {info.audio.sampleRate} Hz · {info.audio.channels} ch</span>
      {:else if info.audioError}
        <span class="note err">{info.audioError}</span>
      {/if}
    </label>

    <div class="field">
      <span class="section-title">{i18n.t("settings.workspace")}</span>
      <div class="row">
        <code title={info.workspace ?? ""}>{info.workspace ?? "—"}</code>
        <button class="btn" disabled={app.movingLibrary} onclick={() => app.chooseWorkspace()}>{i18n.t("settings.change")}</button>
      </div>
      {#if app.pendingWorkspace}
        <div class="choice" role="group" aria-label={i18n.t("workspace.choiceTitle")}>
          <p>{i18n.t("workspace.choiceTitle")}</p>
          <code class="target" title={app.pendingWorkspace}>{app.pendingWorkspace}</code>
          {#if app.movingLibrary}
            <p class="note">{i18n.t("workspace.moving")}</p>
          {:else}
            <div class="choice-buttons">
              <button class="btn primary" onclick={() => app.applyWorkspace(app.pendingWorkspace!, true)}>{i18n.t("workspace.move")}</button>
              <button class="btn" onclick={() => app.applyWorkspace(app.pendingWorkspace!, false)}>{i18n.t("workspace.empty")}</button>
              <button class="btn" onclick={() => (app.pendingWorkspace = null)}>{i18n.t("common.cancel")}</button>
            </div>
            <p class="note">{i18n.t("workspace.choiceHint")}</p>
          {/if}
        </div>
      {/if}
    </div>

    <div class="field">
      <span class="section-title">{i18n.t("exportDir.title")}</span>
      <div class="row">
        <code title={info.exportDir ?? ""}>{info.exportDir ?? "—"}</code>
        <button class="btn" onclick={() => app.chooseExportDir()}>{i18n.t("settings.change")}</button>
        {#if info.exportDirCustom}
          <button class="btn" onclick={() => app.resetExportDir()}>{i18n.t("exportDir.reset")}</button>
        {/if}
      </div>
      <span class="note">{info.exportDirCustom ? i18n.t("exportDir.customHint") : i18n.t("exportDir.defaultHint")}</span>
    </div>

    <label class="check">
      <input type="checkbox" checked={info.updateCheck} onchange={(e) => app.setUpdateCheck(e.currentTarget.checked)} />
      <span>
        <strong>{i18n.t("update.setting")}</strong>
        <span class="note">{i18n.t("update.settingHint")}</span>
      </span>
    </label>

    {#if app.desktopEntry?.available}
      <div class="field">
        <span class="section-title">{i18n.t("desktopEntry.title")}</span>
        <div class="row">
          <span class="note">{app.desktopEntry.installed ? i18n.t("desktopEntry.on") : i18n.t("desktopEntry.off")}</span>
          <button class="btn" onclick={() => app.setDesktopEntry(!app.desktopEntry?.installed)}>
            {app.desktopEntry.installed ? i18n.t("desktopEntry.remove") : i18n.t("desktopEntry.add")}
          </button>
        </div>
      </div>
    {/if}

    <footer class="tabular">{i18n.t("settings.version", { v: info.version })} · MIT</footer>
  {/if}
</dialog>

<style>
  .choice {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
    padding: 12px;
    border-radius: var(--radius);
    background: var(--bg-panel);
    border: 1px solid var(--line-strong);
  }
  .choice p {
    margin: 0;
    line-height: 1.45;
  }
  .choice .target {
    display: block;
    position: static;
    align-self: stretch;
    overflow-wrap: anywhere;
  }
  .choice-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  dialog {
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    padding: 0 24px 20px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    background: var(--bg-panel);
    color: var(--text);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 55%);
  }
  header {
    position: sticky;
    top: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 0 8px;
    background: var(--bg-panel);
    z-index: 1;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-title);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 18px 0 0;
    padding: 0;
    border: 0;
  }
  .section-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  select,
  input {
    height: 36px;
    padding: 0 10px;
    border-radius: var(--radius-s);
    border: 1px solid var(--line-strong);
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }
  input {
    flex: 1;
    min-width: 0;
    user-select: text;
  }
  .radio {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }
  .radio input {
    height: auto;
    flex: none;
    accent-color: var(--accent);
  }
  .radio:has(input:disabled) {
    opacity: 0.45;
  }
  .status {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--bg);
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .bar {
    flex-basis: 100%;
    height: 4px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .cloud {
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .note {
    margin: 0;
    font-size: var(--fs-small);
    color: var(--text-faint);
    line-height: 1.5;
  }
  .ok {
    color: var(--ok);
    font-weight: 600;
  }
  .err {
    color: var(--danger);
  }
  .small {
    height: 30px;
    padding: 0 12px;
    font-size: var(--fs-small);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    margin-top: 16px;
    cursor: pointer;
  }
  .check input {
    width: 16px;
    height: 16px;
    margin-top: 2px;
    flex: none;
    accent-color: var(--accent);
  }
  .check > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .check strong {
    font-size: var(--fs-body);
  }
  .link {
    align-self: flex-start;
    color: var(--accent);
    font-weight: 600;
    font-size: var(--fs-small);
  }
  .link:hover {
    text-decoration: underline;
  }
  code {
    flex: 1;
    min-width: 0;
    padding: 9px 10px;
    border-radius: var(--radius-s);
    background: var(--bg);
    border: 1px solid var(--line);
    font-family: var(--mono);
    font-size: var(--fs-small);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    user-select: text;
  }
  footer {
    margin-top: 20px;
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
</style>
