<script lang="ts">
  // Folder badge: a small rounded tile tinted with the folder colour and a
  // pictogram in that colour (the same one every time for a given folder).
  import { FOLDER_GLYPHS, glyphFor, type FolderGlyphName } from "../folderGlyph";

  let {
    id,
    color,
    glyph = undefined,
    size = 20,
  }: { id: string | null; color: string | null; glyph?: FolderGlyphName | undefined; size?: number } = $props();
  const path = $derived(FOLDER_GLYPHS[glyph ?? (id ? glyphFor(id) : "folder")]);
</script>

<span class="tile" class:none={!color} style={`--c:${color ?? "var(--text-faint)"};--s:${size}px`} aria-hidden="true">
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <path d={path} />
  </svg>
</span>

<style>
  .tile {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--s);
    height: var(--s);
    border-radius: calc(var(--s) * 0.3);
    color: var(--c);
    background: color-mix(in srgb, var(--c) 18%, transparent);
  }
  .tile.none {
    background: none;
    border: 1.5px dashed color-mix(in srgb, var(--c) 70%, transparent);
  }
  svg {
    width: 62%;
    height: 62%;
  }
</style>
