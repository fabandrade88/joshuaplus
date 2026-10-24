import type { SongSummary } from "./types";

/**
 * Playlist order: songs listed in `order` first (in that order), then the
 * remaining songs newest first. Unknown ids in `order` are ignored.
 */
export function orderSongs(pool: SongSummary[], order: readonly string[]): SongSummary[] {
  const newestFirst = [...pool].sort((a, b) => b.createdUnix - a.createdUnix);
  if (order.length === 0) return newestFirst;
  const rank = new Map(order.map((id, i) => [id, i]));
  return newestFirst
    .map((s, i) => ({ s, k: rank.get(s.id) ?? order.length + i }))
    .sort((a, b) => a.k - b.k)
    .map((x) => x.s);
}

/** Moves `id` to `to` (0-based) and returns the new id list. */
export function moveId(ids: readonly string[], id: string, to: number): string[] {
  const out = ids.filter((x) => x !== id);
  if (out.length === ids.length) return [...ids];
  out.splice(Math.max(0, Math.min(to, out.length)), 0, id);
  return out;
}
