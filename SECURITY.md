# Security Policy

## Supported versions

Joshua+ is in **beta**. Security fixes go into the newest release only.

| Version | Supported |
|---|---|
| latest `0.0.x` beta | ✅ |
| anything older | ❌ |

## Reporting a vulnerability

**Please do not open a public issue.** Report it privately through GitHub's
[**Report a vulnerability**](https://github.com/fabandrade88/joshuaplus/security/advisories/new) form (Security tab → Advisories).

Include:

- what an attacker can do (for example, read files outside the workspace, or crash or hang the app with a crafted file);
- a proof of concept, such as a crafted audio, cover or `project.json` file, or a MIDI byte sequence;
- the version and operating system.

You will get a first reply within **7 days**. Once a fix is released, the advisory is published with credit to you, unless you prefer to stay anonymous.

## Threat model

Joshua+ is an offline desktop app. The things it treats as **hostile** are:

| Input | Defence |
|---|---|
| Audio files (MP3, FLAC, OGG, M4A, WAV, AIFF) | The type is checked by file signature, not extension. Decoding is capped by size, duration and channel limits, and the total decoded frames are capped. Only regular files are opened (`O_NOFOLLOW`, non-blocking: no FIFOs, devices or symlinks). Fuzzed in CI |
| Tags and cover art | Size-limited. Covers are decoded with a pixel limit and **re-encoded** before they reach the UI. Fuzzed in CI |
| Workspace files (`project.json`, folders, settings) | Strict schema validation, no symlinks, atomic writes (random exclusive temp file, then rename). Settings files have `0600` permissions |
| File and folder names | Sanitised. Traversal (`..`), absolute paths, separators and Windows reserved names are rejected. Every path is resolved inside the workspace |
| MIDI bytes | A parser that tolerates any byte stream; tested with 1 M random messages |
| Network (model download, optional LALAL.AI) | HTTPS only (rustls). The model is pinned to a URL and revision and **verified by SHA-256 and exact size** before it is loaded. Timeouts on every request |
| The web UI | Strict CSP (`script-src 'self'`, no remote content), frozen prototypes, a minimal Tauri capability set (no shell, fs or http plugins), and no `innerHTML`/`{@html}`/`eval`. These are enforced by tests |
| Audio thread | No `unsafe` anywhere in the codebase. The audio callback does not allocate (checked with `assert_no_alloc`) |

## Automated checks (every push and pull request)

- `cargo audit` (RustSec advisories) and `pnpm audit`.
- Attack suites: `crates/joshua-core/tests/security_attacks.rs`, `review_attacks.rs`, `fuzz_media.rs`, `properties.rs` and `crates/joshua-midi/tests/hostile_input.rs`.
- Static UI checks: `app/src/security.test.ts`.

### Known upstream advisories (no fix available yet)

`cargo audit` reports **0 vulnerabilities**. It shows **warnings** only, all in Tauri's Linux GTK/D-Bus dependency stack:

- `proc-macro-error`, `zbus_names`, `zvariant` and `zvariant_derive` are flagged as **unmaintained**;
- `glib` is flagged as **unsound** (RUSTSEC-2024-0429).

None of them process untrusted input in Joshua+. They are tracked and will be updated when Tauri updates.

## Known limitations

- The beta installers are **not code-signed or notarised** yet. Verify downloads against `SHA256SUMS`; the install scripts do this for you.
- The optional cloud separator sends audio to LALAL.AI, but only when you enable it with your own key.
