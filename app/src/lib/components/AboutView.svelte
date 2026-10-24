<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app } from "../app.svelte";
  import { i18n } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";

  const REPO = "https://github.com/fabandrade88/joshuaplus";

  // Project contributors (kept in sync with the repository).
  const CONTRIBUTORS = [{ name: "Fabricio O. de Andrade", role: "Creator & maintainer", github: "fabandrade88" }];

  // Main open-source building blocks (full list with licenses in the README).
  const CREDITS: { name: string; what: string; license: string }[] = [
    { name: "Tauri", what: "Desktop app framework", license: "MIT / Apache-2.0" },
    { name: "Svelte", what: "User interface", license: "MIT" },
    { name: "Rust", what: "Language and standard library", license: "MIT / Apache-2.0" },
    { name: "HT-Demucs (Meta AI)", what: "Studio separation model", license: "MIT" },
    { name: "ONNX Runtime + ort", what: "Neural inference", license: "MIT / Apache-2.0" },
    { name: "Symphonia", what: "Audio decoding", license: "MPL-2.0" },
    { name: "cpal", what: "Audio input/output", license: "Apache-2.0" },
    { name: "midir", what: "MIDI input", license: "MIT" },
    { name: "rustfft · rubato · hound", what: "DSP, resampling, WAV", license: "MIT / Apache-2.0" },
    { name: "image", what: "Cover art", license: "MIT / Apache-2.0" },
    { name: "Mediabunny", what: "Video encoding", license: "MPL-2.0" },
    { name: "JetBrains Mono", what: "Interface font", license: "OFL-1.1" },
  ];
</script>

<div class="about">
  <header class="hero">
    <Logo />
    <div class="hero-text">
      <h1>{i18n.t("about.title")}</h1>
      <p>{i18n.t("about.beta")}</p>
    </div>
    <span class="version tabular">{i18n.t("about.version")} {app.info?.version ?? ""}</span>
  </header>

  <div class="grid">
    <section class="card" style="--c: var(--accent)" aria-labelledby="about-created">
      <div class="card-head">
        <span class="badge"><Icon name="music" size={14} /></span>
        <h2 id="about-created">{i18n.t("about.createdBy")}</h2>
      </div>
      <p class="big">Fabricio O. de Andrade</p>
    </section>

    <section class="card" style="--c: var(--folder-violet)" aria-labelledby="about-repo">
      <div class="card-head">
        <span class="badge"><Icon name="edit" size={14} /></span>
        <h2 id="about-repo">{i18n.t("about.repo")}</h2>
      </div>
      <code>github.com/fabandrade88/<wbr />joshuaplus</code>
      <div class="actions">
        <button class="btn small" onclick={() => openUrl(REPO)}>{i18n.t("about.openRepo")} ↗</button>
        <button class="btn small ghost" onclick={() => openUrl(`${REPO}/issues`)}>{i18n.t("about.issues")} ↗</button>
      </div>
    </section>

    <section class="card" style="--c: var(--ok)" aria-labelledby="about-license">
      <div class="card-head">
        <span class="badge"><Icon name="help" size={14} /></span>
        <h2 id="about-license">{i18n.t("about.license")}</h2>
      </div>
      <p>{i18n.t("about.licenseText")}</p>
    </section>

    <section class="card" style="--c: var(--stem-vocals)" aria-labelledby="about-people">
      <div class="card-head">
        <span class="badge"><Icon name="spark" size={14} /></span>
        <h2 id="about-people">{i18n.t("about.contributors")}</h2>
      </div>
      <ul class="people">
        {#each CONTRIBUTORS as c (c.github)}
          <li>
            <span class="avatar" aria-hidden="true">{c.name.split(" ").map((w) => w[0]).slice(0, 2).join("")}</span>
            <span class="who"><strong>{c.name}</strong><small>{c.role} · @{c.github}</small></span>
          </li>
        {/each}
      </ul>
      <button class="link" onclick={() => openUrl(`${REPO}/graphs/contributors`)}>{i18n.t("about.allContributors")} ↗</button>
    </section>
  </div>

  <section class="card" style="--c: var(--stem-piano)" aria-labelledby="about-credits">
    <div class="card-head">
      <span class="badge"><Icon name="plug" size={14} /></span>
      <h2 id="about-credits">{i18n.t("about.builtWith")}</h2>
    </div>
    <dl class="credits">
      {#each CREDITS as c (c.name)}
        <div class="credit">
          <dt>{c.name}</dt>
          <dd>{c.what}</dd>
          <dd class="lic">{c.license}</dd>
        </div>
      {/each}
    </dl>
  </section>
</div>

<style>
  .about {
    flex: 1;
    overflow-y: auto;
    padding: 16px 24px 28px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .about > * {
    flex: none;
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 8px 18px;
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
    color: var(--text);
  }
  .hero p {
    margin: 2px 0 0;
    font-size: var(--fs-small);
    color: var(--text-dim);
  }
  .version {
    font-family: var(--mono);
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
  }
  .card {
    position: relative;
    overflow: hidden;
    min-width: 0;
    padding: 10px 14px 12px 18px;
    border-radius: var(--radius);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 6px;
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
  h2 {
    margin: 0;
    font-size: var(--fs-label);
    font-weight: 600;
    color: color-mix(in srgb, var(--c) 70%, var(--text));
  }
  .card p {
    margin: 0;
    font-size: var(--fs-body);
    color: var(--text-dim);
  }
  .card .big {
    font-size: var(--fs-emph);
    font-weight: 600;
    color: var(--text);
  }
  code {
    font-family: var(--mono);
    font-size: var(--fs-small);
    color: var(--text);
    overflow-wrap: anywhere;
    user-select: text;
  }
  .actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .small {
    height: 26px;
    padding: 0 10px;
    font-size: var(--fs-body);
  }
  .ghost {
    background: transparent;
  }
  .people {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .people li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .who {
    min-width: 0;
    font-size: var(--fs-body);
  }
  .people strong {
    display: block;
  }
  .people small {
    display: block;
    font-size: var(--fs-small);
    color: var(--text-faint);
  }
  .avatar {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-s);
    font-size: var(--fs-label);
    font-weight: 600;
    color: var(--accent-ink);
    background: var(--c);
  }
  .link {
    align-self: flex-start;
    font-size: var(--fs-small);
    font-weight: 600;
    text-align: left;
    color: var(--c);
  }
  .link:hover {
    text-decoration: underline;
  }
  .credits {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(380px, 1fr));
    column-gap: 28px;
    margin: 0;
  }
  .credit {
    display: grid;
    grid-template-columns: minmax(120px, 40%) 1fr auto;
    gap: 10px;
    align-items: baseline;
    padding: 4px 0;
    border-bottom: 1px solid var(--line);
    font-size: var(--fs-small);
  }
  dt,
  dd {
    margin: 0;
    min-width: 0;
  }
  dt {
    font-weight: 600;
  }
  dd {
    color: var(--text-dim);
  }
  .lic {
    font-family: var(--mono);
    font-size: var(--fs-small);
    color: var(--text-faint);
    text-align: right;
    white-space: nowrap;
  }
  @media (max-width: 1180px) {
    .grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-width: 1100px) {
    .about {
      padding: 12px 16px 20px;
    }
  }
</style>
