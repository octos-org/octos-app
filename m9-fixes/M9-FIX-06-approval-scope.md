# M9-FIX-06 — `approval_scope` enforcement

| | |
|---|---|
| Severity | **Blocker** |
| Wave | 2 |
| Files | `crates/octos-cli/src/api/ui_protocol_approvals.rs`, light touches to `ui_protocol.rs` |
| Branch | `fix/m9-06-approval-scope` |
| Worktree | `~/home/octos-m9-fix-06` |
| Estimated | 1.5 dev-days |
| Conflicts | None within Wave 2 — file is independent |

## Problem

From `m9-review/03-approvals-diff.md` finding #1:

> `approval_scope` is dropped on the floor. `ApprovalRespondParams` carries the field
> (`octos-core/src/ui_protocol.rs:572–580`) but `respond` (`ui_protocol_approvals.rs:38–77`)
> never reads it. No policy table, no future-call gating. Capability-advertised, silently
> no-op.

This is a **security/contract lie**. The protocol exposes a scope mechanism — `approve_once`,
`approve_for_session`, `approve_for_turn`, `approve_for_tool`, etc. — and clients (including
`octos-app`'s W05 typed approval card) surface it as a UI control. The server records
the user's choice but never gates future calls based on it.

Consequence: a user clicks "approve for this session" expecting future invocations within
the session to skip approval; instead, every invocation re-prompts. Or worse: the server
auto-approves silently without the user's intent.

## Acceptance criteria

1. **Scope enforcement matrix**:

   | Scope | Server behavior on subsequent matching call |
   |---|---|
   | `approve_once` (default) | re-prompt every time |
   | `approve_for_turn` | auto-approve within the same `turn_id`, re-prompt on next turn |
   | `approve_for_session` | auto-approve within the same `session_id` until session/close |
   | `approve_for_tool` | auto-approve every call to the same `tool_name` until session/close |
   | `approve_for_command_pattern` | (if supported by spec) auto-approve calls whose typed_details command matches the previously-approved pattern |
   | `deny_*` analogs | symmetric — auto-deny |

2. **Policy table**: a per-session map keyed by tuple `(scope_kind, match_key)` where
   `match_key` is the turn_id / session_id / tool_name / command_pattern. Lookups before
   emitting `approval/requested`.

3. **Eviction**: scopes evict on session close; `approve_for_turn` evicts on
   `turn/completed | turn/error`.

4. **`approval/requested` is suppressed** if a matching policy entry resolves the decision.
   Server emits `approval/auto_resolved { approval_id, scope, decision, scope_match }` so
   clients can show the auto-approval transparently. (This requires a new notification —
   coordinate with M9-FIX-01 / M9-FIX-07.)

5. **API for clients to query active scopes**: `approval/scopes/list { session_id }` →
   `{scopes: [...]}`. Useful for debugging + a "review active permissions" UI later.

6. **Tests**:
   - `scope_approve_for_turn_auto_resolves_within_turn`
   - `scope_approve_for_turn_re_prompts_on_next_turn`
   - `scope_approve_for_session_persists_until_session_close`
   - `scope_approve_for_tool_auto_resolves_same_tool`
   - `scope_approve_for_tool_does_not_match_different_tool`
   - `scope_evicts_on_session_close`
   - `unknown_scope_string_falls_back_to_approve_once`

## Files & lines

- `crates/octos-core/src/ui_protocol.rs:572–580` — `ApprovalRespondParams.approval_scope` (already typed; used here).
- `crates/octos-cli/src/api/ui_protocol_approvals.rs:38–77` — `respond` function (currently ignores scope).
- `crates/octos-cli/src/api/ui_protocol_approvals.rs:198–215` — pending tracking.
- New file: `crates/octos-cli/src/api/ui_protocol_scope.rs` (300-400 LOC) — the policy table and lookup helpers.

## Notes

- Coordinate with M9-FIX-01: the new `approval/auto_resolved` notification needs a
  `UiNotification` variant. Either land it as part of this fix or wait for M9-FIX-01
  to merge first; rebase.
- Coordinate with M9-FIX-07 (audit trail): every auto-resolution gets the same audit
  entry as a manual decision, with `auto_resolved: true` and `policy_id` set.
- "Open registry" rule: `ApprovalScope` should be an open string with documented
  constants (per the type review's open-registry recommendation). Unknown scope strings
  fall back to `approve_once`.

## Tests

```rust
#[tokio::test]
async fn scope_approve_for_turn_auto_resolves_within_turn() {
    /* request approval, respond with scope=approve_for_turn,
       trigger second matching request in same turn,
       observe approval/auto_resolved (no approval/requested) */
}

#[tokio::test]
async fn scope_evicts_on_session_close() {
    /* set scope, close session, reopen,
       verify request emits approval/requested again (scope lost) */
}
```

## Out of scope

- UI for "review/revoke active scopes" (clients build this).
- Cross-session scopes (deferred — privacy implications).
- Server-side scope policy from manifest (e.g., admin sets default scope per tool).
  Future workstream.

## Implementer briefing

1. `cd ~/home/octos-m9-fix-06`. Wait for Wave 1 (M9-FIX-01) to land.
2. Read `ui_protocol_approvals.rs` end-to-end. Diagram pending → respond → auto_resolve flow.
3. Create `ui_protocol_scope.rs` with the policy table. Use `HashMap<(ApprovalScopeKind, MatchKey), ScopeEntry>`.
4. Modify `respond` to insert a scope entry on success (when `approval_scope != approve_once`).
5. Add a pre-emit check in the approval-request emission path: lookup policy → if hit, emit `approval/auto_resolved` instead of `approval/requested`.
6. Eviction hooks: subscribe to session/close and turn/completed events.
7. Tests per the list. Run with `--test-threads=1` for determinism.
8. Update spec § "Approval / diff preview" to document scope semantics.

Constraints:
- ≤ 600 LOC new (mostly the new scope module).
- Backward compat: clients that don't send `approval_scope` keep working.
- `cargo fmt` + `cargo clippy` clean.

When done: status note showing the matrix + a short demo trace.
