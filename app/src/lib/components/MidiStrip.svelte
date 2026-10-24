<script lang="ts">
  // Console strip for the connected e-drum kit (MIDI). It shows hits, not
  // audio: the meter follows strike velocity and the last pad played. When the
  // module sends no audio over USB, the strip says so and links to Inputs.
  import { app } from "../app.svelte";
  import type { MessageKey } from "../i18n/en";
  import { i18n } from "../i18n/index.svelte";
  import MeterStrip from "./MeterStrip.svelte";

  const DECAY_MS = 450;

  let level = $state(0);

  $effect(() => {
    const hit = app.lastNote;
    if (!hit) return;
    let raf = 0;
    const tick = () => {
      const t = (performance.now() - hit.at) / DECAY_MS;
      level = t >= 1 ? 0 : (hit.velocity / 127) * (1 - t);
      if (t < 1) raf = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(raf);
  });

  const padLabel = $derived(
    app.lastNote?.pad ? i18n.t(`pad.${app.lastNote.pad}` as MessageKey) : app.lastNote ? `#${app.lastNote.note}` : "—",
  );
</script>

<MeterStrip
  kind="drums"
  name={app.drums?.connected ?? ""}
  color="var(--stem-drums)"
  tag="MIDI"
  {level}
  caption={padLabel}
  note={app.drumsMidiOnly ? i18n.t("console.noAudio") : undefined}
  noteTitle={i18n.t("gear.midiOnly", { name: app.drums?.connected ?? "" })}
  onnote={() => app.openGear()}
/>
