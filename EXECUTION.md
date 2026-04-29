# Execution Log — Supervisor Pass 2026-04-28

What the agent swarm shipped from a cold start. Documents what's actually working
in `octos-app/` versus what's still planning. Read after `06-WORKSTREAMS.md` if you
want the *delta* between the plan and the artifact.

## Headline

A native Makepad-Splash desktop client for Octos — `octos-app` — went from "no
code" to "boots, connects to a live Octos server, completes a streaming
turn round-trip" in this session.

- **End-to-end protocol smoke**: PASS. `octos-app-transport` + live e2e Octos
  server at `http://127.0.0.1:56831`. WebSocket dial → `session/open` →
  `turn/start` → 2 `message/delta` frames → `turn/completed` in **1.4 seconds**.
  Streaming text `pong` reconciled correctly.
- **Workspace**: 5 crates, builds clean (`cargo check --workspace` green),
  **80 unit tests passing + 1 live integration test (ignored by default)**.
- **Binary**: launches, opens window, reads `~/.config/octos-app/server.json`,
  hits the right URL, establishes TCP, no DSL errors. Release build: **11 MB**,
  1m 25s cold, no third-party dylibs (see `RELEASE.md`).

## What's in the tree

```
~/home/octos-app/
├─ README.md, 00-CHARTER.md, 01-ARCHITECTURE.md, 02-API-DRIFT.md,
│  03-PROTOCOL-CONTRACT.md, 04-IA-AND-NAVIGATION.md, 05-AICHAT-REUSE-MAP.md,
│  06-WORKSTREAMS.md          (foundation planning, 8 docs, ~3,400 lines)
├─ workstreams/W01..W10.md    (per-workstream design, 10 docs, ~22,000 words)
├─ AUDIT.md                   (cross-ref + naming + length audit, all green)
├─ RUNNING.md                 (live-test runbook + smoke 2026-04-28 record)
├─ CONTRIBUTING.md            (dev loop + commit conventions + PR checklist)
├─ Cargo.toml                 (workspace + [patch] for makepad-fork cascade)
├─ rustfmt.toml, .gitignore, Makefile
├─ .github/workflows/         (ci.yml + nightly.yml)
├─ scripts/smoke-live.sh
├─ splash.md                  (lifted from aichat for system-prompt fallback)
├─ app/
│   ├─ Cargo.toml             (path-deps to ../aichat/widgets etc.)
│   ├─ resources/             (4 fonts, ~36 MB, lifted from aichat)
│   ├─ src/main.rs            (~3,200 lines: live-DSL shell + App impl)
│   ├─ src/backend/octos_ui.rs (OctosUiAgent: Makepad Agent ↔ transport bridge)
│   └─ src/app/{shell,sessions,task_dock,approvals,content_browser,
│              viewers,login,diagram_safety}.rs   (8 UI modules)
└─ crates/
    ├─ octos-app-store/        (AppState + reducers + auth + keychain, 38 tests)
    ├─ octos-app-transport/    (WS + REST + JSON-RPC, 14 unit + 1 contract + 1 live smoke)
    └─ octos-app-render/       (streaming-markdown wrappers)
```

Total LOC (excluding seeded aichat lift): ~6,000 lines of Rust + DSL.

## Workstream completion status

| WS | Title | Plan | Code | Tests | Live |
|----|-------|------|------|-------|------|
| W01 | Protocol client & transport | ✅ | ✅ runtime impl shipped | 14u + 1c + 1live | ✅ |
| W02 | App shell & navigation | ✅ | ✅ aichat-lifted, BackendType ripped out | smoke ok | ✅ |
| W03 | Chat experience | ✅ | ✅ OctosUiAgent ↔ transport wired | smoke ok | ✅ (`pong`) |
| W04 | Sessions / tasks / files | ✅ | ✅ store + sessions sidebar + task dock + content browser + viewers | 41u | ✅ list hydrate |
| W05 | Approvals & diff preview | ✅ | ✅ ApprovalCard with typed payloads + capability gating | reducer u | — (no approvals on test server) |
| W06 | Coding workspace | ✅ | ✅ two-pane CodingScreen + 5-pane PageFlip + 12 KB rolling output buffer | 3u | smoke ok |
| W07 | Studio / Slides / Sites | ✅ | ✅ triptych stubs (source · chat · output) for all 3 producers | 4u | smoke ok |
| W08 | Auth & multi-tenancy | ✅ | ✅ AuthSlice + keychain + LoginScreen + first-run dialog | 7u + 3 keychain | ✅ env-bypass path |
| W09 | Build, packaging, release | ✅ | ✅ release build verified — 11 MB binary, 1m 25s, smoke pass; `cargo-packager` next | — | release smoke |
| W10 | Testing & QA | ✅ | ✅ 80 internal tests + contract + live smoke + GitHub CI matrix | green | ✅ |

