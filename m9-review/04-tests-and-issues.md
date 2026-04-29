# M9 Review — Tests & Issues

Date: 2026-04-28. Reviewer scope: `/Users/yuechen/home/octos/e2e/tests/` (M9-era specs on `coding-green-m9-local-20260428`) plus open GitHub issues against the M9 family.

## Verdict

**RED on test reliability for the M9 *protocol* surface; AMBER on bug burden.**

The 8 specs labeled "M9-era" don't actually exercise the UI Protocol v1 wire methods (`session/open`, `turn/start`, `turn/interrupt`, `approval/respond`, `diff/preview/get`, `task/output/read`, `tool/started`, `tool/completed`, `task/updated`, `turn/completed`, `message/delta`). They drive the legacy SSE `/api/chat` surface (M8 + M8.10) and assert against the rendered DOM. The new wire methods exist only in `crates/octos-core/src/ui_protocol.rs` (Rust unit tests for JSON shape) and have **no live integration test, no fault-injection test, no double-call/cursor-stale/cursor-future probe, and no e2e probe** — only a tmux-driven `octos-tui --readonly` smoke test (`/Users/yuechen/home/octos/e2e/tmux/run.sh`) that verifies the TUI binary connects. Calling the M9 protocol "stable" today would be premature.

## Test coverage matrix (M9 wire surface)

| M9 method | Live e2e test | Mock/unit test | Status |
|-|-|-|-|
| `session/open` | none | Rust unit (`ui_protocol.rs:1784`) | shape only |
| `turn/start` | none | Rust unit (`ui_protocol.rs:1894`) | shape only |
| `turn/interrupt` | none | Rust unit | shape only |
| `approval/respond` | none | Rust unit (`ui_protocol.rs:2294,2306`) | shape only |
| `diff/preview/get` | none | Rust unit | shape only |
| `task/output/read` | none | Rust unit | shape only |
| `turn/started` | none | Rust unit | shape only |
| `turn/completed` | none | Rust unit | shape only |
| `message/delta` | none (SSE `replace`/`token` covered) | Rust unit | shape only |
| `tool/started` | none (SSE `tool_progress` covered) | Rust unit | shape only |
| `tool/completed` | none (SSE `tool_end` covered) | Rust unit | shape only |
| `task/updated` | none (REST `/api/sessions/:id/tasks` polled) | Rust unit | shape only |

The 8 "M9-era" Playwright specs (`live-server-thread-id`, `live-spawn-end-to-end`, `live-thread-interleave`, `live-thread-persistence`, `live-tool-progress`, `live-tool-retry-collapse`, `m8-runtime-invariants-live`, `speculative-defect-c-probe`) are M8.10 / M8 follow-ups, not M9-protocol tests. All run **live** against a real `dspfac.*.ominix.io` host, none against a mocked WS server.

## Reliability counts on the M9-era specs

- `test.skip(...)` paths: **8** total — `live-spawn-end-to-end` (4), `m8-runtime-invariants-live` (4), `live-tool-retry-collapse` (2), `speculative-defect-c-probe` (1). All are graceful-degrade skips when a feature/host condition is absent (cancel API not deployed, no SSH, LLM nailed first try, etc.).
- `test.fixme`: **0**.
- Hard `setTimeout`/`new Promise(...setTimeout)`: ubiquitous as poll intervals (2-5 s); spec timeouts up to 900 s on `live-spawn-end-to-end`. No retry decorators.
- `test.describe.serial` / `--workers=1` requirement: yes, all live specs require serial execution to avoid quota collisions.
- "flaky/race/intermittent" comments: 0 in spec bodies; #618, #632, #634, #636 are open issues that DO label the SSE thread_id race + tool-retry spec as flaky.

## Top flaky tests + workarounds

