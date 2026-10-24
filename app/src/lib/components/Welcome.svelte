<script lang="ts">
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";
</script>

<div class="welcome">
  <div class="card">
    <Logo />
    <h1>{i18n.t("welcome.title")}</h1>
    <p>{i18n.t("welcome.body")}</p>
    {#if app.info?.suggestedWorkspace}
      <button class="btn primary big" onclick={() => app.useSuggestedWorkspace()}>
        <Icon name="music" />
        {i18n.t("workspace.useSuggested")}
      </button>
      <code class="suggested" title={app.info.suggestedWorkspace}>{app.info.suggestedWorkspace}</code>
      <button class="btn" onclick={() => app.chooseWorkspace()}>
        <Icon name="folder" />
        {i18n.t("workspace.chooseOther")}
      </button>
    {:else}
      <button class="btn primary big" onclick={() => app.chooseWorkspace()}>
        <Icon name="folder" />
        {i18n.t("welcome.choose")}
      </button>
    {/if}
    <p class="hint">{i18n.t("welcome.hint")}</p>
  </div>
</div>

<style>
  .suggested {
    max-width: 100%;
    margin-top: -4px;
    font-size: 12.5px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .welcome {
    flex: 1;
    display: grid;
    place-items: center;
    padding: 32px;
    background: var(--bg);
  }
  .card {
    max-width: 520px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 16px;
  }
  h1 {
    margin: 12px 0 0;
    font-size: 26px;
    font-weight: 600;
    line-height: 1.2;
  }
  p {
    margin: 0;
    color: var(--text-dim);
    font-size: var(--fs-emph);
    line-height: 1.6;
  }
  .big {
    height: 44px;
    padding: 0 22px;
    font-size: 14px;
  }
  .hint {
    font-size: 12.5px;
    color: var(--text-faint);
  }
</style>
