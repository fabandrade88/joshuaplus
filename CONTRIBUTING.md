# Contributing to Joshua+

Thank you for helping make Joshua+ better. Every contribution helps, from a typo fix to a new DSP algorithm. This guide explains how to contribute and the standards the code is held to.

By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Report a bug.** Use the [bug template](https://github.com/fabandrade88/joshuaplus/issues/new?template=bug_report.yml) and include your OS, the app version and steps to reproduce. If the bug is about tempo or separation, say which song (artist and title) caused it. Never upload copyrighted audio.
- **Suggest a feature.** Use the [feature template](https://github.com/fabandrade88/joshuaplus/issues/new?template=feature_request.yml). Explain the musical problem first and the solution second.
- **Translate.** The UI strings live in `app/src/lib/i18n/` and the help pages in `app/src/lib/help/` (`en`, `pt-BR`, `de`). A test makes sure every language has the same keys.
- **Add your e-drum kit.** Kit brands and note maps are in `crates/joshua-midi/src/brands.rs` and `drums.rs`.
- **Code.** Look for issues labelled `good first issue` or `help wanted`, and comment on the issue before you start on something big.
- **Security problems** must **not** go in public issues. See [SECURITY.md](SECURITY.md).

## Branch flow

```
feature/my-change ──PR──▶ test-branch ──PR (CI green)──▶ main ──tag v*──▶ Release
```

1. Fork the repository, or create a branch if you have access. Name it `feature/…`, `fix/…`, `docs/…` or `i18n/…`.
2. Open your pull request against **`test-branch`**. CI runs on Linux, macOS and Windows.
3. The maintainer collects green changes on `test-branch` and opens a pull request into **`main`**. `main` is protected: it cannot be pushed to directly, and every CI job must pass first.
4. Releases are cut from `main` by pushing a tag such as `v0.0.2-beta`. The release workflow builds the installers and publishes `SHA256SUMS`.

Keep each pull request focused on **one** change. Small pull requests get reviewed fast.

## Development setup

The prerequisites are listed in the [README](README.md#build-from-source). Then:

```sh
cd app
pnpm install
pnpm tauri dev      # full app
pnpm preview:ui     # UI only, mocked backend, http://localhost:1421, great for design work
```

## Project layout

| Path | What lives there | Rules |
|---|---|---|
| `crates/joshua-core` | Decoding, resampling, tempo, metronome, mixer, DSP separation, workspace, security helpers | Pure Rust, no I/O to devices, no UI |
| `crates/joshua-engine` | Real-time audio output and inputs (cpal) | Real-time rules below |
| `crates/joshua-ai` | HT-Demucs (ONNX), model download, cloud client | Every download is pinned and hash-checked |
| `crates/joshua-midi` | MIDI input, message parsing, e-drum maps | Must survive any byte stream |
| `app/src-tauri` | Tauri commands, player, settings, devices, inbox | Thin glue; logic belongs in the crates |
| `app/src` | Svelte 5 UI, store, i18n, help | No direct file system or shell access |

## Coding rules and best practices

### General

- **Readable over clever.** Clear names, short functions, comments that explain *why* rather than *what*. Match the style of the code around you.
- **No dead code, no commented-out code, no `TODO` without an issue link.**
- **Errors are values.** Use `Result` and `thiserror` types. Show the user a clear, translated message, never a raw panic.
- Add a dependency only when it is clearly worth it. It must be MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0 or a compatible license. **No GPL/AGPL** code, because the app ships under MIT. Mention the new dependency in the pull request.

### Rust

- **Edition 2024. `unsafe` is forbidden** (`unsafe_code = "forbid"` in every crate).
- **`cargo fmt`** formatting (see `rustfmt.toml`). **`cargo clippy -- -D warnings`** must be clean.
- No `unwrap()`, `expect()` or `panic!` in non-test code. Clippy warns about all three.
- Use saturating or checked arithmetic on anything derived from user files: sample counts, durations, sizes.
- **Untrusted input** includes every audio file, tag, cover image, JSON file on disk, MIDI byte and network response:
  - Open files only through `security::open_regular` or `open_audio_file`. They refuse symlinks, FIFOs and devices.
  - Write only through `security::atomic_write` or `create_exclusive`.
  - Resolve paths with `resolve_within` / `ensure_inside`. Never build paths by joining raw user strings.
  - Respect `security::Limits` (size, duration, channels).

### Real-time audio (`joshua-engine`, `Mixer::render`)

The audio callback must never:

- **allocate**,
- **lock**,
- **do I/O**,
- **log**,
- **wait**.

Use atomics (`SharedState`) and the `rtrb` ring buffer. Send anything that must be freed back to the control thread (the `Garbage` path). `tests/realtime_safety.rs` checks this with `assert_no_alloc`, so add your new code path to that test.

### Frontend (Svelte 5 + TypeScript)

- Use runes (`$state`, `$derived`, `$effect`). Shared state lives in `app/src/lib/app.svelte.ts`, and components stay presentational.
- **Never** use `{@html}`, `innerHTML`, `eval`, `new Function` or inline event strings. `src/security.test.ts` fails the build if you do.
- Do not loosen the CSP or the Tauri capabilities (`app/src-tauri/capabilities/default.json`). Those are pinned by tests too. If you really need to, explain why in the pull request.
- **Every user-facing string goes through i18n, in all three languages.** If you can't write German or Portuguese, add the English text and say so in the pull request; someone will translate it.
- `pnpm check` must report 0 errors and 0 warnings.
- **Design.** Joshua+ has its own visual identity, built from the warm yellow and amber accents on dark surfaces. Keep it original, and do not copy the look of other apps.
  - Use the existing CSS variables; don't hard-code colours.
  - Test at the minimum window size of 960 × 600.

## Tests

**Every change needs tests.** Bug fixes need a test that fails before the fix.

| Kind | Where | Run |
|---|---|---|
| Unit tests | `#[cfg(test)]` modules, `app/src/**/*.test.ts` | `cargo test --workspace`, `pnpm test` |
| Property tests | `crates/joshua-core/tests/properties.rs` | included in `cargo test` |
| Fuzzing | `crates/joshua-core/tests/fuzz_media.rs`, `crates/joshua-midi/tests/hostile_input.rs` | included in `cargo test` |
| Attack tests | `crates/joshua-core/tests/security_attacks.rs`, `review_attacks.rs` | included in `cargo test` |
| Real-time safety | `crates/joshua-engine/tests/realtime_safety.rs` | included in `cargo test` |
| Performance floors | `crates/joshua-core/tests/performance.rs` | `cargo test --release -p joshua-core --test performance -- --ignored --nocapture` |

Guidelines:

- Write tests for *behaviour*, not implementation details.
- New parsers or file formats need a fixture in `tests/fixtures/`, and must go through the fuzz test.
- Changes to the tempo detector must not make any existing tempo test worse. If you can, add the case that motivated the change as a synthetic signal.
- Don't lower a performance floor to make CI pass. Find the regression instead.
- Test fixtures must be generated or free of copyright. **Never commit commercial music.**

## Commits

- Write the message in English, in the imperative mood, in the form `area: what changed`. For example:
  - `tempo: ignore half-time sections when fitting the grid`
  - `ui: compact bar map on small windows`
- Make one logical change per commit. Explain *why* in the body when it isn't obvious.
- Reference issues with `Fixes #123`.

## Pull-request checklist

The template asks for this, and CI enforces most of it:

- [ ] Targets `test-branch` and does one thing
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass
- [ ] `pnpm check` and `pnpm test` pass
- [ ] Tests added or updated (with a regression test for bug fixes)
- [ ] Strings translated (EN / pt-BR / DE) or marked for translation
- [ ] UI changes have a before/after screenshot
- [ ] No new `unsafe`, `unwrap` in app code, `{@html}`, or relaxed CSP or capabilities
- [ ] New dependencies are justified and have compatible licenses

## Review

The maintainer reviews every pull request. Expect questions, which are about the code and never about you. Once the pull request is approved and CI is green on `test-branch`, it goes into the next pull request to `main`.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE) of this project.