| Test | Failure | Tracking | Workaround |
|-|-|-|-|
| `live-tool-retry-collapse.spec.ts` | LLM nails Chinese-city weather on first try → no retry → assertion `retryCount >= 1` fails | #636 | spec now `test.skip()`s gracefully when `retryCount===0`; needs `OCTOS_FORCE_TOOL_RETRY=1` synthetic path |
| `live-server-thread-id.spec.ts` (SSE) | first `thinking` + early `replace` events emit `thread_id=null` even after PR #635's sticky map | #632, #636 | partial fix #637 binds thread_id earlier; not yet verified clean on all minis |
| `live-thread-persistence.spec.ts` (implicit) | `done` event fires 5-9 s before JSONL commit → reload returns partial history | #634 | web client retries history on mount at 0/2/5/12 s; spec waits 90 s before reading post-reload |
| `live-tool-progress.spec.ts` | `tool_progress` events not surfacing during plugin execution | #618 | none merged; spec fails on mini1 deploy `d191c2c7` |
| `live-cost-tracking.spec.ts` (adjacent) | second pipeline in same session times out at 480 s | #615 | none |

## Fault-injection coverage

- Connection drop mid-turn: **none**.
- Cursor stale (replay from old offset): **none**.
- Cursor future (replay from offset > committed_seq): **none**.
- Double `turn/start` for the same session: **none**.
- `turn/interrupt` after `turn/completed`: **none**.
- `approval/respond` twice for one approval id: **none**.

The closest fault-injection tests are M8-era: workspace-deletion mid-session in `m8-runtime-invariants-live`, cancel-during-spawn in `live-spawn-end-to-end`, and `speculative-defect-c-probe` (a diagnostic probe, not an assertion). None of these touches the M9 wire.

## Live vs mocked

- Live (against real `octos serve` on `dspfac.{crew,bot,octos,river,ocean}.ominix.io`): **all 8 M9-era specs + 25 other live specs**. SSH-host-pinned via `HOST_MAP`; serial `--workers=1`; LLM rate-limited.
- Mocked WS server: **zero specs**. Playwright `routeWebSocket` API exists in `node_modules` but is unused by any spec.
- Tmux-driven: `octos-tui` connects to `ws://127.0.0.1:9/api/ui-protocol/ws` in read-only mode and verifies CLI help / banner — log artefacts under `/Users/yuechen/home/octos/e2e/test-results-tmux/`. This is the only test that touches the M9 protocol path at all, and only as a connect-and-print smoke test.

## Open issue bucket A — M9 protocol bugs (all open, recent)

