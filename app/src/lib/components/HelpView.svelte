<script lang="ts">
  import { onMount } from "svelte";
  import { filterHelp, helpCatalogs } from "../help";
  import { i18n } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  type IconName = "drop" | "folder" | "spark" | "play" | "loop" | "volume" | "click" | "music" | "plug" | "settings" | "edit" | "help";

  // Each section borrows a colour the app already uses (accent, stems, folders).
  const STYLE: Record<string, { color: string; icon: IconName }> = {
    start: { color: "var(--accent)", icon: "drop" },
    library: { color: "var(--folder-violet)", icon: "folder" },
    engines: { color: "var(--stem-piano)", icon: "spark" },
    song: { color: "var(--stem-vocals)", icon: "play" },
    bars: { color: "var(--accent)", icon: "loop" },
    mixer: { color: "var(--stem-guitar)", icon: "volume" },
    metronome: { color: "var(--stem-drums)", icon: "click" },
    info: { color: "var(--stem-acoustic)", icon: "music" },
    gear: { color: "var(--ok)", icon: "plug" },
    settings: { color: "var(--stem-other)", icon: "settings" },
    keys: { color: "var(--accent)", icon: "edit" },
    privacy: { color: "var(--stem-bass)", icon: "help" },
  };
  const style = (id: string) => STYLE[id] ?? { color: "var(--accent)", icon: "help" as IconName };

  let query = $state("");
  let content: HTMLDivElement | undefined = $state();
  let active = $state("start");

  const sections = $derived(filterHelp(helpCatalogs[i18n.locale], query));

  function jump(id: string): void {
    active = id;
    content?.querySelector(`#help-${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
  }

  // Scroll spy: highlight the section being read in the index.
  onMount(() => {
    if (!content) return;
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        const first = visible[0]?.target.id.replace("help-", "");
        if (first) active = first;
      },
      { root: content, rootMargin: "0px 0px -65% 0px" },
    );
    const watch = () => content?.querySelectorAll("section[id]").forEach((el) => observer.observe(el));
    watch();
    const mo = new MutationObserver(() => {
      observer.disconnect();
      watch();
    });
    mo.observe(content, { childList: true });
    return () => {
      observer.disconnect();
      mo.disconnect();
    };
  });
</script>

<div class="help">
  <nav class="toc" aria-label={i18n.t("nav.help")}>
    <h1>{i18n.t("nav.help")}</h1>
    <label class="search">
      <Icon name="help" size={14} />
      <input bind:value={query} placeholder={i18n.t("help.search")} aria-label={i18n.t("help.search")} />
    </label>
    <ul>
      {#each sections as s (s.id)}
        <li>
          <button class:active={active === s.id} style={`--c:${style(s.id).color}`} onclick={() => jump(s.id)}>
            <i class="dot"></i>{s.title}
          </button>
        </li>
      {/each}
    </ul>
  </nav>

  <div class="content" bind:this={content}>
    {#if !query}
      <header class="hero">
        <div class="hero-text">
          <h2>{i18n.t("help.heroTitle")}</h2>
          <p>{i18n.t("help.heroText")}</p>
        </div>
        <div class="tips">
          <span style="--c: var(--stem-piano)"><Icon name="drop" size={13} />{i18n.t("help.tip1")}</span>
          <span style="--c: var(--stem-vocals)"><Icon name="loop" size={13} />{i18n.t("help.tip2")}</span>
          <span style="--c: var(--stem-guitar)"><Icon name="click" size={13} />{i18n.t("help.tip3")}</span>
        </div>
        <div class="bars" aria-hidden="true">
          {#each ["vocals", "drums", "bass", "guitar", "acoustic", "piano", "other"] as stem, i (stem)}
            <i style={`--c:var(--stem-${stem});--d:${i * 110}ms`}></i>
          {/each}
        </div>
      </header>
    {/if}

    {#if sections.length === 0}
      <p class="empty">{i18n.t("help.noResults")}</p>
    {/if}
    {#each sections as s (s.id)}
      {@const st = style(s.id)}
      <section id={`help-${s.id}`} class="card" style={`--c:${st.color}`}>
        <header class="card-head">
          <span class="badge"><Icon name={st.icon} size={18} /></span>
          <h2>{s.title}</h2>
        </header>
        {#if s.intro}<p class="intro">{s.intro}</p>{/if}
        <dl class:keys={s.id === "keys"}>
          {#each s.items as item (item.term)}
            <dt>{#if s.id === "keys"}<kbd>{item.term}</kbd>{:else}{item.term}{/if}</dt>
            <dd>{item.text}</dd>
          {/each}
        </dl>
      </section>
    {/each}
  </div>
</div>

<style>
  .help {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .toc {
    width: 250px;
    flex: none;
    padding: 28px 16px 24px 32px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
  }
  h1 {
    margin: 0;
    font-size: 24px;
    letter-spacing: -0.03em;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 36px;
    padding: 0 10px;
    border-radius: var(--radius);
    border: 1px solid var(--line-strong);
    background: var(--bg);
    color: var(--accent);
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .search:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    outline: none;
    user-select: text;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  ul button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    padding: 7px 10px;
    border-radius: var(--radius-s);
    color: var(--text-dim);
    font-weight: 600;
    transition: background 0.12s, color 0.12s;
  }
  ul button:hover {
    background: var(--bg-hover);
    color: var(--text);
  }
  ul button.active {
    color: var(--text);
    background: color-mix(in srgb, var(--c) 14%, transparent);
  }
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 3px;
    background: var(--c);
    opacity: 0.55;
  }
  .active .dot {
    opacity: 1;
    box-shadow: 0 0 8px var(--c);
  }
  .content {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 28px 40px 60px 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    user-select: text;
  }
  .hero,
  .card {
    flex: none;
  }
  .hero {
    position: relative;
    overflow: hidden;
    padding: 24px 26px;
    border-radius: var(--radius-l);
    background: color-mix(in srgb, var(--accent) 7%, var(--bg-raised));
    border: 1px solid color-mix(in srgb, var(--accent) 30%, var(--line));
  }
  .hero h2 {
    margin: 0;
    font-size: 22px;
    letter-spacing: -0.02em;
    color: var(--accent);
  }
  .hero p {
    margin: 6px 0 0;
    color: var(--text-dim);
    max-width: 560px;
  }
  .tips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 16px;
  }
  .tips span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 11px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 700;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--c) 30%, transparent);
  }
  .bars {
    position: absolute;
    right: 26px;
    top: 22px;
    display: flex;
    align-items: flex-end;
    gap: 5px;
    height: 54px;
    opacity: 0.9;
  }
  .bars i {
    width: 9px;
    height: 100%;
    border-radius: 3px;
    background: var(--c);
    transform-origin: bottom;
    animation: bounce 1.3s var(--d) ease-in-out infinite alternate;
  }
  @keyframes bounce {
    from {
      transform: scaleY(0.3);
    }
    to {
      transform: scaleY(1);
    }
  }
  .card {
    position: relative;
    padding: 18px 20px 18px 24px;
    border-radius: var(--radius-l);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    scroll-margin-top: 16px;
    overflow: hidden;
  }
  .card::before {
    content: "";
    position: absolute;
    inset: 0 auto 0 0;
    width: 4px;
    background: var(--c);
  }
  .card-head {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 10px;
  }
  .badge {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 16%, transparent);
  }
  .card h2 {
    margin: 0;
    font-size: 16px;
  }
  .intro {
    margin: 0 0 12px;
    color: var(--text-dim);
    line-height: 1.6;
  }
  dl {
    display: grid;
    grid-template-columns: minmax(150px, 210px) 1fr;
    gap: 0;
    margin: 0;
  }
  dt,
  dd {
    padding: 9px 0;
    border-top: 1px solid var(--line);
  }
  dt:first-of-type,
  dd:first-of-type {
    border-top: 0;
  }
  dt {
    font-weight: 700;
    color: color-mix(in srgb, var(--c) 65%, var(--text));
    padding-right: 16px;
  }
  dd {
    margin: 0;
    color: var(--text-dim);
    line-height: 1.55;
  }
  kbd {
    display: inline-block;
    padding: 3px 9px;
    border-radius: 7px;
    border: 1px solid color-mix(in srgb, var(--accent) 50%, transparent);
    border-bottom-width: 3px;
    background: color-mix(in srgb, var(--accent) 12%, var(--bg-panel));
    color: var(--accent);
    font-family: var(--mono);
    font-size: 12px;
    font-weight: 700;
  }
  .empty {
    color: var(--text-faint);
  }
</style>
