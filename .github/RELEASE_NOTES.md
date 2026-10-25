**Joshua+ 0.5.4 — making music at home.** Split songs into instruments, play along with a click locked to the song, record yourself and share the video.

### Install

- **macOS / Linux:** `curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh`
- **Windows (PowerShell):** `irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex`

The scripts verify every download against `SHA256SUMS`. Platforms: macOS on Apple Silicon (13.4+), Windows 10/11 x64, Linux x86_64 (glibc 2.39+). Installation tested on Ubuntu 26.04 LTS, Omarchy (Arch Linux), Windows 11 and a MacBook Pro M4 Pro; every installer below was also installed and started on clean Windows, macOS and Linux machines before this release was published. Builds are not code-signed yet (see the README). Joshua+ 0.5.1 and later tell you about this release by themselves.

### New in 0.5.4

- **Exports that keep their punch.** A record plus your take ran over full scale, and the playback safety clipper flattened every kick and tom hit. Audio exports and the videos' sound are now mastered for sharing: measured as streaming platforms do (ITU-R BS.1770), brought to −14 LUFS (YouTube, Spotify, Instagram) and kept under −1 dBFS by a look-ahead limiter instead of clipping. Playing in the app is unchanged.
- **Tone:** a take can take on the sound of the instrument it replaces. Balance with the original measures both and sets a 10-band equaliser (on e-drum takes: the missing highs back, less boxy mids); the Tone knob sets how much. Older takes get their match the first time you turn it up.
- **Body:** up to +5 dB around 60–125 Hz for kick and toms that sound thin.
- **Balancing never undoes your mix.** With "balance after recording" on, every recording used to unmute stems and reset levels. Now only the new takes are balanced (their level follows the replaced instrument's fader) and nothing is ever unmuted.
- **Video:** the sound is AAC at a constant 320 kbit/s (it came out near 120); the camera is asked for exactly 1080p when it can, and the studio shows the resolution it really delivers, with a warning below 1080p.

### Since 0.0.1-beta

- Count-in before the intro and a click through it, tempo in the right pulse, tighter beats, typed BPM kept, Settings as a page (0.5.3).
- Frame-by-frame video export, social formats cropped not squeezed, Balance with the original and Reset levels, a flat monospace design (0.5.2).
- New-version notice, Linux AppImage in the application menu, install scripts without the GitHub API (0.5.1).
- One take per input aligned with the song, a shared **Room** for takes, a **Video studio** with any webcam, an **Exports** page, plug-and-play interfaces and cameras (0.5.0).
- Local stem separation (HT-Demucs 6-stem), a click locked to the song, loops, setlists, up to 8 inputs, MIDI and e-drum kits from ten brands. English, Português (Brasil) and Deutsch.
