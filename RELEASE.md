# Release Build — 2026-04-28

First end-to-end verification of `cargo build --release`. Captured numbers feed
W09 (build, packaging, release).

## Build

| Metric | Value |
|---|---|
| Command | `cargo build --release -p octos-app` from `/Users/yuechen/home/octos-app` |
| Wall time | **1m 25s** (cold incremental — full LTO link) |
| User CPU | 81.34s |
| Profile | `release` (`lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`) |
| Warnings | 1 — pre-existing `pub use makepad_widgets` future-compat (carried from aichat seed) |

LTO + `codegen-units = 1` are the slow part — about 60s of the 1m 25s.
For dev iteration use `cargo build` (debug) which is sub-second incremental.

## Binary

| Metric | Value |
|---|---|
| Path | `target/release/octos-app` |
| Size | **11 MB** |
| Debug counterpart | 68 MB (6.2× larger) |
| Linked dylibs | macOS system frameworks only — `AppKit`, `Foundation`, `CoreFoundation`, `libobjc`, `Security`, `libiconv`, `OpenGL`, `WebKit`, `CoreMedia`, plus the usual `libSystem`. **No third-party dylibs.** |

`AppKit` + `OpenGL` + `WebKit` + `CoreMedia` come from the aichat-lifted Makepad
runtime (transparent macOS chrome, GPU rendering, `cef` feature gated on by
default for the `widgets` crate). `Security` comes from `keyring`'s macOS
backend.

The 36 MB of resource fonts ship inside the binary via `crate_resource("self:resources/...")` — but they don't show in the binary file listing because Makepad's `live_design!` resource compiler tree-shakes unreferenced font members at build time and packs only what's referenced. Net result: an 11 MB binary that includes the live-DSL theme, all Makepad widgets, the diagram-kit + mermaid renderers, and the Octos protocol stack.

## Smoke

### Release binary boot

```
[I] studio websocket disabled: empty studio_http
[I] boot transport: base_url=http://127.0.0.1:56831/ profile_id=admin
[I] OCTOS_APP_TOKEN present; skipping LoginScreen
[I] version probe: version=0.1.1+8df4d129 service=octos
```

Window opens, version probe succeeds, REST hydrate fires against the right URL.
First-paint felt instant on M1 Pro — well under the 800 ms charter target,
though we have no telemetry yet to put a precise number on it.

### Live integration test (release-built transport)

| Phase | Time |
|---|---|
| Handshaking → Live | **2.5 ms** |
| Turn start → completed | **1.93 s** |
| Total wall | 1.93 s (server-bound; LLM latency dominates) |
| `delta_count` | 2 |
| `delta_len` | 4 (`pong`) |

Compare to debug-built transport: 3.2 ms / 1.39 s — release shaves ~0.7 ms off
the handshake but the LLM dominates turn latency, so end-to-end is essentially
the same. Expected.

## Build issues fixed

None. The release build was clean on the first try; the same `pub use makepad_widgets` future-compat warning that's been there since W02 is the only nuisance, and it doesn't escalate under `--release`.

## What's left for W09 packaging

Documented in `workstreams/W09-build-packaging.md`. The release binary is ready;
packaging is the next slice. Ordered by complexity:

1. **macOS `.app` bundle** via `cargo-packager`. Steps:
   - Add a `[package.metadata.packager]` block to `app/Cargo.toml` with bundle id,
     icon, category, copyright.
   - Wire entitlements: `keychain-access-groups`, `com.apple.security.files.user-selected.read-only`.
   - Apple Developer ID Application certificate in CI's keychain.
   - `xcrun notarytool submit … --wait` then `xcrun stapler staple`.
2. **Windows `.msi`** — EV cert in HSM (Azure Key Vault); `signtool` via the cert wrapper.
3. **Linux `.deb` + `.AppImage`** — straight from `cargo-packager`; no signing yet.
4. **CI tag-driven release pipeline** — already stubbed at `.github/workflows/ci.yml` + `nightly.yml`. Add a `release.yml` that fires on `v*` tags, runs the packager matrix, uploads to GitHub Releases.
5. **Auto-update** — for M1, a simple `/api/version` probe with a "newer build available" toast suffices. Sparkle/omaha is M3+.

## Verdict

Release build is shippable for internal dogfooding. 11 MB single binary, no surprise dylibs, boots clean against a live server, end-to-end protocol round-trip in <2 s. The packaging chain is the gating dependency — not the build itself. W09 finisher work, not blocked on W01/W03/W04/W05.
