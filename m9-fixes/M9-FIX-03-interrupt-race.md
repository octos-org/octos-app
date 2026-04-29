# M9-FIX-03 — `turn/interrupt` TOCTOU race + spec semantics

| | |
|---|---|
| Severity | **Blocker** |
| Wave | 2 |
| Files | `crates/octos-cli/src/api/ui_protocol.rs` |
| Branch | `fix/m9-03-interrupt-race` |
| Worktree | `~/home/octos-m9-fix-03` |
| Estimated | 1.5 dev-day |
| Conflicts | Wave 2 with M9-FIX-10 (same file, different functions) |

## Problem

From `m9-review/02-server-handler.md`:

> `turn/interrupt` (`ui_protocol.rs:1091–1110`) violates the spec's "idempotent and explicit" requirement:
> - Stale `turn_id` returns `interrupted: false` — indistinguishable from "no such turn ever existed."
> - Mismatched current `turn_id` returns hard `invalid_params` — but the spec says interrupt must be idempotent against already-completed turns.
> - **TOCTOU race** between lookup (line 1093) and remove (line 1109): if the turn task completes naturally in that window, both `turn/completed` AND `turn/error` for the same `turn_id` get sent.

Spec at `OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md` § "Turn control" requires:
- Interrupt is idempotent — calling on an already-completed turn returns success without further effect.
- Exactly one terminal event per turn — `turn/completed` xor `turn/error`.

Today's implementation breaks both.

## Acceptance criteria

1. **Idempotency**:
   - Interrupt on an already-completed turn returns `{interrupted: false, terminal_state: "completed"}`.
   - Interrupt on a never-existed turn returns a typed `RpcError::unknown_turn(turn_id)` (uses M9-FIX-02 codes).
   - Interrupt on an in-flight turn returns `{interrupted: true}` after the turn task acknowledges abort.
   - Interrupt on the same in-flight turn called twice — second call returns the same response as the first (idempotent), no double-emit.
2. **No TOCTOU**:
   - Lookup + abort + state-transition is atomic with respect to natural turn completion. Implementation: take the registry lock once, transition the turn state inside the same critical section, release lock. Use `Arc<Mutex<TurnState>>` or a state-machine enum guarded by a single mutex.
   - **Exactly one terminal event per turn** — assert in tests, observe in logs.
3. **Spec semantics**:
   - Mismatched `turn_id` (e.g., interrupting turn A while turn B is active in the same session) returns `{interrupted: false, reason: "turn_id_mismatch"}` rather than `invalid_params`.
4. **Tests** (in `crates/octos-cli/src/api/ui_protocol.rs` `#[cfg(test)] mod` or new `tests/`):
   - `interrupt_idempotent_on_completed_turn`
   - `interrupt_unknown_turn_returns_unknown_turn_error`
   - `interrupt_in_flight_turn_aborts_emits_one_terminal`
   - `interrupt_then_completion_race_emits_one_terminal` (the TOCTOU repro)
   - `interrupt_called_twice_returns_same_response`

## Files & lines

- `crates/octos-cli/src/api/ui_protocol.rs:991+` — `run_standalone_turn` (turn task).
- `crates/octos-cli/src/api/ui_protocol.rs:1084–1129` — `handle_turn_interrupt`.
- `crates/octos-cli/src/api/ui_protocol.rs:297` — `SharedConnectionTurns` registry.
- `crates/octos-cli/src/api/ui_protocol.rs:~1351` — `_abort_guard` (related; see notes).

## Notes

- The `_abort_guard` (line ~1351) only aborts the agent task, not the outer turn future.
  If the outer future is dropped without going through interrupt, the registry holds a
  dead handle. This is related but tracked separately as part of M9-FIX-04 (resource
  cleanup). Do not enlarge scope here.
- Registry lock granularity: per-session is fine. Don't take a global registry lock.
- Use `oneshot::channel` for "abort acknowledged" so the handler can wait for the task
  to confirm it stopped before responding. Add a 5-second timeout; on timeout return
  `{interrupted: true, ack_timeout: true}` and let the caller decide.

## Tests to add

```rust
#[tokio::test]
async fn interrupt_idempotent_on_completed_turn() { /* … */ }

#[tokio::test]
async fn interrupt_in_flight_turn_aborts_emits_one_terminal() { /* … */ }

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn interrupt_then_completion_race_emits_one_terminal() {
    /* boot the registry, spawn a turn that completes after a tiny delay,
       call interrupt at the same moment, observe exactly one terminal */
}
```

The race test should run 100 iterations; assert at least one iteration triggers the
race window (otherwise the test is a no-op).

## Out of scope

- M9-FIX-08 (interrupt drains pending approvals) — same handler, different concern.
- Resource cleanup of registry entries on disconnect — covered by M9-FIX-04 / 05.

## Implementer briefing

1. `cd ~/home/octos-m9-fix-03`. Verify branch + worktree clean.
2. Wait for M9-FIX-02 to land first (need `RpcError::unknown_turn` + `RpcError::turn_id_mismatch`). If it hasn't yet, stub the error codes locally and rebase later.
3. Read `handle_turn_interrupt` and `run_standalone_turn` end-to-end. Diagram the state transitions: `Pending → Running → (Completed | Errored | Interrupted)`. Identify the lock boundary.
4. Refactor the turn-state into an explicit enum with a single mutex per turn.
5. Implement the abort-acknowledge protocol: handler signals abort → task observes signal at next await → task transitions state → task acks via oneshot.
6. Cover the 5 tests. Run with `--test-threads=1` for determinism, then with multiple threads to surface races.
7. Verify: `cargo test -p octos-cli --lib` clean, plus the new race test passes 100 iterations.
8. Commit per logical step.

Constraints:
- ≤ 400 LOC of new + modified code.
- No new public types in `octos-core` (the contract types stay; only the handler internals change).
- `cargo fmt` + `cargo clippy` clean.

When done: status note describing the new state machine, the lock boundary, and the race-test iteration count + observed-race-occurrences.