None of the open issues directly land against the M9 wire methods because no production client uses them yet. M9.1–M9.8 issues (#566–#573) are still all *enhancement / design / long-term* — none labelled `bug`. The closest defect-class issues that gate M9 are M8.10 follow-ups:

| # | Title | Severity | Summary |
|-|-|-|-|
| #632 | M8.10 SSE `thread_id` race: first thinking + early replace events emit without thread_id | **blocker** | Without thread_id on first events the sticky map can't repair; #635 only halved the leak. |
| #636 | PR #635 didn't fully close SSE thread_id race + tool-retry spec flake | **blocker** | 3 events/turn still leak; `live-tool-retry-collapse` flakes on Anthropic models. |
| #634 | SSE `done` event fires 5-9 s before JSONL commit | **serious** | History reload after `done` returns partial result; UI retries are a band-aid. |
| #618 | `tool_progress` SSE events not surfacing during plugin execution | **serious** | Live regression on mini1 deploy `d191c2c7`; user sees a blank assistant placeholder. |
| #627 | M8.10 (revised) — Thread-by-cmid chat data model | **serious** | Underlying model change that #632/#634/#636 sit on top of; PRs #1–#5 partially landed. |
| #580 | M4.1A `live-progress-gate`: built-in `DeepSearchTool` misses `synthesize` phase | **minor** (bug-labelled) | Test 1 skipped pending alignment of phase ladder; cosmetic for M9. |
| #626 | Soul config not isolated across profiles/users | **serious** | Multi-tenant data isolation; orthogonal to M9 wire but affects shared-host correctness. |

## Open issue bucket B — M8 fix-first checklist that gates M9 stability (per spec § 12)

All listed in `docs/OCTOS_M8_FIX_FIRST_CHECKLIST_2026-04-24.md`. Spec § 12 says "M9 should not freeze over known M8 runtime defects".

| # | Title | Severity | Status |
|-|-|-|-|
| #536 | M8.1 — Typed `ToolContext` | **blocker** (foundation) | open |
| #537 | M8.2 — `AgentDefinition` manifest format | serious | open |
| #538 | M8.3 — Profile system | serious | open |
| #539 | M8.4 — `FileStateCache` | serious | open |
| #540 | M8.5 — 3-tier compaction | minor for M9 | open |
| #541 | M8.6 — Structured resume pipeline | **blocker** | open (sanitizer correctness still under review per checklist § 2) |
| #543 | M8.8 — Concurrent-safe vs exclusive scheduler | serious | open (the rewrite that lost `agent_definitions` + `file_state_cache` per § 1) |
| #575 | M8.10 — Strict timestamp + historySeq + sync-tool quota + loop-dup | serious | open (parent of #627) |

The checklist's six exit criteria (`ToolContext` propagation, resume sanitizer, worktree-missing refusal, M8.7 wiring, profile/manifest authority, concurrency classification) are **not yet collectively green**. Calling M9 "stable" while these are open contradicts spec § 12.

## Open issue bucket C — recent ui_protocol churn (last 30 days)

Touching `crates/octos-core/src/ui_protocol.rs` and `crates/octos-cli/src/api/ui_protocol*.rs`:

- `70c756d8` (Apr 28) — "Add coding green M9 protocol and sandbox work": **+8042 LOC** in one commit landing the entire M9 protocol scaffolding (`ui_protocol.rs` 2949 LOC, `_approvals.rs` 465, `_diff.rs` 395, `_ledger.rs` 328, `_progress.rs` 573, `_task_output.rs` 532, core types 2718). No tests added in `crates/octos-cli/tests/`. **No follow-up commits** in the last 30 days against these files. The protocol code is brand-new and unverified at the wire level beyond Rust unit tests embedded in `ui_protocol.rs`.

Adjacent SSE/session-actor churn in the same window:

- `6816578b`, `d2878c80` (#637) — bind thread_id BEFORE first SSE emission (#636).
- `9342f7e1`, `ba9cd83e` (#635) — sticky thread_id map.
- `7e8ed03d`, `ff8e9557` (#629) — thread_id on every SSE event + `committed_seq` on done.
- `d2b54082`, `b3aca9a7` (#628) — thread_id persistence.
- `63d42748` (#627 PR #5) — delete `session_result` primary-turn emissions.
- `9687fa95` — include `tool_call_id` in SSE `tool_progress`.
- `7d973de9` — skip worktree check when no transcript loaded (M8.6 patch).

Six SSE protocol fixes and one M8.6 patch in 30 days. All on the SSE side, not the M9 WS side.

## Suggested triage before "M9 is stable" claim

1. **Block the claim** until at least one e2e spec drives the M9 WS endpoint end-to-end. Minimum: `session/open` → `turn/start` → assert `turn/started` → `message/delta`*N → `turn/completed`. Without this, "M9 stability" is a theoretical claim about a code path no test ever traverses.
2. **Add fault-injection specs** for the six gaps in § "Fault-injection coverage" — each is one of the standard wire-protocol robustness invariants and they're trivially scriptable against a mocked WS or a live server with a kill-switch.
3. **Close #632 + #636** (SSE thread_id race, 3 events/turn still leak). Marked blockers because the M9 ledger inherits the same binding code path.
4. **Close #634** (`done` fires before JSONL commit). M9's `turn/completed` should be a hard write-then-emit barrier; today its SSE precursor isn't.
5. **Close #618** (tool_progress not surfacing). M9 `tool/started`/`tool/completed` will fail to emit for the same reason.
6. **Land at least #536, #541, and #543** from the M8 fix-first checklist — these are the items the M9 spec § 12 names explicitly as gates on protocol features that depend on runtime truth.
7. **Promote `live-tool-retry-collapse` from `test.skip()` to a deterministic synthetic** (the issue #636 suggestion of `OCTOS_FORCE_TOOL_RETRY=1` against a test-mode tool). Today the spec passes by skipping when the LLM is fast — that's not a passing test, it's a no-op on Anthropic.
8. **Fix the M9 Rust crate's missing integration tests**: nothing in `crates/octos-cli/tests/` exercises `ui_protocol.rs`. Add a `tests/ui_protocol_wire.rs` that opens a real `axum::test_server`, drives the WS, and asserts on the documented method set.
