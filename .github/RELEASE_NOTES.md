**Joshua+ 0.5.3 — making music at home.** Split songs into instruments, play along with a click locked to the song, record yourself and share the video.

### Install

- **macOS / Linux:** `curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh`
- **Windows (PowerShell):** `irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex`

The scripts verify every download against `SHA256SUMS`. Platforms: macOS on Apple Silicon (13.4+), Windows 10/11 x64, Linux x86_64 (glibc 2.39+). Installation tested on Ubuntu 26.04 LTS, Omarchy (Arch Linux), Windows 11 and a MacBook Pro M4 Pro; every installer below was also installed and started on clean Windows, macOS and Linux machines before this release was published. Builds are not code-signed yet (see the README). Joshua+ 0.5.1 and 0.5.2 tell you about this release by themselves.

### New in 0.5.3

- **Count-in before the intro, click through it.** The count-in always ends before the music starts, even when the song opens with an intro, and the click then carries on through the intro in time instead of waiting for the first detected beat.
- **Tempo in the right pulse.** Heavy grooves with dotted-eighth figures were counted in their dotted pulse (4/3 of the tempo). The tempo is now the one at which the song repeats bar after bar: In the Name of the Father 100, Paparazzi 105, Conclave 80, Dionysus 140 (all were off before).
- **Tighter beats.** A tiny tempo error no longer drifts over a song (Fearless: beats within 20 ms of the drums 47% → 80%); bands playing live without a click are followed beat by beat (a live session: 35% → 76%); and beats land on the start of the hits instead of 11–14 ms early. Press ↻ on the metronome to re-detect songs you imported before.
- **Your typed tempo is kept.** Re-detecting no longer loses a BPM you typed: it shows under the BPM as "Previous: 100 BPM", with Use to put it back.
- **Settings is a page** in the sidebar, in the same style as About; About shows the authors under Contributors.
- The **Joshua+ name, logo and icon** are not part of the MIT License (see TRADEMARKS.md); the code stays MIT.

### Since 0.0.1-beta

- Frame-by-frame video export, social formats cropped not squeezed, Balance with the original and Reset levels, a flat monospace design (0.5.2).
- New-version notice, Linux AppImage in the application menu, install scripts without the GitHub API (0.5.1).
- One take per input aligned with the song, a shared **Room** for takes, a **Video studio** with any webcam, an **Exports** page, plug-and-play interfaces and cameras (0.5.0).
- Local stem separation (HT-Demucs 6-stem), a click locked to the song, loops, setlists, up to 8 inputs, MIDI and e-drum kits from ten brands. English, Português (Brasil) and Deutsch.
