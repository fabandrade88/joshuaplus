<p align="center">
  <img src="app/assets/icon.svg" width="96" alt="Joshua+ icon">
</p>

<h1 align="center">Joshua+ <sup><code>beta</code></sup></h1>
<p align="center"><b>Making music at home.</b> Split any song into instrument tracks, get a click that follows the song, and play along.</p>
<p align="center"><i>"Moises led the way. Joshua takes you home"</i></p>

<p align="center">
  <a href="https://github.com/fabandrade88/joshuaplus/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/fabandrade88/joshuaplus/actions/workflows/ci.yml/badge.svg?branch=test-branch"></a>
  <a href="https://github.com/fabandrade88/joshuaplus/releases"><img alt="Release" src="https://img.shields.io/github/v/release/fabandrade88/joshuaplus?include_prereleases&label=release"></a>
  <a href="LICENSE"><img alt="MIT" src="https://img.shields.io/badge/license-MIT-ffb000"></a>
  <img alt="Platforms" src="https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-333">
</p>

<p align="center">English · <a href="README.pt-BR.md">Português (Brasil)</a></p>

![Song view](docs/screenshots/song.png)

Joshua+ is a free, open-source (MIT) desktop app for musicians who practise at home. Drop a song in, and it:

- **separates it into instrument tracks** (vocals, drums, bass, guitar, piano, other) on your own computer,
- **finds the tempo and the bars** and builds a **metronome track that is locked to the song**,
- gives you a **mixer** with volume, mute, solo and pan per track,
- lets you **loop bars**, count yourself in, and keep a **playlist** per folder that remembers every setting.

Everything runs locally. Your music never leaves your computer unless you choose the optional cloud separator.

The app speaks **English, Português (Brasil) and Deutsch**.

