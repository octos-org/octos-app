# W09 — Build, packaging, release

## Mission

Own the road from `cargo build --release` to a signed installer a non-technical
user double-clicks: the workspace `Cargo.toml`, build profiles, the `octos-core`
pin, the 36 MB font bundle, four packagers (`.app`, `.msi`, `.AppImage`, `.deb`),
macOS notarization, Windows EV signing, an auto-update channel, light telemetry
hooks, and a tag-driven CI matrix. By M2, `octos-app-<ver>.dmg` opens with no
Gatekeeper warning. By M3, the app surfaces "newer version available" without
the user revisiting a webpage.

## Header

| | |
|---|---|
| Lane | Q — QA & build |
| Depends on | W02 (App shell — first builds need a binary that boots) |
| Lifts from | `aichat`'s `Cargo.toml`; `tools/cargo_makepad/src/apple/compile.rs` codesign args |
| Output | `octos-app/Cargo.toml`, `app/Cargo.toml`, `packaging/`, `.github/workflows/release.yml` |
| Milestone | M1+ continuous (binary M1, signed installers M2, update banner M3) |

## Scope

**In:** workspace `Cargo.toml` + profiles; `octos-core` pin (path-dep dev,
git-tag CI); `crate_resource` resource bundle; four packagers; macOS Developer
ID + notarize + staple; Windows EV `signtool`; M1 "version older than server's"
banner (auto-download deferred); M2 opt-in structural-only telemetry; tag-driven
release pipeline.

**Out:** A/B testing, feature flags (single channel); crash-analytics
dashboard; App-Store / MS-Store / Snap / flatpak; mobile / WASM packaging;
localization bundles.

## Workspace layout

Mirrors `01-ARCHITECTURE.md` §2:

```
octos-app/
├─ Cargo.toml                      # workspace
├─ app/Cargo.toml                  # binary; deps mirror examples/aichat/Cargo.toml
├─ crates/octos-app-{transport,store,render}/Cargo.toml
├─ packaging/{packager.toml, macos/, windows/wix.wxs, linux/}
└─ .github/workflows/release.yml
```

Concrete shape:

```toml
[workspace]
members = ["app", "crates/octos-app-transport",
           "crates/octos-app-store", "crates/octos-app-render"]
resolver = "2"

[workspace.package]
version = "0.1.0"; edition = "2021"; rust-version = "1.95"

[workspace.dependencies]
octos-core      = { git = "https://github.com/<org>/octos.git", tag = "octos-core-v0.7.3" }
makepad-widgets = { git = "https://github.com/makepad/makepad.git", tag = "v1.0.x", features = ["pdf"] }
makepad-ai      = { git = "https://github.com/makepad/makepad.git", tag = "v1.0.x" }
streaming-markdown-kit = { git = "...", branch = "main", features = ["mermaid"] }
makepad-diagram-kit    = { git = "...", branch = "main", features = ["makepad"] }
robius-open = { git = "https://github.com/project-robius/robius" }

[profile.release]      lto="thin", codegen-units=1, strip="debuginfo"
[profile.release-small] inherits="release", opt-level="z", lto=true, panic="abort"
[profile.dist]         inherits="release", debug="line-tables-only"   # CI publishes
```

**Pinning `octos-core`.** Workspace dep is git+tag. For local dev, a `make
dev-link` script materializes a `[patch."https://github.com/<org>/octos.git"]`
overlay pointing at `vendor/octos/crates/octos-core` — same shape aichat's parent
workspace uses for `[patch.crates-io]` / `[patch."…/makepad.git"]`. Tag bumps at
every M-milestone exit; ad-hoc on UPCR landings.

## Resource bundling

Lift aichat's four-member font fallback (`examples/aichat/src/main.rs:43–55`)
intact into `app/resources/`: LXGW Mono 25 MB (CJK monospace), NotoColorEmoji
10 MB, NotoSans ~700 KB (Latin / symbols), Liberation Mono ~300 KB (Latin
monospace). Total ~36 MB. Loaded via `crate_resource("self:resources/...")` —
embedded at build time. Final binary ~140 MB, budgeted to 200.

`aichat` enables `makepad-widgets` features `["maps", "pdf"]`. We keep `pdf`
(W04 file viewer) and drop `maps` — nothing in `04-IA-AND-NAVIGATION.md` needs
it; re-enable behind a flag later if a producer asks.

## Packaging tooling choice

| Tool | Pros | Cons |
|---|---|---|
| **`cargo-packager`** | One config → `.app`/`.dmg`/`.msi`/`.AppImage`/`.deb`. Built-in macOS sign+notarize. Charter §5 names it. | Smaller community; some Info.plist edges need template overrides. |
| `cargo-bundle` | Older, well-known. | No release in years; no MSI; notarize is DIY. |
| `cargo_makepad` (in-tree) | Already runs `codesign` (`apple/compile.rs:1123,1136`); understands `crate_resource`. | Built for Makepad dev loop. No MSI / AppImage / notarize helper. |

