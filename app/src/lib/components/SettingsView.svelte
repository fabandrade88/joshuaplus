<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import { api } from "../api";
  import { app } from "../app.svelte";
  import { i18n, LOCALES, type Locale } from "../i18n/index.svelte";
  import type { DeviceView, SeparatorChoice } from "../types";
  import Icon from "./Icon.svelte";

  const LALAL_KEY_URL = "https://www.lalal.ai/api/";

  let devices = $state<DeviceView[]>([]);
  let cloudKey = $state("");
  let connecting = $state(false);
  let minutes = $state<number | null>(null);

  onMount(() => {
    void api.listOutputDevices().then((d) => (devices = d));
    void app.refreshInfo();
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

<div class="settings">
  <header class="hero">
    <span class="badge" style="--c: var(--accent)"><Icon name="settings" size={16} /></span>
    <div class="hero-text">
      <h1>{i18n.t("settings.title")}</h1>
      <p>{i18n.t("settings.intro")}</p>
    </div>
    {#if app.info}<span class="version tabular">{i18n.t("settings.version", { v: app.info.version })}</span>{/if}
  </header>

  {#if app.info}
    {@const info = app.info}
    <div class="grid">
      <section class="card" style="--c: var(--folder-cyan)" aria-labelledby="set-language">
        <div class="card-head">
          <span class="badge"><Icon name="help" size={14} /></span>
          <h2 id="set-language">{i18n.t("settings.language")}</h2>
        </div>
        <select aria-labelledby="set-language" value={i18n.locale} onchange={(e) => app.setLocale(e.currentTarget.value as Locale)}>
          {#each LOCALES as l (l.id)}
            <option value={l.id}>{l.label}</option>
          {/each}
        </select>
      </section>

      <section class="card" style="--c: var(--ok)" aria-labelledby="set-audio">
        <div class="card-head">
          <span class="badge"><Icon name="volume" size={14} /></span>
          <h2 id="set-audio">{i18n.t("settings.audioDevice")}</h2>
        </div>
        <select aria-labelledby="set-audio" value={info.outputDevice ?? ""} onchange={(e) => app.setOutputDevice(e.currentTarget.value || null)}>
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
      </section>

      <section class="card" style="--c: var(--accent)" aria-labelledby="set-engine">
        <div class="card-head">
          <span class="badge"><Icon name="spark" size={14} /></span>
          <h2 id="set-engine">{i18n.t("settings.separator")}</h2>
        </div>
        <div class="radios" role="radiogroup" aria-labelledby="set-engine">
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
        </div>
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
      </section>

      <section class="card" style="--c: var(--folder-violet)" aria-labelledby="set-cloud">
        <div class="card-head">
          <span class="badge"><Icon name="cloud" size={14} /></span>
          <h2 id="set-cloud">{i18n.t("settings.cloud.title")}</h2>
        </div>
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

      <section class="card wide" style="--c: var(--folder-amber)" aria-labelledby="set-folders">
        <div class="card-head">
          <span class="badge"><Icon name="folder" size={14} /></span>
          <h2 id="set-folders">{i18n.t("settings.folders")}</h2>
        </div>
        <span class="label">{i18n.t("settings.workspace")}</span>
        <div class="row">
          <code title={info.workspace ?? ""}>{info.workspace ?? "—"}</code>
          <button class="btn small" disabled={app.movingLibrary} onclick={() => app.chooseWorkspace()}>{i18n.t("settings.change")}</button>
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
        <span class="label">{i18n.t("exportDir.title")}</span>
        <div class="row">
          <code title={info.exportDir ?? ""}>{info.exportDir ?? "—"}</code>
          <button class="btn small" onclick={() => app.chooseExportDir()}>{i18n.t("settings.change")}</button>
          {#if info.exportDirCustom}
            <button class="btn small" onclick={() => app.resetExportDir()}>{i18n.t("exportDir.reset")}</button>
          {/if}
        </div>
        <span class="note">{info.exportDirCustom ? i18n.t("exportDir.customHint") : i18n.t("exportDir.defaultHint")}</span>
      </section>

      <section class="card wide" style="--c: var(--stem-vocals)" aria-labelledby="set-prefs">
        <div class="card-head">
          <span class="badge"><Icon name="settings" size={14} /></span>
          <h2 id="set-prefs">{i18n.t("settings.preferences")}</h2>
        </div>
        <label class="check">
          <input type="checkbox" checked={info.updateCheck} onchange={(e) => app.setUpdateCheck(e.currentTarget.checked)} />
          <span>
            <strong>{i18n.t("update.setting")}</strong>
            <span class="note">{i18n.t("update.settingHint")}</span>
          </span>
        </label>
        <label class="check">
          <input type="checkbox" checked={info.autoBalance} onchange={(e) => app.setAutoBalance(e.currentTarget.checked)} />
          <span>
            <strong>{i18n.t("mix.autoSetting")}</strong>
            <span class="note">{i18n.t("mix.autoSettingHint")}</span>
          </span>
        </label>
        {#if app.desktopEntry?.available}
          <div class="row desktop">
            <span class="note">
              <strong>{i18n.t("desktopEntry.title")}</strong> · {app.desktopEntry.installed ? i18n.t("desktopEntry.on") : i18n.t("desktopEntry.off")}
            </span>
            <button class="btn small" onclick={() => app.setDesktopEntry(!app.desktopEntry?.installed)}>
              {app.desktopEntry.installed ? i18n.t("desktopEntry.remove") : i18n.t("desktopEntry.add")}
            </button>
          </div>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  /* Same page language as About: a header panel, then cards. */
  .settings {
    flex: 1;
    overflow-y: auto;
    padding: 16px 24px 28px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .settings > * {
    flex: none;
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 8px 14px;
    flex-wrap: wrap;
    padding: 10px 16px;
    border-radius: var(--radius);
    background: var(--bg-panel);
    border: 1px solid var(--line-strong);
  }
  .hero-text {
    flex: 1;
    min-width: 220px;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-title);
  }
  .hero p {
    margin: 2px 0 0;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .version {
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }
  .card {
    min-width: 0;
    padding: 10px 14px 14px;
    border-radius: var(--radius);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .card.wide {
    grid-column: 1 / -1;
  }
  .card-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .badge {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-s);
    color: var(--c);
    background: var(--bg-active);
  }
  .hero .badge {
    width: 30px;
    height: 30px;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-label);
    font-weight: 600;
    color: color-mix(in srgb, var(--c) 70%, var(--text));
  }
  .label {
    margin-top: 4px;
    font-size: var(--fs-label);
    color: var(--text-faint);
  }
  select,
  input {
    height: 32px;
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
  .radios {
    display: flex;
    flex-direction: column;
    gap: 2px;
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
    padding: 8px 10px;
    border-radius: var(--radius-s);
    background: var(--bg);
    border: 1px solid var(--line);
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .bar {
    flex-basis: 100%;
    height: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
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
    height: 28px;
    padding: 0 10px;
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
  .desktop {
    justify-content: space-between;
    padding-top: 6px;
    border-top: 1px solid var(--line);
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
    padding: 7px 10px;
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
  .choice {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: var(--radius-s);
    background: var(--bg-panel);
    border: 1px solid var(--accent);
  }
  .choice p {
    margin: 0;
    line-height: 1.45;
  }
  .choice .target {
    display: block;
    align-self: stretch;
    overflow-wrap: anywhere;
    white-space: normal;
  }
  .choice-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  @media (max-width: 1180px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