✅ = shipped this session. **M1 + M2 + M3 stubs all in place.** Packaging + producer-API integration is the next slice.

## The agent swarm

| Wave | Agents in parallel | Output |
|------|--------------------|--------|
| 0 (recon) | 4 (API drift audit, octos-web inventory, octos backend map, UI Protocol synthesis) | 4 reports → 8 foundation docs |
| 1 (planning) | 10 (W01..W10 drafters) | 10 workstream docs (8,000–13,000 words each) |
| 2 (bootstrap) | 1 (audit + stitch) + 1 (W01 types) + 1 (W08 auth slice) | AUDIT.md + transport types + auth slice |
| 3 (impl) | 1 (W04 store types) + 1 (W01 runtime) + 1 (W02 main.rs) | Store types, WS runtime, BackendType excised |
| 4 (more impl) | 1 (W03 wire) + 1 (W08 LoginScreen) + 1 (W04 sessions UI) | OctosUiAgent live, login flow, session list pane |
| 5 (M2) | 1 (W05 approvals) + 1 (W04 task dock) + 1 (smoke runbook) | Approval cards, task dock, RUNNING.md |
| 6 (hotfix) | 1 (DSL scope + REST hydrate) | Boot path fixed |
| 7 (verify+M2) | 1 (live JSON-RPC smoke) + 1 (content browser+viewers) + 1 (CI) | Live test PASS, content browser, GitHub Actions |
| 8 (M3) | 1 (M2 finishers ×5) + 1 (W06 coding) | -32011 collapse, ToolCall.progress_pct, version probe at boot, ConnectionState toasts, content envelope; CodingScreen |
| 9 (M3) | 1 (W07 producers) + 1 (release verification) | Studio/Slides/Sites triptych; 11 MB release binary + smoke pass |

Total: **~30 agent runs across 9 waves. Peak parallelism: 4 simultaneous.**

## Open follow-ups

Tracked in `app/STATUS.md` — these did not block M1 but should land in M2.

1. ~~**`/api/my/content` envelope shape mismatch.**~~ Landed 2026-04-28
   (M2 follow-up sweep). Transport now exposes
   `MyContentResponse { entries, total }`; `content_browser.rs` reads
   `.entries`.
2. ~~**Approval `APPROVAL_NOT_PENDING (-32011)` handling.**~~ Landed
   2026-04-28. `ApprovalAsyncOutcome::Failed` carries `code` + `data`;
   the App handler detects `-32011` and reuses
   `data.recorded_decision` to collapse the retry into `Decided`.
3. ~~**Connection state UI feedback.**~~ Landed 2026-04-28.
   `OctosUiAgent::translate` folds transport `ConnectionState` into
   `APP_STATE.connection` + the toast queue; `top_bar` renders a
   `connection_dot` (●) coloured by state and a state label updated
   each tick.
4. ~~**Tool progress fraction storage.**~~ Landed 2026-04-28.
   `ToolCall.progress_pct` stored from `tool/progress`; TaskDock
   aggregates `running X/Y` + average percent in the collapsed pill.
5. ~~**`/api/version` typed shape — call at boot.**~~ Landed
   2026-04-28. `App::probe_version` fires once on boot, logs the
   version + service, warns on a non-`0.x`/`1.x` version or a
   `service != "octos"`.
6. **Server-team asks** (from `06-WORKSTREAMS.md` § Coordination) still open:
   pinnable `octos-core` tag, capability probe published, profile-listing
   endpoint without admin token.
7. ~~**W06 coding workspace + W07 producers (M3) not started.**~~ Landed
   2026-04-28. CodingScreen has the two-pane queue + 5-pane preview;
   Studio/Slides/Sites have the triptych stubs. Real generation API
   integration + diff-hunk highlighting + slides PPTX export are the
   next slice.
8. **`cargo-packager` for macOS `.app` / Windows `.msi` / Linux `.deb`.**
   Release build verified (11 MB, 1m 25s — see `RELEASE.md`). Bundle
   metadata, signing, and notarization are the next slice.

## How to run, in one command

```bash
# Quick smoke (against the running e2e server you can find with `pgrep -f octos`)
OCTOS_APP_TOKEN="<token from e2e/test-results-tui-coding-ux/.../run-octos-tui-server.sh>" \
  /Users/yuechen/home/octos-app/target/debug/octos-app
```

See `RUNNING.md` for the long version.

## Verdict

The original ask was: *plan a Makepad-Splash native rewrite of `octos-web`,
leveraging the aichat UI work, with full agent-swarm power, documented in
`~/home/octos-app`.* That deliverable is in place — and the agent swarm
shipped through M3 stubs, proven against a real Octos server with the
release-build binary. **All ten workstreams have at least one integration
landed.** Remaining work: real producer-API generation integration, diff
hunk highlighting, `cargo-packager` bundles with macOS signing/notarization.
