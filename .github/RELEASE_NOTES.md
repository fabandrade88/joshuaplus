**Joshua+ 0.5.6 — making music at home.** Split songs into instruments, play along with a click locked to the song, record yourself and share the video.

### Install

- **macOS / Linux:** `curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh`
- **Windows (PowerShell):** `irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex`

The scripts verify every download against `SHA256SUMS`. Platforms: macOS on Apple Silicon (13.4+), Windows 10/11 x64, Linux x86_64 (glibc 2.39+). Installation tested on Ubuntu 26.04 LTS, Omarchy (Arch Linux), Windows 11 and a MacBook Pro M4 Pro; every installer below was also installed and started on clean Windows, macOS and Linux machines before this release was published. Builds are not code-signed yet (see the README). Joshua+ 0.5.1 and later tell you about this release by themselves.

### New in 0.5.6

- **Takes that arrive distorted get their hits back.** An interface or drum module set too hot, or with an effect such as an amp simulator switched on, flattens the loudest hits before they reach the computer: the take sounds distorted and dull, although nothing clips in Joshua+. When a recording stops, Joshua+ now looks for that signature (every loud hit squashed at the same level, well under full scale) and rebuilds the flattened tops from the shape of the wave on either side. It is an estimate, made on your computer by plain signal processing: no model, no internet.
- **It tells you where to fix it.** The notice after recording says the level the hits were squashed at, so you can turn the gain down (or the effect off) at the interface or module, which is the real cure. It stays for 20 seconds, in English, Português and Deutsch.
- **Nothing else is touched.** Clean takes are saved exactly as they were recorded; distortion that is there on purpose (a fuzz guitar is squashed all the time, not only on the hits) is left alone; takes already in your library stay as they are.

### Since 0.0.1-beta

- Chords in every bar and time-synced lyrics to play along, in a floating window you can read from the drum stool (0.5.5).
- Exports mastered to streaming loudness with a look-ahead limiter, Tone and Body on takes, balancing that never undoes your mix, 320 kbit/s video sound at 1080p (0.5.4).
- Count-in before the intro and a click through it, tempo in the right pulse, tighter beats, typed BPM kept, Settings as a page (0.5.3).
- Frame-by-frame video export, social formats cropped not squeezed, Balance with the original and Reset levels, a flat monospace design (0.5.2).
- New-version notice, Linux AppImage in the application menu, install scripts without the GitHub API (0.5.1).
- One take per input aligned with the song, a shared **Room** for takes, a **Video studio** with any webcam, an **Exports** page, plug-and-play interfaces and cameras (0.5.0).
- Local stem separation (HT-Demucs 6-stem), a click locked to the song, loops, setlists, up to 8 inputs, MIDI and e-drum kits from ten brands. English, Português (Brasil) and Deutsch.
