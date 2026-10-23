<script lang="ts">
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";

  const stems = ["vocals", "drums", "bass", "guitar", "piano", "other"] as const;
</script>

<div class="welcome">
  <div class="card">
    <Logo />
    <h1>{i18n.t("welcome.title")}</h1>
    <p>{i18n.t("welcome.body")}</p>
    <div class="stems" aria-hidden="true">
      {#each stems as stem, i (stem)}
        <span style={`--c:var(--stem-${stem});--d:${i * 90}ms`}></span>
      {/each}
    </div>
    <button class="btn primary big" onclick={() => app.chooseWorkspace()}>
      <Icon name="folder" />
      {i18n.t("welcome.choose")}
    </button>
    <p class="hint">{i18n.t("welcome.hint")}</p>
  </div>
</div>

<style>
  .welcome {
    flex: 1;
    display: grid;
    place-items: center;
    padding: 32px;
    background:
      radial-gradient(1200px 500px at 50% -10%, color-mix(in srgb, var(--accent) 12%, transparent), transparent 60%),
      var(--bg);
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
    font-size: 34px;
    letter-spacing: -0.03em;
    line-height: 1.1;
  }
  p {
    margin: 0;
    color: var(--text-dim);
    font-size: 14.5px;
    line-height: 1.6;
  }
  .stems {
    display: flex;
    gap: 6px;
    height: 42px;
    align-items: center;
  }
  .stems span {
    width: 34px;
    height: 100%;
    border-radius: 6px;
    background: var(--c);
    opacity: 0.9;
    animation: bounce 1.4s var(--d) ease-in-out infinite alternate;
  }
  @keyframes bounce {
    from {
      transform: scaleY(0.35);
    }
    to {
      transform: scaleY(1);
    }
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
