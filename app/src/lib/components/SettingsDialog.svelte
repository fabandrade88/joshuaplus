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
        <button class="btn" onclick={() => app.chooseWorkspace()}>{i18n.t("settings.change")}</button>
      </div>
    </div>

    <footer class="tabular">{i18n.t("settings.version", { v: info.version })} · MIT</footer>
  {/if}
</dialog>

<style>
  dialog {
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    padding: 0 24px 20px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-l);
    background: var(--bg-panel);
    color: var(--text);
    box-shadow: var(--shadow);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 55%);
    backdrop-filter: blur(3px);
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
    font-size: 18px;
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
    font-size: 12.5px;
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
    font-size: 12px;
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
    font-size: 12.5px;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .link {
    align-self: flex-start;
    color: var(--accent);
    font-weight: 600;
    font-size: 12.5px;
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
    font-size: 11.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    user-select: text;
  }
  footer {
    margin-top: 20px;
    font-size: 11.5px;
    color: var(--text-faint);
  }
</style>
