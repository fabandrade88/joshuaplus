**Joshua+ 0.5.5 — making music at home.** Split songs into instruments, play along with a click locked to the song, record yourself and share the video.

### Install

- **macOS / Linux:** `curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh`
- **Windows (PowerShell):** `irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex`

The scripts verify every download against `SHA256SUMS`. Platforms: macOS on Apple Silicon (13.4+), Windows 10/11 x64, Linux x86_64 (glibc 2.39+). Installation tested on Ubuntu 26.04 LTS, Omarchy (Arch Linux), Windows 11 and a MacBook Pro M4 Pro; every installer below was also installed and started on clean Windows, macOS and Linux machines before this release was published. Builds are not code-signed yet (see the README). Joshua+ 0.5.1 and later tell you about this release by themselves.

### New in 0.5.5

- **Chords to play along.** Switch on **Chords** and every bar of the bar map shows its chord, the counter shows the chord now and the next one, and a large window shows the current chord with the next three and a beat countdown. Chords are recognised on your computer from the song's instruments (major, minor and power chords, spelled for the song's key); no internet, nothing uploaded. Treat them as a guide: very dense or distorted parts can be off.
- **Lyrics to sing along.** Switch on **Lyrics** and the line being sung and the next ones show in time with the song, from [LRCLIB](https://lrclib.net), the open lyrics database. Only the title, artist, album and length are sent. Lyrics are kept with the song and work offline; they can be moved earlier or later, edited, or pasted (plain text or LRC).
- **Ready before you need them.** "Download lyrics automatically" (Settings; switched on the first time you use Lyrics) fetches the lyrics of your whole library in the background and tries again when you are back online.
- **A window you can read from the drum stool.** Chords and lyrics open in a floating window: drag it anywhere, resize it and the text grows with it; its place is remembered.

### Since 0.0.1-beta

- Exports mastered to streaming loudness with a look-ahead limiter, Tone and Body on takes, balancing that never undoes your mix, 320 kbit/s video sound at 1080p (0.5.4).
- Count-in before the intro and a click through it, tempo in the right pulse, tighter beats, typed BPM kept, Settings as a page (0.5.3).
- Frame-by-frame video export, social formats cropped not squeezed, Balance with the original and Reset levels, a flat monospace design (0.5.2).
- New-version notice, Linux AppImage in the application menu, install scripts without the GitHub API (0.5.1).
- One take per input aligned with the song, a shared **Room** for takes, a **Video studio** with any webcam, an **Exports** page, plug-and-play interfaces and cameras (0.5.0).
- Local stem separation (HT-Demucs 6-stem), a click locked to the song, loops, setlists, up to 8 inputs, MIDI and e-drum kits from ten brands. English, Português (Brasil) and Deutsch.
