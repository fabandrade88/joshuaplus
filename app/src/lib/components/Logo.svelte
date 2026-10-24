<script lang="ts">
  import { i18n } from "../i18n/index.svelte";
  let { compact = false }: { compact?: boolean } = $props();
</script>

<div class="logo" class:compact>
  <svg class="mark" viewBox="0 0 40 40" aria-hidden="true">
    <rect width="40" height="40" rx="11" fill="var(--accent)" />
    <g fill="var(--accent-ink)">
      <rect x="7" y="16" width="3" height="8" rx="1.5" />
      <rect x="12" y="11" width="3" height="18" rx="1.5" />
      <rect x="17" y="14" width="3" height="12" rx="1.5" />
      <rect x="25" y="13" width="3" height="14" rx="1.5" />
      <rect x="19.5" y="18.5" width="14" height="3" rx="1.5" />
    </g>
  </svg>
  {#if !compact}
    <div>
      <div class="word">Joshua<span>+</span><em class="beta">beta</em></div>
      <div class="tag">{i18n.t("app.tagline")}</div>
    </div>
  {/if}
</div>

<style>
  .logo {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .mark {
    width: 36px;
    height: 36px;
    flex: none;
  }
  .word {
    font-size: var(--fs-title);
    font-weight: 800;
    letter-spacing: -0.02em;
  }
  .word span {
    color: var(--accent);
  }
  .beta {
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 5px;
    vertical-align: 3px;
    font-size: var(--fs-label);
    font-style: normal;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--accent-ink);
    background: var(--accent);
  }
  .tag {
    font-size: var(--fs-small);
    color: var(--text-faint);
    margin-top: -2px;
  }
</style>
