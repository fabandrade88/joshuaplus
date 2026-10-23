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
    <section class="card" style="--c: var(--accent)">
      <span class="badge"><Icon name="music" size={18} /></span>
      <h2>{i18n.t("about.createdBy")}</h2>
      <p class="big">Fabricio O. de Andrade</p>
    </section>

    <section class="card" style="--c: var(--folder-violet)">
      <span class="badge"><Icon name="edit" size={18} /></span>
      <h2>{i18n.t("about.repo")}</h2>
      <code>github.com/fabandrade88/joshuaplus</code>
      <div class="actions">
        <button class="btn small" onclick={() => openUrl(REPO)}>{i18n.t("about.openRepo")} ↗</button>
        <button class="btn small ghost" onclick={() => openUrl(`${REPO}/issues`)}>{i18n.t("about.issues")} ↗</button>
      </div>
    </section>

    <section class="card" style="--c: var(--ok)">
      <span class="badge"><Icon name="help" size={18} /></span>
      <h2>{i18n.t("about.license")}</h2>
      <p>{i18n.t("about.licenseText")}</p>
    </section>
  </div>

  <section class="card wide" style="--c: var(--stem-vocals)">
    <span class="badge"><Icon name="spark" size={18} /></span>
    <h2>{i18n.t("about.contributors")}</h2>
    <ul class="people">
      {#each CONTRIBUTORS as c (c.github)}
        <li>
          <span class="avatar">{c.name.split(" ").map((w) => w[0]).slice(0, 2).join("")}</span>
          <span><strong>{c.name}</strong><small>{c.role} · @{c.github}</small></span>
        </li>
      {/each}
    </ul>
    <button class="link" onclick={() => openUrl(`${REPO}/graphs/contributors`)}>{i18n.t("about.allContributors")} ↗</button>
  </section>

  <section class="card wide" style="--c: var(--stem-piano)">
    <span class="badge"><Icon name="plug" size={18} /></span>
    <h2>{i18n.t("about.builtWith")}</h2>
    <dl>
      {#each CREDITS as c (c.name)}
        <dt>{c.name}</dt>
        <dd>{c.what}</dd>
        <dd class="lic">{c.license}</dd>
      {/each}
    </dl>
  </section>
</div>

<style>
  .about {
    flex: 1;
    overflow-y: auto;
    padding: 28px 40px 48px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .about > * {
    flex: none;
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 20px;
    flex-wrap: wrap;
    padding: 20px 22px;
    border-radius: var(--radius-l);
    background: color-mix(in srgb, var(--accent) 7%, var(--bg-raised));
    border: 1px solid color-mix(in srgb, var(--accent) 30%, var(--line));
  }
  .hero-text {
    flex: 1;
    min-width: 220px;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    color: var(--accent);
  }
  .hero p {
    margin: 4px 0 0;
    color: var(--text-dim);
  }
  .version {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-faint);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 14px;
  }
  .card {
    position: relative;
    overflow: hidden;
    padding: 18px 20px 18px 24px;
    border-radius: var(--radius-l);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .card::before {
    content: "";
    position: absolute;
    inset: 0 auto 0 0;
    width: 4px;
    background: var(--c);
  }
  .badge {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 10px;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 16%, transparent);
  }
  h2 {
    margin: 0;
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: color-mix(in srgb, var(--c) 70%, var(--text));
  }
  .card p {
    margin: 0;
    color: var(--text-dim);
  }
  .card .big {
    font-size: 18px;
    font-weight: 800;
    color: var(--text);
  }
  code {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text);
    user-select: text;
  }
  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .small {
    height: 30px;
    padding: 0 12px;
    font-size: 12.5px;
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
    gap: 12px;
  }
  .people li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px 8px 8px;
    border-radius: 999px;
    background: var(--bg-panel);
    border: 1px solid var(--line);
  }
  .people strong {
    display: block;
  }
  .people small {
    color: var(--text-faint);
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    font-size: 12px;
    font-weight: 800;
    color: var(--accent-ink);
    background: var(--c);
  }
  .link {
    align-self: flex-start;
    font-size: 12.5px;
    font-weight: 700;
    color: var(--c);
  }
  .link:hover {
    text-decoration: underline;
  }
  dl {
    display: grid;
    grid-template-columns: minmax(160px, 220px) 1fr auto;
    margin: 0;
  }
  dt,
  dd {
    margin: 0;
    padding: 7px 0;
    border-top: 1px solid var(--line);
  }
  dt:first-of-type,
  dt:first-of-type + dd,
  dt:first-of-type + dd + dd {
    border-top: 0;
  }
  dt {
    font-weight: 700;
  }
  dd {
    color: var(--text-dim);
  }
  .lic {
    font-family: var(--mono);
    font-size: 11.5px;
    color: var(--text-faint);
    text-align: right;
  }
</style>
