<script lang="ts">
  import type { StemKind } from "../types";

  // Instrument pictograms (24×24, stroke).
  const PATHS: Record<StemKind | "metronome" | "master" | "song", string> = {
    vocals: "M12 3a3 3 0 0 1 3 3v5a3 3 0 0 1-6 0V6a3 3 0 0 1 3-3zM6 11a6 6 0 0 0 12 0M12 17v4M9 21h6",
    drums: "M4 9c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 9v7c0 1.7 3.6 3 8 3s8-1.3 8-3V9M8 3l3 5M16 3l-3 5",
    bass: "M15 3l6 6-2 2-6-6zM14 8 6.5 15.5M8.5 13.5a3 3 0 1 0-1 5.5c1.5 0 2.5-1 3-2.5l1-1.5",
    guitar: "M16 3l5 5-2 2-5-5zM13 8l-5 5M9.5 11a4 4 0 0 0-5.3 1.2c-1.4 2 .2 4.1 1.6 5.5s3.5 3 5.5 1.6a4 4 0 0 0 1.2-5.3",
    acoustic: "M16 3l5 5-2 2-5-5zM14 7l-4 4M10 9.5a4.5 4.5 0 0 0-6.2.9c-1.6 2 0 4.6 1.8 6.4s4.4 3.4 6.4 1.8a4.5 4.5 0 0 0 .9-6.2M8.5 14.5a1 1 0 1 0 1 1",
    piano: "M3 5h18v14H3zM8 5v9M13 5v9M18 5v9M6.5 14v5M11.5 14v5M16.5 14v5",
    other: "M3 12h2l2-5 3 10 3-12 3 9 2-2h3",
    metronome: "M9 3h6l3.5 18h-13zM12 15l5-9M7 17h10",
    master: "M5 4v16M12 4v16M19 4v16M3 9h4M10 14h4M17 7h4",
    song: "M9 18V5l12-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM21 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z",
  };

  let { kind, size = 18 }: { kind: StemKind | "metronome" | "master" | "song"; size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d={PATHS[kind]} />
</svg>
