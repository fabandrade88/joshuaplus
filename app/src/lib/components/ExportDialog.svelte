<script lang="ts">
  // "Export audio": says what goes into the file, lets the musician decide
  // about the click (whatever the mixer has it set to) and where it goes.
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  const CLICK_KEY = "joshua.export.click";

  let dialog: HTMLDialogElement | undefined = $state();
  let withClick = $state(readClick());

  function readClick(): boolean {
    try {
      return localStorage.getItem(CLICK_KEY) === "1";
    } catch {
      return false;
    }
  }

  $effect(() => {
    if (!dialog) return;
    if (app.exportDialogOpen && !dialog.open) {
      dialog.showModal();
      void app.refreshInfo();
    } else if (!app.exportDialogOpen && dialog.open) dialog.close();
  });

  function start(): void {
    try {
      localStorage.setItem(CLICK_KEY, withClick ? "1" : "0");
    } catch {
      // Remembering the choice is a convenience only.
    }
    void app.exportMix(withClick);
  }
</script>

<dialog bind:this={dialog} onclose={() => (app.exportDialogOpen = false)} aria-labelledby="export-title">
  <header>
    <h2 id="export-title">{i18n.t("export.dialogTitle")}</h2>
    <button class="icon-btn" title={i18n.t("common.close")} onclick={() => (app.exportDialogOpen = false)}>
      <Icon name="close" />
    </button>
  </header>

  <p class="what">{i18n.t("export.what")}</p>

  <label class="click" class:on={withClick}>
    <input type="checkbox" bind:checked={withClick} />
    <span class="box" aria-hidden="true"><Icon name="click" size={16} /></span>
    <span class="text">
      <strong>{i18n.t("export.withClick")}</strong>
      <span>{withClick ? i18n.t("export.withClickOn") : i18n.t("export.withClickOff")}</span>
    </span>
  </label>

  <div class="dest">
    <span class="section-title">{i18n.t("export.saveTo")}</span>
    <code title={app.info?.exportDir ?? ""}>{app.info?.exportDir ?? "—"}</code>
    <button class="link" onclick={() => app.chooseExportDir()}>{i18n.t("exports.changeFolder")}</button>
  </div>

  <footer>
    <button class="btn" onclick={() => (app.exportDialogOpen = false)}>{i18n.t("common.cancel")}</button>
    <button class="btn primary" onclick={start}>
      <Icon name="export" size={14} />
      {i18n.t("export.start")}
    </button>
  </footer>
</dialog>

<style>
  dialog {
    width: min(460px, calc(100vw - 32px));
    padding: 0 22px 18px;
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
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 18px 0 6px;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-title);
  }
  .what {
    margin: 0 0 14px;
    font-size: var(--fs-body);
    line-height: 1.45;
    color: var(--text-dim);
  }
  .click {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: var(--radius);
    border: 1px solid var(--line-strong);
    cursor: pointer;
  }
  .click.on {
    border-color: color-mix(in srgb, var(--accent) 60%, var(--line));
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .click input {
    width: 18px;
    height: 18px;
    flex: none;
    accent-color: var(--accent);
  }
  .box {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 8px;
    color: var(--text-dim);
    background: var(--bg-raised);
  }
  .click.on .box {
    color: var(--accent);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .text strong {
    font-size: var(--fs-body);
    color: var(--text);
  }
  .dest {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 14px;
  }
  code {
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--bg-raised);
    font-size: var(--fs-small);
    color: var(--text-dim);
    overflow-wrap: anywhere;
  }
  .link {
    align-self: flex-start;
    padding: 0;
    color: var(--accent);
    font-weight: 600;
    font-size: var(--fs-small);
    background: none;
  }
  .link:hover {
    text-decoration: underline;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 18px;
  }
</style>
