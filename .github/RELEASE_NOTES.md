**Joshua+ 0.5.2 — making music at home.** Split songs into instruments, play along with a click locked to the song, record yourself and share the video.

### Install

- **macOS / Linux:** `curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh`
- **Windows (PowerShell):** `irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex`

The scripts verify every download against `SHA256SUMS`. Platforms: macOS on Apple Silicon (13.4+), Windows 10/11 x64, Linux x86_64 (glibc 2.39+). Installation tested on Ubuntu 26.04 LTS, Omarchy (Arch Linux), Windows 11 and a MacBook Pro M4 Pro; every installer below was also installed and started on clean Windows, macOS and Linux machines before this release was published. Builds are not code-signed yet (see the README). Joshua+ 0.5.1 tells you about this release by itself.

### New in 0.5.2

- **Videos without stutter, in better quality.** Clips are now encoded frame by frame instead of being recorded while they play: no more frozen picture or dropped frames, a steady 30 fps, H.264 High at about 16 Mbit/s for 1080p (was Constrained Baseline at a variable rate), and the camera is recorded at 24 Mbit/s. The sound is in exact sync in QuickTime/Safari and in FFmpeg-based players and platforms (YouTube, Instagram, VLC).
- **Social formats are cropped, not squeezed.** Shorts/Reels/TikTok 9:16, Instagram 1:1 and 4:5 show the centre of the camera picture again.
- **Smaller cover, title and artist** in the video corner; long names shrink to fit instead of being compressed.
- **Balance with the original:** one click (above the takes, or in the video studio's sound panel) matches each take to the instrument it plays, sets it to that instrument's level and place in the stereo field, and mutes the original. It can run by itself after every recording (Settings, off by default).
- **Reset levels** puts the song, its tracks, the takes and the master back to their original values.
- **A plainer look:** flat, monospace, square-cornered design in the spirit of tiling desktops such as Omarchy. Same screens and same icon.

### Since 0.0.1-beta

- New-version notice, Linux AppImage in the application menu, install scripts without the GitHub API (0.5.1).
- One take per input aligned with the song, recording again replaces the previous take, a shared **Room** for takes, a **Video studio** with any webcam, an **Exports** page, plug-and-play interfaces and cameras (0.5.0).
- Local stem separation (HT-Demucs 6-stem), a click locked to the song, loops, setlists, up to 8 inputs, MIDI and e-drum kits from ten brands. English, Português (Brasil) and Deutsch.
