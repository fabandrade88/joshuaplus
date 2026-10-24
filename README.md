<p align="center">
  <img src="app/assets/icon.svg" width="96" alt="Joshua+ icon">
</p>

<h1 align="center">Joshua+</h1>
<p align="center"><b>Making music at home.</b> Split any song into instruments, play along with a click locked to the song, record yourself and share the video.</p>
<p align="center"><i>"Moises led the way. Joshua takes you home"</i></p>

<p align="center">
  <a href="https://github.com/fabandrade88/joshuaplus/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/fabandrade88/joshuaplus?include_prereleases&label=release"></a>
  <a href="https://github.com/fabandrade88/joshuaplus/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/fabandrade88/joshuaplus/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="LICENSE"><img alt="MIT" src="https://img.shields.io/badge/license-MIT-ffb000"></a>
  <img alt="Platforms" src="https://img.shields.io/badge/Windows%20%7C%20macOS%20%7C%20Linux-333">
</p>

<p align="center">English · <a href="README.pt-BR.md">Português (Brasil)</a></p>

![Joshua+](docs/screenshots/song.png)

Free and open source (MIT). Runs on your computer: your music never leaves it. In English, Português (Brasil) and Deutsch.

## Install

### macOS

Apple Silicon (M1 or newer), macOS 13.4 or newer. Paste in Terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh
```

### Windows

Windows 10 or 11, 64-bit, no admin rights needed. Paste in PowerShell:

```powershell
irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex
```

### Linux

x86_64 with glibc 2.39+ (Ubuntu 24.04+, Debian 13+, Fedora 40+, Arch). Paste in a terminal:

```sh
curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh
```

The scripts download the newest release, **check its SHA-256 against the release's `SHA256SUMS`** and only then install it: the `.deb` with apt on Debian/Ubuntu, the AppImage in `~/.local/bin` on other Linux, the app in Applications on macOS. They are short; read [`install.sh`](scripts/install.sh) and [`install.ps1`](scripts/install.ps1) first if you like. Set `JOSHUA_VERSION=v0.5.0` to pick a version.

**Prefer to download by hand?** Get the `.dmg` (macOS), `-setup.exe` or `.msi` (Windows), or `.deb`, `.rpm` or `.AppImage` (Linux) from [Releases](https://github.com/fabandrade88/joshuaplus/releases/latest). The builds are not code-signed yet: on macOS right-click the app and choose **Open** the first time; on Windows choose **More info → Run anyway**.

Every installer is installed and started on clean Windows, macOS and Linux machines by the release pipeline before a release is published.

## What you get

| | Feature | Why it helps |
|---|---|---|
| 🎚️ | **Instrument separation** into vocals, drums, bass, guitar, piano and other, on your own computer (HT-Demucs) | Mute the part you play and play it yourself, or solo it to learn it |
| 🥁 | **A click locked to the song**: tempo and bars detected, constant or variable tempo, count-in, six click sounds, tap or type the BPM | Practise in time with the real recording, not a grid that drifts away |
| 🔁 | **Bar map and loops**: click a bar to jump, double-click to loop it, loop 1–8 bars | Drill the hard part until it sits |
| 📁 | **Folders as setlists** that remember every song's mix, tempo fixes and loop | Open a song and continue exactly where you left it |
| 🎤 | **Record every input as its own take**, lined up with the song automatically (latency and clock drift corrected). Recording again replaces the previous take | Hear how you really played, against the band |
| 🏠 | **Room**: a shared space for your takes | A dry drum module or DI guitar sits in the mix instead of on top of it |
| 🎬 | **Video studio**: film yourself with any webcam, adjust the sound before exporting, and export **YouTube 16:9, Shorts/Reels/TikTok 9:16 and Instagram 1:1 / 4:5** at once with optional cover, title and artist. Never the click | Post a play-along without a video editor |
| 💾 | **Export the mix** as a 24-bit WAV, with or without the click. Everything you export is listed under **Exports** | Send it to your band or teacher |
| 🎛️ | **Up to 8 audio inputs** with meters and monitoring, **MIDI and e-drums** (Alesis, Roland, Yamaha, EFNOTE, Simmons, Pearl, GEWA, ATV, Millenium, General MIDI) | Use the gear you already have |
| 🔌 | **Plug and play**: unplug an interface or a camera and Joshua+ moves to what is connected, and back when it returns | No restart in the middle of practice |

| Home and folders | Video studio | Exports |
|---|---|---|
| ![Home](docs/screenshots/home.png) | ![Video review](docs/screenshots/video-review.png) | ![Exports](docs/screenshots/exports.png) |

## Quick start

1. **Choose where to keep your library** on first launch (*Music › Joshua+* is suggested; you can move it later in Settings).
2. **Drop a song** (MP3, WAV, FLAC, OGG, M4A/AAC, AIFF) onto the window or a folder. It is separated in the background.
3. **Open it**: mute your instrument, turn the click on, press play.
4. **Record**: arm an input with ● and press the red REC button, or switch REC to the camera to film yourself.
5. **Share**: *Export audio* for a WAV, or export the video formats you need. Find the files under **Exports** in the sidebar.

The built-in **How to use** page explains every control in your language.

## Separation engines

| Engine | Runs on | Gives you | Cost |
|---|---|---|---|
| **Studio (HT-Demucs 6-stem)**, default | Your computer (CPU; DirectML on Windows, CUDA optional) | Six clean stems, about 5× real time on Apple M-series | Free. One-time ~136 MB model download, SHA-256 verified |
| **Quick (DSP)** | Your computer, nothing to download | Instant drums-vs-rest and band separation | Free |
| **Cloud (LALAL.AI)**, optional | LALAL.AI, with your own key | Extra stems such as acoustic vs. electric guitar | Paid by you to LALAL.AI |

## Quality and security

Nothing reaches `main` without green CI on Linux, macOS and Windows ([ci.yml](.github/workflows/ci.yml)): `rustfmt`, `clippy -D warnings`, unit and property tests, fuzzing of hostile audio and images, attack suites (path traversal, symlinks, decode bombs, MIDI floods, CSP pinning), a real-time check that the audio callback never allocates, performance floors, and `cargo audit` / `pnpm audit`. `unsafe` code is forbidden. Threat model: [SECURITY.md](SECURITY.md).

Measured on an Apple M-series laptop (multiples of real time): mixer 1426×, mixer with 4 takes in the room 556×, tempo detection 3497×, quick separation 46×, WAV decode 6441×, resampling 1220×.

## Build from source

Needs Rust stable, Node.js 20+ and pnpm 10. On Linux also `libasound2-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev`.

```sh
git clone https://github.com/fabandrade88/joshuaplus.git
cd joshuaplus/app && pnpm install
pnpm tauri dev      # run
pnpm tauri build    # installers in target/release/bundle
cargo test --workspace && pnpm check && pnpm test   # checks
```

Architecture: a Tauri 2 + Svelte 5 app (`app/`) over four Rust crates: `joshua-core` (decoding, tempo, metronome, mixer, room, recording alignment, export, workspace security), `joshua-engine` (lock-free audio I/O with cpal), `joshua-ai` (HT-Demucs on ONNX Runtime, LALAL.AI client) and `joshua-midi` (MIDI and drum-kit maps).

## Open source used by Joshua+

| Component | Used for | License |
|---|---|---|
| [Tauri 2](https://tauri.app) | Desktop shell, IPC, installers | MIT / Apache-2.0 |
| [Svelte 5](https://svelte.dev) + [Vite](https://vite.dev) | User interface and build | MIT |
| [Symphonia](https://github.com/pdeljanov/Symphonia) | Audio decoding and tags | MPL-2.0 |
| [rubato](https://github.com/HEnquist/rubato) | Resampling | MIT / Apache-2.0 |
| [RustFFT](https://github.com/ejmahler/RustFFT) / [RealFFT](https://github.com/HEnquist/realfft) | Tempo analysis and DSP | MIT / Apache-2.0 |
| [cpal](https://github.com/RustAudio/cpal) | Audio output and inputs | Apache-2.0 |
| [rtrb](https://github.com/mgeier/rtrb) | Wait-free ring buffers to the audio thread | MIT / Apache-2.0 |
| [midir](https://github.com/Boddlnagg/midir) | MIDI input | MIT |
| [HT-Demucs](https://github.com/facebookresearch/demucs) (Meta AI) and its [6-stem ONNX export](https://huggingface.co/adowu/htdemucs-6s-onnx) | Separation model | MIT |
| [ONNX Runtime](https://onnxruntime.ai) via [`ort`](https://github.com/pykeio/ort) | Running the model | MIT / Apache-2.0 |
| [hound](https://github.com/ruuda/hound), [image](https://github.com/image-rs/image) | WAV files, safe cover art | Apache-2.0, MIT / Apache-2.0 |
| [ureq](https://github.com/algesten/ureq) + [rustls](https://github.com/rustls/rustls), [sha2](https://github.com/RustCrypto/hashes) | HTTPS and verified downloads | MIT / Apache-2.0 / ISC |
| [serde](https://serde.rs), [thiserror](https://github.com/dtolnay/thiserror) | Files and errors | MIT / Apache-2.0 |
| [proptest](https://github.com/proptest-rs/proptest), [assert_no_alloc](https://github.com/Windfisch/rust-assert-no-alloc), [Vitest](https://vitest.dev) | Testing | MIT / Apache-2.0 / BSD |

Every one of the ~600 dependencies, with version and license: [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md). LALAL.AI is a commercial service used only if you add your own key.

## Roadmap

- Instrument plugins (EZdrummer, Superior Drummer…) inside Joshua+ ([#1](https://github.com/fabandrade88/joshuaplus/issues/1))
- Speed and pitch change
- Signed and notarised installers, Intel Mac support

## Contributing

Bug reports, translations, tests and code are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) (branch flow `feature/*` → `test-branch` → `main`, coding rules, tests, review checklist), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), and report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © 2026 **Fabricio O. de Andrade**
