**Joshua+ 0.5.1 — making music at home.** Split songs into instruments, play along with a click locked to the song, record yourself and share the video.

### Install

- **macOS / Linux:** `curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh`
- **Windows (PowerShell):** `irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex`

The scripts verify every download against `SHA256SUMS`. Platforms: macOS on Apple Silicon (13.4+), Windows 10/11 x64, Linux x86_64 (glibc 2.39+). Installation tested on Ubuntu 26.04 LTS, Omarchy (Arch Linux), Windows 11 and a MacBook Pro M4 Pro; every installer below was also installed and started on clean Windows, macOS and Linux machines before this release was published. Builds are not code-signed yet (see the README).

### New in 0.5.1

- **New-version notice:** Joshua+ now tells you when a newer release is out, with a link to its notes and installers. It never downloads or installs anything by itself, and you can turn it off in Settings. (From this version on: 0.5.0 cannot tell you about this release.)
- **Linux AppImage in the application menu** ([#15](https://github.com/fabandrade88/joshuaplus/issues/15)): on first run the app offers to add itself to the menu, keeps the entry working if the AppImage is moved, and can remove it from Settings. The install script registers it automatically on Arch-based systems such as Omarchy. Icons now use valid 256 px and 512 px sizes.
- **Install scripts** find the release without the GitHub API, so shared networks no longer hit its rate limit.

### Since 0.0.1-beta (0.5.0)

- Recording one take per input, aligned with the song; recording again replaces the previous take; a shared **Room** for takes.
- **Video studio** with any webcam, sound editing before export, YouTube / Shorts / Reels / TikTok / Instagram formats in one pass, cover and title in the corner.
- **Exports** page, explicit "include the metronome" choice, plug-and-play audio interfaces and cameras, library and export folders of your choice.
- Local stem separation (HT-Demucs 6-stem), a click locked to the song, loops, setlists, up to 8 inputs, MIDI and e-drum kits from ten brands. English, Português (Brasil) and Deutsch.
