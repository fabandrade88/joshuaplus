import { describe, expect, it } from "vitest";
import { compactDeviceName, hasMatchingAudio } from "./devices";

const mac = ["Yamaha AG06MK2", "MacBook Pro Microphone", "SPD-SX PRO"];

describe("hasMatchingAudio", () => {
  it("finds modules that also stream audio", () => {
    expect(hasMatchingAudio("SPD-SX PRO", mac)).toBe(true);
    expect(hasMatchingAudio("Yamaha AG06MK2", mac)).toBe(true);
  });

  it("flags MIDI-only modules", () => {
    expect(hasMatchingAudio("Alesis Prime Drum Module", mac)).toBe(false);
    expect(hasMatchingAudio("Alesis Prime Drum Module MIDI", [])).toBe(false);
  });

  it("ignores Windows and Linux port decorations", () => {
    expect(hasMatchingAudio("MIDIIN2 (SPD-SX PRO)", ["IN (SPD-SX PRO)"])).toBe(true);
    expect(hasMatchingAudio("Strata Prime:Strata Prime MIDI 1 24:0", ["Strata Prime, USB Audio"])).toBe(true);
    expect(compactDeviceName("MIDIOUT2 (TD-27)")).toBe("td27");
  });

  it("never matches on tiny names", () => {
    expect(hasMatchingAudio("In", ["Line In"])).toBe(false);
  });
});
