# M9-FIX-09 — Wire-level e2e harness + fault injection

| | |
|---|---|
| Severity | **Blocker** |
| Wave | 1 |
| Files | `e2e/tests/m9-protocol-*.spec.ts` (new) + `e2e/tmux/run.sh` extensions |
| Branch | `fix/m9-09-wire-e2e-harness` |
| Worktree | `~/home/octos-m9-fix-09` |
| Estimated | 2 dev-days |
| Conflicts | None — entirely new files |

## Problem

From `m9-review/04-tests-and-issues.md`:

> 0% wire coverage. None of the 8 specs labeled "M9-era" actually exercise WS protocol
> methods — they hit `/api/chat` SSE and assert against the rendered DOM. `session/open`,
> `turn/start`, `turn/interrupt`, `approval/respond`, `diff/preview/get`, `task/output/read`
> — all untested at the wire.
>
> Zero fault injection.

The "M9 is stable" claim cannot be defended on evidence until at least one e2e exercises
the actual UI Protocol v1 wire end-to-end against a live Octos server.

`octos-app`'s `live_smoke.rs` (running today) is one such test on the client side; we
want the Octos repo to have its own coverage and run it in CI.

## Acceptance criteria

A new test family at `e2e/tests/m9-protocol-*.spec.ts` covering:

### Method coverage (one spec per method)

1. `m9-protocol-session-open.spec.ts` — open / resume / close.
2. `m9-protocol-turn-start.spec.ts` — happy path turn lifecycle (start → completed).
3. `m9-protocol-turn-interrupt.spec.ts` — interrupt in-flight, interrupt completed (idempotent), unknown turn.
4. `m9-protocol-approval-respond.spec.ts` — approve, deny, scope, double-respond, idempotent (`-32011 APPROVAL_NOT_PENDING`).
5. `m9-protocol-diff-preview.spec.ts` — preview generation, file size limits, missing preview_id.
6. `m9-protocol-task-output-read.spec.ts` — initial read, follow-up tail with cursor.
7. `m9-protocol-tool-events.spec.ts` — tool/started, tool/progress, tool/completed correlation.
8. `m9-protocol-progress-updated.spec.ts` — depends on M9-FIX-01.

### Fault injection (one spec)

`m9-protocol-fault-injection.spec.ts`:

- **Drop mid-turn**: kill the WebSocket mid-stream; reconnect with cursor; verify replay restores state, no duplicates.
- **Stale cursor**: send `session/open { after: <ancient cursor> }`; verify `CURSOR_OUT_OF_RANGE` typed error.
- **Future cursor**: send a cursor with seq beyond head; same check.
- **Double `turn/start`**: same session/turn_id submitted twice; verify second returns the same accepted result (idempotent) or typed error.
- **Double `approval/respond`**: depends on M9-FIX-02 (`-32011`).
- **Double `turn/interrupt`**: depends on M9-FIX-03.
- **Slow client**: pause read; verify `protocol/replay_lossy` (depends on M9-FIX-04).

### CI integration

- Add a CI job (or extend existing) that boots a dedicated `octos serve` instance, runs
  the M9 protocol suite, and tears down. Block PRs touching `crates/octos-core/src/ui_protocol.rs`
  or `crates/octos-cli/src/api/ui_protocol*.rs` on this job.
- Per-spec timeout: 60s. Suite total: 5 min.
- Emit JSON summary for the `compare-tui-coding-ux-tmux.sh`-style operator scripts.

### Test infrastructure

- A small TypeScript helper `e2e/lib/m9-ws-client.ts` that wraps the raw `ws` library
  with typed JSON-RPC envelope + cursor management. Reusable by every spec.
- Use `playwright` if existing specs use it; else `vitest` + `ws`. Match the repo's
  current convention.
- Each spec asserts wire-level (envelope shape, error codes, cursor monotonicity) — NOT
  rendered DOM.

## Files & lines

- New: `e2e/tests/m9-protocol-*.spec.ts` (8 method specs + 1 fault-injection spec).
- New: `e2e/lib/m9-ws-client.ts` (~300 LOC).
- Modify: `e2e/tmux/run.sh` to add a target for the new suite.
- Modify: `.github/workflows/ci.yml` (or wherever Octos CI lives) to add the M9 protocol job.

## Notes

- This workstream is about *infrastructure*. Specs that depend on M9-FIX-NN behavior
  (like `-32011`) gate on those workstreams. Wave 1 lands the harness; Waves 2-3-4 add
  spec coverage for their respective behaviors.
- The harness is independent of the M9 fixes themselves — it can run on the current
  branch and immediately surface gaps.

## Out of scope

- Visual / DOM assertions (those are octos-web concern).
- Mocking the server (we want live coverage, not mocked).
- Performance / latency benchmarks (separate workstream).

## Implementer briefing

1. `cd ~/home/octos-m9-fix-09`. No dependencies — start immediately in Wave 1.
2. Read existing e2e specs at `e2e/tests/` to understand the harness convention. If they're Playwright specs hitting a browser, that's the wrong shape — we need a dedicated WS-protocol harness.
3. Write `m9-ws-client.ts` first. Sketch:
   ```typescript
   class M9WsClient {
     async openSession(opts: SessionOpenParams): Promise<SessionOpenResult>;
     async startTurn(opts: TurnStartParams): Promise<TurnStartResult>;
     async interruptTurn(opts: TurnInterruptParams): Promise<TurnInterruptResult>;
     async sendApprovalResponse(opts: ApprovalRespondParams): Promise<ApprovalRespondResult>;
     async getDiffPreview(opts: DiffPreviewGetParams): Promise<DiffPreview>;
     // ...
     onNotification(handler: (n: UiNotification) => void): void;
     getCurrentCursor(): UiCursor | undefined;
   }
   ```
4. Write the 8 method specs + 1 fault-injection spec.
5. Add a CI job. If running `octos serve` requires keys (DEEPSEEK_API_KEY etc.), document the secret list and use repo secrets.
6. Verify locally: boot a `octos serve`, run the suite, all green.

Constraints:
- ≤ 1500 LOC across all new files.
- Tests must be reproducible (deterministic seeds, no timing-dependent asserts).
- Each spec is independently runnable: `npx playwright test e2e/tests/m9-protocol-session-open.spec.ts`.
- CI total time budget: 5 minutes.

When done: status note showing a green CI run output + the per-spec pass rate.
