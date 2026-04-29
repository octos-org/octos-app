# M9-FIX-08 — `turn/interrupt` drains pending approvals

| | |
|---|---|
| Severity | Serious |
| Wave | 3 |
| Files | `crates/octos-cli/src/api/ui_protocol.rs`, `crates/octos-cli/src/api/ui_protocol_approvals.rs` |
| Branch | `fix/m9-08-interrupt-drains-approvals` |
| Worktree | `~/home/octos-m9-fix-08` |
| Estimated | 1 dev-day |
| Conflicts | Wave 3 — touches same files as M9-FIX-04 / M9-FIX-07; rebases on both |

## Problem

From `m9-review/03-approvals-diff.md` finding #5:

> Race on `turn/interrupt`: aborts the task but does not drain pending approvals. Late
> `respond` succeeds with `accepted: true, runtime_resumed: false` against a turn that no
> longer exists — the client thinks the command ran.

Sequence:

1. Turn T1 emits `approval/requested A1`.
2. User goes off to think.
3. Different process / user calls `turn/interrupt T1`.
4. T1 aborts; A1 is now an orphan (the runtime that would have used the decision is gone).
5. User clicks Approve on A1.
6. Server returns `{accepted: true, runtime_resumed: false}`.
7. Client UI shows "approved" — but nothing actually ran.

This violates the user's mental model. Either the approval should be cancelled (with
visible client-side state change) or the response must clearly say "your approval is
moot, the turn is gone."

## Acceptance criteria

1. **On `turn/interrupt`**: enumerate pending approvals tied to the interrupted turn; mark each as `Cancelled` and emit `approval/cancelled { approval_id, reason: "turn_interrupted" }`.

2. **Late `respond` against a cancelled approval**: returns `RpcError::approval_cancelled { approval_id, reason }` (uses the M9-FIX-02 codes — define `APPROVAL_CANCELLED = -32012`).

3. **Pending approvals across reconnect**: replayed approvals include the cancelled state (`approval/cancelled` is a durable notification, ledgered).

4. **`approval/cancelled` is a new durable notification**: register in M9-FIX-01-style. Coordinate with M9-FIX-01.

5. **Tests**:
   - `interrupt_cancels_pending_approvals_for_turn`
   - `respond_to_cancelled_approval_returns_typed_error`
   - `cancelled_approval_replays_on_reconnect`
   - `approve_for_session_scope_survives_interrupt` (per M9-FIX-06 — session scopes are turn-independent)

## Files & lines

- `crates/octos-cli/src/api/ui_protocol.rs:1084–1129` — `handle_turn_interrupt` (extended).
- `crates/octos-cli/src/api/ui_protocol_approvals.rs:198–215` — pending tracking; add cancel API.
- `crates/octos-core/src/ui_protocol.rs:333+` — register `APPROVAL_CANCELLED` method.
- `crates/octos-core/src/ui_protocol.rs:1577+` — `ApprovalCancelled` variant.

## Notes

- Coordinate with M9-FIX-03: the interrupt handler refactor lands first; this builds on top.
- Coordinate with M9-FIX-06: session-scoped approvals (e.g., `approve_for_session`) survive
  interrupt — only `approve_for_turn` and the per-call pending entries get cancelled.
- The `approval/cancelled` notification carries enough context for clients to update UI
  without state machinery (`approval_id`, `reason`, `turn_id`).

## Tests

```rust
#[tokio::test]
async fn interrupt_cancels_pending_approvals_for_turn() {
    /* turn T emits approval A; interrupt T; verify approval/cancelled is emitted
       with reason "turn_interrupted" and approval state is Cancelled */
}

#[tokio::test]
async fn respond_to_cancelled_approval_returns_typed_error() {
    /* cancel approval A via interrupt; respond(A, approve);
       expect RpcError code APPROVAL_CANCELLED */
}
```

## Out of scope

- Auto-restart of cancelled approvals on the next turn (would be a separate UX feature).
- Cancellation reasons other than `turn_interrupted` (e.g., `session_closed`, `tool_unavailable`).
  Add as needed in followups.

## Implementer briefing

1. `cd ~/home/octos-m9-fix-08`. Wait for M9-FIX-03 + M9-FIX-06 + M9-FIX-07 to merge.
2. Extend `handle_turn_interrupt` to call `cancel_pending_for_turn(turn_id)` on the approvals slice.
3. Implement `cancel_pending_for_turn` in `ui_protocol_approvals.rs`. Atomic: take lock, transition all matching pending entries to Cancelled, emit notifications, release.
4. Add `APPROVAL_CANCELLED` error code.
5. Update `respond` to detect Cancelled state and return the typed error.
6. Tests per the list.

Constraints:
- ≤ 300 LOC new.
- Compose cleanly with M9-FIX-06's auto-resolve path: a cancelled scope entry doesn't auto-resolve future requests.

When done: status note demonstrating the cancel flow + a stress test of 100 concurrent interrupts.
