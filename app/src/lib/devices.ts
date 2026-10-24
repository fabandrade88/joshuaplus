// Matching MIDI ports to audio devices by name, so the app can tell when a
// USB device (typically an e-drum module) sends MIDI but no audio.

/**
 * Lowercase alphanumerics without port decorations: "MIDIIN2 (…)" on
 * Windows, "Card:Port 24:0" on Linux (only the card part is kept) and
 * "USB Audio" suffixes.
 */
export function compactDeviceName(name: string): string {
  return (name.split(":")[0] ?? name)
    .toLowerCase()
    .replace(/usb\s*audio/g, "")
    .replace(/midi(in|out)?\d*/g, "")
    .replace(/\b(port|in|out)\s*\d*\b/g, "")
    .replace(/[^a-z0-9]/g, "");
}

/** Whether `midiName` looks like the same hardware as one of `audioNames`. */
export function hasMatchingAudio(midiName: string, audioNames: readonly string[]): boolean {
  const m = compactDeviceName(midiName);
  if (m.length < 3) return false;
  return audioNames.some((a) => {
    const c = compactDeviceName(a);
    return c.length >= 3 && (c.includes(m) || m.includes(c));
  });
}
