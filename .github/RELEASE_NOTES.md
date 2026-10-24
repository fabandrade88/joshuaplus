**Joshua+ 0.5.0 — making music at home.** Split songs into instruments, play along with a click locked to the song, record yourself and share the video.

### Install

- **macOS / Linux:** `curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh`
- **Windows (PowerShell):** `irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex`

The scripts verify every download against `SHA256SUMS`. Platforms: macOS on Apple Silicon (13.4+), Windows 10/11 x64, Linux x86_64 (glibc 2.39+). Every installer below was installed and started on clean Windows, macOS and Linux machines before this release was published. Builds are not code-signed yet (see the README).

### New since 0.0.1-beta

- **Recording:** one take per armed input, aligned with the song; recording again replaces the previous take of that input. A mic or guitar on one side of a stereo pair is centred.
- **Room:** a shared space for takes, so dry recordings sit in the mix (live, in audio exports and in videos).
- **Video studio:** any webcam (plugged-in cameras are found automatically), edit the sound before exporting, export YouTube 16:9, Shorts/Reels/TikTok 9:16 and Instagram 1:1 / 4:5 in one pass, with the cover, title and artist in the bottom-left corner (each optional). The studio can be closed while an export runs. A clip keeps the song it was filmed on, so its title, artist and cover always match; discarding a clip turns the camera back on.
- **Exports:** a sidebar page with every mix and video, and an explicit "include the metronome" choice when exporting audio.
- **Plug and play:** unplugging the audio interface moves playback to the connected one and back when it returns; no more permanent device error. Disconnected inputs leave the song screen and recording, and come back when reconnected.
- **Library folders:** choose where the library lives, move it later, separate export folder.
- Readability and layout fixes in English, Português (Brasil) and Deutsch.

### Everything else

- Local stem separation (HT-Demucs 6-stem), an instant DSP mode and an optional LALAL.AI cloud mode.
- Tempo and bar detection with a metronome locked to the song, count-in, six click sounds, loops and a bar map.
- Folders as setlists, up to 8 audio inputs, MIDI and e-drum kits from ten brands.