**Decision: `cargo-packager` ships M1.** Borrow the `codesign` arg shape from
`cargo_makepad` (debugged) but invoke through packager's `macos.signing-identity`
/ `notarization` blocks. Use `cargo_makepad` only for live-reload dev (`cargo
makepad run`); production goes through packager.

## macOS signing & notarization

1. **Cert** "Developer ID Application: Octos Inc (TEAMID)" imported via
   `security create-keychain` + `security import` from a base64 secret.
2. **Hardened runtime** `--options runtime` (default when packager
   `macos.hardened-runtime = true`).
3. **Entitlements** (`packaging/macos/entitlements.plist`):
   `com.apple.security.cs.allow-jit` (Splash), `…files.user-selected.read-write`
   (W04 viewer), `…network.client` (WS / REST),
   `keychain-access-groups: $(TeamIdentifierPrefix)octos.app` (W08 `keyring`).
4. **Sign bundled binaries** before signing the outer `.app`. Packager
   automatic.
5. **Notarize** `xcrun notarytool submit octos-app.dmg --apple-id … --team-id …
   --wait` with app-specific password from secrets; 5–20 min typical, 60 min
   timeout.
6. **Staple** `xcrun stapler staple octos-app.dmg` so it opens offline.
7. **Verify** `spctl --assess -t install …` + `codesign -dv --verbose=4 …` gate
   upload.

## Windows MSI

`cargo-packager` `windows.wix` block.

- **EV cert in HSM** (Sectigo / DigiCert in Azure Key Vault or DigiCert
  KeyLocker). CI calls `signtool sign /tr <RFC3161> /td sha256 /fd sha256 /a`
  via the HSM provider — cert never hits the runner's disk.
- **SmartScreen** still warns for the first ~50–100 downloads on a new EV
  identity. Accepted; v0.1.0 notes call it out.
- **MSI shape** per-user (no admin), HKCU footprint, real Add/Remove entry,
  stable upgrade code so 0.2 replaces 0.1 cleanly.
- **Chocolatey** deferred — moderation friction not worth it pre-M3.

## Linux (.deb + .AppImage; flatpak deferred)

Two artifacts, neither code-signed:

- **`.deb`** for Debian 12+ / Ubuntu 22.04+. `Depends: libgtk-3-0, libxcb1,
  libssl3`. Installs `/opt/octos-app/` + `/usr/bin/octos-app` symlink + `.desktop`.
- **`.AppImage`** via packager's `appimage` target (`appimagetool`). Same
  icons + `.desktop`. Recommended for non-Debian distros.
- **Flatpak / snap** deferred — manifest maintenance / confinement-update
  friction.

Fonts ship in-binary on Linux too; `crate_resource` takes precedence over
fontconfig. W10 verifies on a clean Ubuntu container.

## Auto-update strategy

