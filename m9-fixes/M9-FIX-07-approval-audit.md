# M9-FIX-07 — Approval audit trail + `approval/decided` event

| | |
|---|---|
| Severity | Serious |
| Wave | 3 |
| Files | `crates/octos-cli/src/api/ui_protocol_approvals.rs`, `octos-core/src/ui_protocol.rs` |
| Branch | `fix/m9-07-approval-audit` |
| Worktree | `~/home/octos-m9-fix-07` |
| Estimated | 1.5 dev-days |
| Conflicts | Wave 3 with M9-FIX-04 (same `ui_protocol.rs`, different concerns) |

## Problem

From `m9-review/03-approvals-diff.md` findings #2 and #4:

> No audit trail. No `tracing`, no ledger entry, no `approval/decided` event. Decisions
> live only in RAM.
>
> Reconnect replays request but loses decision. `pending_for_session` filters out
> `Responded` entries; no `approval/decided` event exists in the protocol.

Two coupled issues:

1. **No record** of who decided what, when. Compliance + operator-trust gap.
2. **Reconnect-after-decision** can't recover the decision: the ledger replays the
   `approval/requested` (durable) but never had a `approval/decided` (didn't exist).
   The new client sees a pending approval that was actually decided minutes ago.

## Acceptance criteria

1. **New durable notification: `approval/decided`**
   - Method name registered.
   - Variant on `UiNotification`.
   - Payload: `{approval_id, turn_id, session_id, decision, scope, decided_at, decided_by, auto_resolved: bool, policy_id: Option<String>, client_note: Option<String>}`.
   - Emitted on every decision (manual or auto).
   - Durable — appears in the ledger and replays on reconnect.

2. **Tracing**: every decision logs at INFO via `tracing::info!`:
   ```
   target = "octos.approvals.decision"
   approval_id, decision, scope, auto_resolved, policy_id, decided_by, decided_at, turn_id, session_id, tool_name
   ```

3. **Audit log** (separate from tracing — for compliance):
   - Append-only file `<data_dir>/audit/approvals-<epoch>.log` (JSON-Lines).
   - Each line is a structured record per decision.
   - Rotation: by size (10 MB) or time (daily). Keep last 90 days by default.
   - Configurable via `ApprovalsAuditConfig` in profile config.

4. **Reconnect carries decision**:
   - `pending_for_session` continues to return `pending` only (unchanged), BUT the ledger replay also delivers historical `approval/decided` events.
   - Client receives both `approval/requested` AND a matching `approval/decided` for any decided approval; client renders as Decided.

5. **Tests**:
   - `decision_emits_approval_decided_durable_notification`
   - `auto_resolved_emits_approval_decided_with_auto_resolved_true`
   - `audit_log_records_every_decision`
   - `reconnect_after_decision_replays_decided_event`
   - `audit_log_rotates_on_size_threshold`

## Files & lines

- `crates/octos-core/src/ui_protocol.rs:333+` — register `APPROVAL_DECIDED` method name.
- `crates/octos-core/src/ui_protocol.rs:1577+` — add `ApprovalDecided` variant to `UiNotification`.
- `crates/octos-core/src/ui_protocol.rs:1480+` — add `ApprovalDecidedEvent` struct.
- `crates/octos-cli/src/api/ui_protocol_approvals.rs:38–77` — emit `approval/decided` on respond.
- New file: `crates/octos-cli/src/api/ui_protocol_audit.rs` — audit log writer.

## Notes

- This depends on M9-FIX-01 (UiNotification structure) and M9-FIX-06 (auto_resolved path).
  Wave 3 ordering is correct.
- The audit log is a security/compliance feature; do not log payload bodies that contain
  user content (file diffs, command bodies). Log identifiers and decision metadata only.
- `decided_by` should carry the user identity if known (from auth context); empty string for system.

## Tests

```rust
#[tokio::test]
async fn reconnect_after_decision_replays_decided_event() {
    /* (1) emit approval/requested with cursor C1.
       (2) call respond with decision=approve, cursor C2.
       (3) client disconnects, reconnects with after: cursor before C1.
       (4) verify replay yields BOTH approval/requested AND approval/decided. */
}
```

## Out of scope

- Audit log read API (the file is for compliance / forensics, not real-time).
- Ledger-of-ledgers (cross-session audit aggregation).
- PII redaction policy. Decisions are metadata; payloads were already redacted out.

## Implementer briefing

1. `cd ~/home/octos-m9-fix-07`. Wait for M9-FIX-01 + M9-FIX-06 to merge.
2. Add the new event type + method name. Coordinate with M9-FIX-01's pattern.
3. Wire emission from `respond`. Always emit, regardless of manual vs auto resolution.
4. Implement the audit log writer (with rotation).
5. Add tracing macro at INFO level.
6. Tests. Run with deterministic clock for the rotation test.
7. Update spec § "Approval / diff preview" with the new event.

Constraints:
- ≤ 500 LOC new.
- No payload bodies in tracing or audit log.
- `cargo fmt` + `cargo clippy` clean.

When done: status note showing a sample audit log entry and a sample reconnect replay trace.
