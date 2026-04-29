# Contributing to octos-app

Thanks for taking a look. octos-app is the Makepad-Splash native client for
Octos. The repository ships:

- `app/` — the binary (Makepad UI, transport agent glue).
- `crates/octos-app-store` — Makepad-free reducer + selectors.
- `crates/octos-app-transport` — JSON-RPC-over-WebSocket + REST snapshot.
- `crates/octos-app-render` — streaming-markdown wrappers.
- `workstreams/W01–W10` — what to build and in what order.

## Planning docs first

Before opening anything bigger than a typo PR, skim:

| Doc | Purpose |
|---|---|
| `00-CHARTER.md` | mission, scope, non-goals |
| `01-ARCHITECTURE.md` | crate layout, threading, persistence |
| `03-PROTOCOL-CONTRACT.md` | wire summary, reconnect rules |
| `04-IA-AND-NAVIGATION.md` | screen catalog |
| `06-WORKSTREAMS.md` | DAG and parallelism plan |
| `workstreams/W##-*.md` | the workstream that owns the area you're touching |

Almost every change should reference a workstream — link it in the PR.

## Dev loop

Prerequisites: a sibling clone of [`octos`](https://github.com/octos-ai/octos)
at `../octos` (we path-dep `octos-core` from there) and a sibling clone of
[`aichat`](https://github.com/octos-ai/aichat) at `../aichat` (Makepad fork
the binary path-deps).

```sh
make check          # cargo check --workspace
make test           # cargo test --workspace  (72 tests at time of writing)
make run            # cargo run -p octos-app
```

`make help` lists every target. The Makefile auto-sources `.env` if present,
so put `OCTOS_APP_TOKEN=...` and friends there to skip the LoginScreen on
every `make run`.

For the live integration smoke (talks to a real Octos server, gated by
`#[ignore]`):

```sh
OCTOS_LIVE_TOKEN=... OCTOS_LIVE_URL=http://127.0.0.1:56831 make smoke-live
```

`RUNNING.md` is the long-form runbook for live smoke prep.

## CI

`/.github/workflows/ci.yml` runs on every PR and push to `main`:

- `cargo fmt -- --check` (advisory; warn-only).
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo check --workspace`.
- `cargo test --workspace`.

The binary `octos-app` is *excluded* from CI — its Makepad path-deps to
`../aichat` would balloon the cache and cold build past the 12 min budget,
and the binary surface adds little testable behaviour over the three
internal crates. Re-enabling is tracked under W09 / W10.

`/.github/workflows/nightly.yml` adds `cargo test --release` and
`cargo doc --no-deps` daily at 08:00 UTC.

## Style

- `rustfmt.toml` mirrors the upstream Makepad fork (`disable_all_formatting
  = true`); CI's fmt step is advisory only. Keep imports sorted by hand,
  one item per line for the long ones.
- Clippy is a hard gate. The workspace `[workspace.lints.clippy]` block in
  `Cargo.toml` allows a small set of pre-existing lints (`large_enum_variant`,
  `derivable_impls`, `unnecessary_get_then_check`, `collapsible_match`)
  whose clean-up is tracked separately. Don't add new allows without a
  comment and a workstream link.
- One `pub use makepad_widgets;` warning in `app/src/main.rs` is the
  standard Makepad trampoline; leave it.

## Commits

Conventional Commits — `<type>(<scope>): <summary>`. Aligns with the
release-notes generator we plan to wire up in W09. Common types: `feat`,
`fix`, `docs`, `refactor`, `test`, `ci`, `chore`. Scope is the workstream
(e.g. `feat(W04): task dock filters by current session`) or the crate
(`fix(transport): clamp reconnect budget to 5 min`).

Body is optional; if present, explain *why* the change is the right shape.
Reference workstream sections by file + heading
(`workstreams/W04-…md § Task dock`) when relevant.

## Pull requests

PR description checklist:

- [ ] **Workstream link** — `workstreams/W##-*.md`, ideally to the section.
- [ ] **What changed** — 2–4 sentences, focused on intent, not diff.
- [ ] **Tests** — what unit / contract / integration tests cover the change?
      If a behaviour can't be tested, say so and explain.
- [ ] **`make check && make test`** is green.
- [ ] **Screenshot / clip** — required for any UI-visible change. Drop a
      PNG into the PR description directly.
- [ ] **Manual smoke** — if you ran `make smoke-live` against a live
      server, paste the result block (`PASS` / `FAIL` + one line).
- [ ] **Doc updates** — `STATUS.md`, `RUNNING.md`, the relevant
      workstream — touched if the change moves the bar.

## Reviews

- One reviewer is enough until M2. After M2, two for any change in
  `app/`, `crates/octos-app-transport`, or `crates/octos-app-store`.
- W10 owner is the gatekeeper for flaky-test triage and CI changes.

## Filing issues

Tag with the workstream first, severity second (`S0` block-release, `S1`
block-merge, `S2` next-release, `S3` cosmetic — see W10 § "Bug triage
workflow"). A repro that the maintainer can paste into `make smoke-live`
or a unit test stub is worth ten paragraphs of prose.

## License

By contributing you agree your changes are licensed under MIT OR
Apache-2.0 (per the workspace `Cargo.toml`).
