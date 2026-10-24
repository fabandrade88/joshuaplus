// Small music-themed pictograms for folders. Each folder always gets the
// same one (picked from its id), so the sidebar stays stable between runs.

/** 24×24 stroke paths. */
export const FOLDER_GLYPHS = {
  folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  note: "M9 18V5l11-2v12M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM20 15a3 3 0 1 1-6 0 3 3 0 0 1 6 0z",
  guitar: "M16 3l5 5-2 2-5-5zM13 8l-5 5M9.5 11a4 4 0 0 0-5.3 1.2c-1.4 2 .2 4.1 1.6 5.5s3.5 3 5.5 1.6a4 4 0 0 0 1.2-5.3",
  drum: "M4 9c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 9v7c0 1.7 3.6 3 8 3s8-1.3 8-3V9M8 3l3 5M16 3l-3 5",
  mic: "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0M12 17v4M9 21h6",
  headphones: "M4 15v-3a8 8 0 0 1 16 0v3M4 15a2 2 0 0 1 2-2h1v7H6a2 2 0 0 1-2-2zM20 15a2 2 0 0 0-2-2h-1v7h1a2 2 0 0 0 2-2z",
  keys: "M3 5h18v14H3zM8 5v9M13 5v9M18 5v9M6.5 14v5M11.5 14v5M16.5 14v5",
  vinyl: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 14.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5zM12 6.5a5.5 5.5 0 0 0-5.5 5.5",
  star: "M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z",
  heart: "M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z",
  bolt: "M13 3 5 13h6l-1 8 8-10h-6z",
  wave: "M3 12h2l2-5 3 10 3-12 3 9 2-2h3",
} as const;

export type FolderGlyphName = keyof typeof FOLDER_GLYPHS;

const NAMES = Object.keys(FOLDER_GLYPHS) as FolderGlyphName[];

function hash(id: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < id.length; i++) {
    h ^= id.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** Stable pictogram for a folder id (FNV-1a hash → glyph). */
export function glyphFor(id: string): FolderGlyphName {
  return NAMES[hash(id) % NAMES.length]!;
}

/** Pictograms for a set of folders: each starts from its hashed glyph and
 *  moves to the next free one, so folders don't repeat an icon until all
 *  twelve are used. Oldest folders choose first, so creating a new folder
 *  never changes the existing ones. */
export function glyphsFor(folders: readonly { id: string; createdUnix: number }[]): Map<string, FolderGlyphName> {
  const out = new Map<string, FolderGlyphName>();
  const used = new Set<FolderGlyphName>();
  const ordered = [...folders].sort((a, b) => a.createdUnix - b.createdUnix || a.id.localeCompare(b.id));
  for (const f of ordered) {
    if (used.size === NAMES.length) used.clear();
    let i = hash(f.id) % NAMES.length;
    while (used.has(NAMES[i]!)) i = (i + 1) % NAMES.length;
    used.add(NAMES[i]!);
    out.set(f.id, NAMES[i]!);
  }
  return out;
}
