<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/app.svelte";
  import { i18n } from "./lib/i18n/index.svelte";
  import AboutView from "./lib/components/AboutView.svelte";
  import FolderView from "./lib/components/FolderView.svelte";
  import GearView from "./lib/components/GearView.svelte";
  import HelpView from "./lib/components/HelpView.svelte";
  import Home from "./lib/components/Home.svelte";
  import SettingsDialog from "./lib/components/SettingsDialog.svelte";
  import VideoStudio from "./lib/components/VideoStudio.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import BarMap from "./lib/components/BarMap.svelte";
  import Console from "./lib/components/Console.svelte";
  import SongDeck from "./lib/components/SongDeck.svelte";
  import Welcome from "./lib/components/Welcome.svelte";

  onMount(() => {
    void app.init();
    // Block the webview's default navigation on dropped files.
    const prevent = (e: DragEvent) => e.preventDefault();
    window.addEventListener("dragover", prevent);
    window.addEventListener("drop", prevent);
    return () => {
      window.removeEventListener("dragover", prevent);
      window.removeEventListener("drop", prevent);
    };
  });

  $effect(() => {
    document.documentElement.lang = i18n.locale;
  });

  function onKey(e: KeyboardEvent): void {
    const target = e.target as HTMLElement | null;
    if (target?.closest("input, select, textarea, dialog")) return;
    if (app.view.kind !== "song") return;
    if (e.code === "Space") {
      e.preventDefault();
      void app.togglePlay();
    } else if (e.code === "Home" || e.code === "Enter") {
      e.preventDefault();
      void app.seek(app.loop && app.loopEnabled ? app.loop.start : 0);
    } else if (e.code === "KeyL") {
      void app.toggleLoop();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="shell">
  {#if !app.ready}
    <div class="splash"></div>
  {:else if !app.hasWorkspace}
    <Welcome />
  {:else}
    <Sidebar />
    <main>
      {#if app.view.kind === "song" && app.song}
        <SongDeck />
        <BarMap />
        <Console />
      {:else if app.view.kind === "about"}
        <AboutView />
      {:else if app.view.kind === "help"}
        <HelpView />
      {:else if app.view.kind === "gear"}
        <GearView />
      {:else if app.view.kind === "folder"}
        <FolderView folderKey={app.view.folder} />
      {:else}
        <Home />
      {/if}
    </main>
  {/if}

  {#if app.toast}
    <div class="toast" role="status">{app.toast}</div>
  {/if}
  {#if app.transport.deviceError}
    <div class="toast err" role="alert">{i18n.t("error.audio_engine")}</div>
  {/if}
</div>

<SettingsDialog />
{#if app.videoOpen}<VideoStudio />{/if}

<style>
  .shell {
    display: flex;
    height: 100%;
  }
  .splash {
    flex: 1;
    background: var(--bg);
  }
  main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    translate: -50% 0;
    max-width: min(560px, calc(100vw - 32px));
    padding: 10px 16px;
    border-radius: var(--radius);
    background: var(--bg-active);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    font-weight: 600;
    animation: rise 0.2s ease-out;
  }
  .toast.err {
    bottom: 72px;
    border-color: var(--danger);
    color: var(--danger);
  }
  @keyframes rise {
    from {
      opacity: 0;
      translate: -50% 8px;
    }
  }
</style>