> **Beta 0.0.1.** This is the first public build. Expect rough edges, and please [report problems](https://github.com/fabandrade88/joshuaplus/issues).

---

## Install

### macOS (Apple Silicon or Intel, macOS 13.4 or newer)

```sh
curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh
```

### Linux (x86_64: Ubuntu 24.04+, Debian 13+, Fedora 40+, Arch, … — needs glibc 2.39+)

```sh
curl -fsSL https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.sh | sh
```

On Debian and Ubuntu this installs the `.deb` package with `apt`. On other distributions it installs the AppImage to `~/.local/bin/joshua-plus`.

### Windows 10/11 (x64), in PowerShell

```powershell
irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex
```

The installer is per-user and does not need admin rights.

### What the install scripts do

Every installer:

1. Looks up the newest release on GitHub. To pick a version, set `JOSHUA_VERSION=v0.0.1-beta`.
2. Downloads the package for your system.
3. **Checks its SHA-256 against the release's `SHA256SUMS` and refuses to install if it doesn't match.**
4. Installs it.

Read [`scripts/install.sh`](scripts/install.sh) and [`scripts/install.ps1`](scripts/install.ps1) before you run them. They are short.

You can also download the packages by hand from [Releases](https://github.com/fabandrade88/joshuaplus/releases):

| System | Packages |
|---|---|
| macOS | `.dmg` |
| Windows | `-setup.exe` or `.msi` |
| Linux | `.deb`, `.rpm` or `.AppImage` |

> The beta builds are **not code-signed yet**. On macOS the install script removes the quarantine flag after it verifies the checksum. If you install by hand instead, right-click the app and choose **Open** the first time. On Windows, SmartScreen may ask you to confirm with **More info → Run anyway**.

---

## How to use it

### 1. Home: your folders

![Home](docs/screenshots/home.png)

Create folders for your band, your lessons or a gig. Drag audio files onto a folder or onto the window to import them. Joshua+ reads MP3, WAV, FLAC, OGG/Vorbis, M4A/AAC and AIFF.

Each folder also has an **inbox** on disk. Any file you drop into it from Finder or Explorer is imported automatically.

### 2. A folder is a playlist

![Folder](docs/screenshots/folder.png)

Songs in a folder play one after the other, like a setlist. Drag songs to reorder them. Every song keeps its own settings: mutes, volumes, tempo corrections, loop and click sound. Album art and tags (artist, album, year, …) come from the file. Only the cover is shown; click it to see the details. You can rename any song.

### 3. The song: tracks, bar map and click

![Song](docs/screenshots/song.png)

- **Tracks.** Each separated instrument has its own fader, mute (M), solo (S) and pan.
- **Collapsing the song.** You can fold all the song's tracks into a single "song" track. That keeps room for the recording tracks that are coming in a future version.

  ![Collapsed](docs/screenshots/song-collapsed.png)

- **Bar map.** A compact map of the song's bars, with the downbeats marked. It scrolls with the music.
  - Click a bar to jump there.
  - **Double-click a bar to loop it** until you turn the loop off.
  - Choose how many bars to repeat (1, 2, 4, 8…) to loop a whole phrase.
- **Metronome.** The click is placed on the song's own beats, not on an ideal grid that drifts away from the recording.
  - **Count-in:** choose 0–4 bars of count-in before playback. The count-in lands exactly on the song's first downbeat.
  - **Click sounds:** six voices: classic, wood block, beep, cowbell, sticks and drums (kick on the one, hi-hat on the others).
  - **Tempo modes:**
    - **Constant** (the default for most music) fits one perfectly steady tempo.
    - **Variable** follows a live band that speeds up and slows down.
    - **Auto** lets Joshua+ decide.
  - **Fixing the tempo:**
    - If the analysis picked double or half the tempo, fix it with **×2 / ÷2**.
    - Nudge the grid, set the "1" of the bar or change the time signature.
    - **Tap** the tempo, or **type the exact BPM**.
- **Playback continuity.** Playback continues while you browse other screens; the mini-player shows what is playing. Clicking the song again returns to it without restarting.

### 4. Audio inputs, MIDI and electronic drums

![Inputs](docs/screenshots/gear.png)

Joshua+ is getting ready for recording. Today you can already:

- **Audio inputs.** Pick **up to 8 audio inputs at once**: several audio interfaces, a USB mic, a guitar interface… Each has a live level meter and an optional monitor so you hear yourself over the song.
- **MIDI and e-drums.** Connect **e-drums and MIDI controllers** over USB-MIDI or a MIDI interface. There are presets for **Alesis** (Strata Prime, Strata Club, Strata Core, Strike Pro SE, Crimson II, Command, Surge, Turbo, Nitro, Debut), **Roland** V-Drums, **Yamaha** DTX, **EFNOTE**, **Simmons**, **Pearl** e/MERGE, **GEWA**, **ATV**, **Millenium** and General MIDI kits. Use **MIDI learn** to map any pad.

### 5. Built-in help and About

The **How to use** page explains every function in your language. **About** shows the version, the repository, the contributors and the open-source components.

| How to use | About |
|---|---|
| ![Help](docs/screenshots/help.png) | ![About](docs/screenshots/about.png) |

---

## Separation quality: three engines

| Engine | Where it runs | What it gives you | Cost |
|---|---|---|---|
| **Studio (HT-Demucs 6-stem)** — default | Your computer (CPU; GPU via DirectML on Windows, CUDA optional) | Vocals, drums, bass, guitar, piano, other. State-of-the-art quality, low bleed. About 5× real time on an Apple M-series CPU | Free. A one-time ~136 MB model download, SHA-256 verified |
| **Quick (built-in DSP)** | Your computer, no download | Harmonic/percussive and frequency-band separation. Instant, good for drums vs. the rest | Free |
| **Cloud (LALAL.AI)** — optional | LALAL.AI servers, with your own key | Extra stems, such as **acoustic vs. electric guitar** | Paid by you to LALAL.AI. Joshua+ never sees the money or your files |

The local engines are the norm; the cloud is the exception. You choose the engine per song in **Settings**.

---

## How it works (architecture)

```
┌──────────────────────────── app/ (Tauri 2 + Svelte 5) ────────────────────────────┐
│  UI: Home · Folder/playlist · Song (console, bar map, metronome) · Inputs · Help    │
│  TypeScript store ── IPC (typed commands, strict CSP, no fs/shell in the webview) ──┤
│  src-tauri: commands, folder inbox watcher, player, settings, device manager        │
└───────────────┬───────────────────┬───────────────────┬───────────────────┬────────┘
                │                   │                   │                   │
        crates/joshua-core   crates/joshua-engine  crates/joshua-ai    crates/joshua-midi
        decode · resample    cpal audio output     HT-Demucs (ONNX)    midir input
        tempo & beat grid    + up to 8 inputs      model download      note parsing
        metronome · mixer    lock-free, no alloc   (pinned + SHA-256)  drum-kit maps
        DSP separation       in the callback       LALAL.AI client     (10+ brands)
        workspace · security
```

### Import

The file is sniffed by its signature, not its extension. It is decoded with **Symphonia** under hard limits on size, duration and channels, then resampled with **rubato** if needed. Tags and cover art are read and the cover is re-encoded, so a malicious image never reaches the UI as-is.

### Separation

**HT-Demucs** runs through **ONNX Runtime** (`ort`) in overlapping segments. The six stems are written as 24-bit WAV files (`hound`) next to the song.

### Tempo

The song's tempo map is built in five steps:

1. Compute a spectral-flux onset curve on log-spaced bands.
2. Estimate the tempo by autocorrelation, with octave-error checks.
3. Track the beats with dynamic programming (Ellis).
4. Fit a constant grid, or detect a real tempo change. A change has to persist over several windows, and metric ratios such as half-time or triplet feel are not counted as changes.
5. Find the downbeat.

Your corrections are stored as edits, so a re-analysis never throws them away.

### Playback

The audio callback mixes up to 12 tracks plus the click and the input monitors. It has no locks and no allocations; the test suite checks this with `assert_no_alloc`. The UI talks to it through atomics and a wait-free ring buffer (`rtrb`).

### Storage

Your workspace is a plain folder:

- one directory per song (`project.json`, stems, cover);
- one JSON file per folder.

Writes are atomic: a temp file, then a rename. Symlinks are refused.

---

## Quality gate: tests, attacks, performance

Nothing reaches `main` without passing CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) on **Linux, macOS and Windows**. Work is pushed to `test-branch`, and `main` is protected and only accepts pull requests whose checks are green.

| Check | What it covers |
|---|---|
| `cargo fmt`, `clippy -D warnings` | Style and lints; `unsafe` is forbidden in every crate |
| Unit tests (Rust + Vitest) | DSP, tempo, mixer, metronome, workspace, MIDI, UI logic, help catalogues |
| **Property tests** (`proptest`) | Random hostile sequences against the mixer, tempo edits, count-in and JSON parsing. The output must never be NaN, clip or panic |
| **Fuzzing** | Thousands of mutated MP3/FLAC/OGG/M4A/AIFF/WAV files and cover images: no panic and no hang |
| **Attack suites** | The attacks the app defends against:<br>• path traversal and `../` names, absolute paths, Windows reserved names<br>• symlink and FIFO tricks, symlinked temp files and workspace directories<br>• zip-bomb-style huge headers and decoded-size caps, absurd beat grids<br>• MIDI floods (1 M messages), CSP and permission pinning, no `innerHTML`/`eval` in the UI |
| **Real-time safety** | The audio callback must not allocate |
| **Performance floors** (release) | Mixer, tempo, DSP separation, decode, resample and peaks must stay above minimum × real-time speeds |
| **Advisories** | `cargo audit` (RustSec) and `pnpm audit` |

Speeds measured on an Apple M-series laptop, as multiples of real time:

| Stage | Speed |
|---|---|
| Mixer (7 stems + click) | 1152× |
| Tempo detection | 2236× |
| Quick DSP separation | 29.5× |
| WAV decode | 4549× |
| Resample | 713× |
| HT-Demucs on the CPU | 5.3× |

See [`SECURITY.md`](SECURITY.md) for the threat model and the audit status.

---

## Build from source

You need:

- **Rust** stable (1.90+), from https://rustup.rs
- **Node.js** 20+ and **pnpm** 10
- **Linux only:**
  ```sh
  sudo apt install libasound2-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev
  ```
- **Windows only:** WebView2 (already on Windows 11) and the MSVC build tools

Then:

```sh
git clone https://github.com/fabandrade88/joshuaplus.git
cd joshuaplus/app
pnpm install
pnpm tauri dev          # run in development
pnpm tauri build        # build installers for your platform (target/release/bundle)
```

Run the checks yourself:

```sh
cargo test --workspace                                                           # every Rust test
cargo test --release -p joshua-core --test performance -- --ignored --nocapture  # performance floors
cd app && pnpm check && pnpm test                                                # UI types + tests
pnpm preview:ui         # the UI in a browser with a mocked backend (http://localhost:1421)
```

---

## Open source used by Joshua+

Joshua+ exists thanks to these projects.

| Component | Used for | License |
|---|---|---|
| [Tauri 2](https://tauri.app) | Desktop shell, IPC, installers | MIT / Apache-2.0 |
| [Svelte 5](https://svelte.dev) + [Vite](https://vite.dev) | User interface and build | MIT |
| [Symphonia](https://github.com/pdeljanov/Symphonia) | Decoding MP3, FLAC, OGG, AAC/M4A, WAV, AIFF and reading tags | MPL-2.0 |
| [rubato](https://github.com/HEnquist/rubato) | High-quality resampling | MIT / Apache-2.0 |
| [RustFFT](https://github.com/ejmahler/RustFFT) / [RealFFT](https://github.com/HEnquist/realfft) | FFTs for tempo analysis and DSP separation | MIT / Apache-2.0 |
| [cpal](https://github.com/RustAudio/cpal) | Audio output and inputs (CoreAudio, WASAPI, ALSA/PulseAudio/JACK) | Apache-2.0 |
| [rtrb](https://github.com/mgeier/rtrb) | Wait-free ring buffer to the audio thread | MIT / Apache-2.0 |
| [midir](https://github.com/Boddlnagg/midir) | MIDI input (e-drums, controllers) | MIT |
| [HT-Demucs](https://github.com/facebookresearch/demucs) (Meta AI Research) | Neural stem separation model | MIT |
| [htdemucs-6s ONNX export](https://huggingface.co/adowu/htdemucs-6s-onnx) | ONNX version of the 6-stem model | MIT |
| [ONNX Runtime](https://onnxruntime.ai) via [`ort`](https://github.com/pykeio/ort) | Running the model on CPU, DirectML or CUDA | MIT / Apache-2.0 |
| [hound](https://github.com/ruuda/hound) | Writing WAV stems | Apache-2.0 |
| [image](https://github.com/image-rs/image) | Decoding and re-encoding cover art safely | MIT / Apache-2.0 |
| [ureq](https://github.com/algesten/ureq) + [rustls](https://github.com/rustls/rustls) | HTTPS for the model download and the optional cloud | MIT / Apache-2.0 / ISC |
| [RustCrypto sha2](https://github.com/RustCrypto/hashes) | Verifying downloads | MIT / Apache-2.0 |
| [serde](https://serde.rs) / serde_json | Project and settings files | MIT / Apache-2.0 |
| [thiserror](https://github.com/dtolnay/thiserror) | Error types | MIT / Apache-2.0 |
| [proptest](https://github.com/proptest-rs/proptest), [assert_no_alloc](https://github.com/Windfisch/rust-assert-no-alloc), [Vitest](https://vitest.dev), [tempfile](https://github.com/Stebalien/tempfile) | Testing | MIT / Apache-2.0 / BSD-1-Clause |

The complete, generated list of all ~600 crates and packages, with versions and licenses, is in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

**LALAL.AI** is a commercial service. Joshua+ links to it only as an optional extra, and you need your own account.

---

## Roadmap

- Multitrack recording from the audio inputs and MIDI, alongside the song.
- Speed and pitch change.
- Signed and notarised installers.

## Contributing

Contributions are very welcome: bug reports, translations, tests, DSP ideas and code.

- **[CONTRIBUTING.md](CONTRIBUTING.md)** covers the branch flow (`feature/*` → `test-branch` → `main`), coding rules and best practices, testing requirements, commit style and the review checklist.
- **[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)**: be kind and respectful.
- **[SECURITY.md](SECURITY.md)**: report vulnerabilities privately, never in a public issue.
- Use the **issue templates** for bugs and feature requests. The **pull-request template** has the checklist CI enforces.

The short version, before you push:

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd app && pnpm check && pnpm test
```

## License

[MIT](LICENSE) © 2026 **Fabricio O. de Andrade**