- **Sparkle** — appcast XML + signed delta dmgs. Best UX, mac-only.
- **Omaha-style** (Chrome's) — cross-platform but we'd run an update server.
  Overkill.
- **Hand-rolled probe** — call `/api/version` on launch; if local <
  `latest_client_version`, banner + "Download update" button opens releases via
  `robius_open`.

**Decision: hand-rolled probe ships M1.** `/api/version` is already on the
server-team asks list (`06-WORKSTREAMS.md`); piggy-back the
`latest_client_version` field. Auto-download is **not** in M1 or M2 — user
clicks through. Revisit a real updater (Sparkle on macOS, equivalent elsewhere)
at M3 once cadence justifies it.

## Telemetry / error reporting

- **M1: nothing.** Panic hook writes a stack trace to `~/Library/Logs/octos-app/
  panic.log` (+ OS equivalents). Users attach manually.
- **M2: opt-in, structural-only.** `sentry`-crate captures `panic!`s and
  toast-surfaced `Err`s. Hard rule: **no message / prompt / file content**.
  Schema is event class (`reconnect_failed`, `approval_render_error`, `panic`),
  call site, protocol cursor. Default off; enable shows a consent dialog.
- **M3+** server-side aggregation owned by server team — not W09.

`octos-app-store::telemetry::Reporter` trait: M1 no-op; M2 Sentry impl behind
`--features telemetry`.

## Release pipeline

`.github/workflows/release.yml`, trigger tag `v*`:

1. **Lint + test** (`ubuntu-latest`): `cargo fmt --check`, `cargo clippy
   --workspace --all-targets`, `cargo test --workspace`.
2. **Matrix** (3 parallel):
   - `macos-14` — `cargo packager build --target universal-apple-darwin
     --profile dist`; sign + notarize + staple.
   - `windows-2022` — `cargo packager build --target x86_64-pc-windows-msvc
     --profile dist`; HSM-backed `signtool`.
   - `ubuntu-22.04` — `.deb` + `.AppImage` x86_64; add `aarch64` on demand.
3. **Verify** `spctl`, `codesign -dv`, `signtool verify`, `dpkg -I` gate upload.
4. **Publish** `softprops/action-gh-release` → draft release; body auto-generated
   from `git log <prev-tag>..HEAD` filtered by Conventional-Commit prefix; manual
   edit before promoting.
5. **Notify server** POST the new tag so `/api/version` updates.

**Secrets** `MACOS_CERT_P12` / `_PASSWORD`, `APPLE_ID`, `APPLE_ID_PASSWORD`,
`APPLE_TEAM_ID`, `WINDOWS_HSM_*` (vault URL, client id/secret, cert name),
`GH_RELEASE_TOKEN` — stored in a `production-release` GitHub environment
requiring manual approval to publish.

## Deliverables

1. `Cargo.toml` workspace + per-crate `Cargo.toml`s with `[workspace.dependencies]`
   and three release profiles.
2. `octos-core` git-tag pin + `make dev-link` for path-dep override.
3. `app/resources/` populated with the four font files.
4. `packaging/packager.toml` + per-OS asset directories.
5. `packaging/macos/{entitlements.plist, Info.plist.tmpl}` with the listed
   entitlements.
6. `.github/workflows/release.yml` with the three-job matrix; `packaging/
   SECRETS.md` checklist.
7. `octos-app-store::telemetry::Reporter` trait + no-op M1 impl; Sentry impl
   behind `--features telemetry`.
8. `octos-app-store::version_check`: `/api/version` REST call, semver compare,
   banner-trigger event.
9. `packaging/RELEASE.md` runbook: tag → publish + manual fallbacks
   (notarization stuck, EV-cert HSM down).

## Tests & verification

- CI matrix passes on every push to `main` and every release tag.
- **Smoke per artifact** (W10 owns harness; W09 contributes launch script):
  clean VM (macOS Ventura, Win11, Ubuntu 22.04), launch, connection indicator
  reaches `Connected` against a stub server, quit cleanly.
- **Signing gates** macOS `spctl --assess --type execute` "accepted" + `xcrun
  stapler validate` "stapled"; Windows `signtool verify /pa /v` succeeds; Linux
  `dpkg -I` clean control metadata; AppImage runs without `--no-sandbox` in a
  clean Ubuntu container.
- **Binary-size budget** `du -h target/dist/octos-app` fails above 200 MB.
- **Update probe** unit test against a wiremock fixture; semver newer / equal /
  older / pre-release inputs covered.
- **Cold-start budget** (Charter success #3): smoke records first-paint on M1
  Pro; threshold 800 ms; regression fails.

## Exit criteria

1. Tag `v0.1.0` produces three signed artifacts (`.dmg`, `.msi`, `.deb` +
   `.AppImage`) uploaded to a GitHub release automatically.
2. Each artifact installs and launches on a clean OS image with no
   Gatekeeper/SmartScreen/permission warnings beyond first-run reputation.
3. `octos-core` pinned by git-tag; bump is a one-line workspace change.
4. Fonts ≤50 MB; final binary ≤200 MB.
5. Auto-update banner shows when local version < `latest_client_version`.
6. Release pipeline wall-clock under 45 min.
7. `packaging/RELEASE.md` is enough for a new release engineer to ship without
   W09's original author.

## Risks

| Risk | Mitigation |
|---|---|
| **Font bloat** — 36 MB is most of install size. | English-only build M1–M3; per-locale packs post-M3. 50 MB hard line; CI fails if breached. |
| **Cert renewal** — Developer ID and EV certs take days. | Track in `SECRETS.md`; renew 60 days early; keep previous cert valid until new ships. |
| **Notarization stalls** (~2 hr Apple incident delays). | 60 min timeout + re-queue; RELEASE.md documents `notarytool log` recovery; don't gate test builds. |
| **EV SmartScreen** — first ~50–100 downloads warn. | Document in v0.1.0 notes; reputation accrues automatically. |
| `cargo-packager` regressions on macOS universal builds. | Pin to a specific tag; bump deliberately. `cargo_makepad` manual dmg as last-resort. |
| `octos-core` tag drift. | Use immutable `octos-core-vX.Y.Z` only; on any retag, switch to a commit SHA. |
| Linux runtime deps missing. | `.AppImage` bundles; `.deb` lists `Depends:`. Smoke-test on minimal Ubuntu (no `-desktop` meta). |
| Update banner nag-blind. | Show once per launch, dismissible; never re-prompt for the same version. |

## Open questions

1. **Universal macOS or two builds?** Default universal M1–M2; revisit on size
   complaints.
2. **MSI per-machine vs per-user?** Default per-user; revisit when enterprise
   asks system-wide.
3. **Sign `.deb`?** `dpkg-sig` exists but isn't widely verified. Skip M1–M2;
   revisit if a distro flags us.
4. **Stable / beta channels?** Single "stable" through M3; beta proposed M4.
5. **Notarize every PR build?** Probably no — sign `main`, notarize only on
   release tags. Revisit if nightly ships.
6. **Crash-dump UX before M2?** M1 is local-file-only. A one-click "send to
   support@" is a small W09 add — flagging in case it slips.
