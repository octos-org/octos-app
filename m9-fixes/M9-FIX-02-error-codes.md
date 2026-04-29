# M9-FIX-02 — Spec §10 error code parity

| | |
|---|---|
| Severity | **Blocker** |
| Wave | 1 |
| Files | `crates/octos-core/src/ui_protocol.rs` |
| Branch | `fix/m9-02-error-codes` |
| Worktree | `~/home/octos-m9-fix-02` |
| Estimated | 1 dev-day |
| Conflicts | Light overlap with M9-FIX-01 in same file, different sections |

## Problem

From `m9-review/01-protocol-types.md` finding #2:

> Spec §10 error taxonomy (`unknown_session`, `cursor_out_of_range`, `permission_denied`,
> `APPROVAL_NOT_PENDING -32011`, etc.) has zero matching codes in `rpc_error_codes`
> (`ui_protocol.rs:48–58`). Clients receive opaque generic codes and can't distinguish
> error classes — every error looks like `INVALID_PARAMS`.

Compounding finding from `02-server-handler.md`: `decode_result` (`ui_protocol.rs:280`)
wraps a malformed *result* in `INVALID_PARAMS` (-32602), which is the JSON-RPC code for
malformed *params*. Wrong code.

Compounding finding from `03-approvals-diff.md`: when `respond` is called against an
already-decided approval, the spec says return `-32011 APPROVAL_NOT_PENDING` with the
recorded decision in `error.data`. The constant doesn't exist; the implementation
returns a generic error message instead.

## Acceptance criteria

1. **All spec §10 error codes are constants** in a `rpc_error_codes` module (or extension thereof). Minimum set, with codes from spec where given:
   - `UNKNOWN_SESSION`
   - `UNKNOWN_TURN`
   - `UNKNOWN_APPROVAL_ID`
   - `UNKNOWN_PREVIEW_ID`
   - `UNKNOWN_TASK_ID`
   - `CURSOR_OUT_OF_RANGE` (stale or future)
   - `CURSOR_INVALID` (malformed or wrong-session)
   - `PERMISSION_DENIED`
   - `APPROVAL_NOT_PENDING` = `-32011` (spec-explicit)
   - `UNSUPPORTED_CAPABILITY`
   - `RUNTIME_NOT_READY`
   - `MALFORMED_RESULT` — for `decode_result` use, distinct from `INVALID_PARAMS`.
   - `RATE_LIMITED` — for backoff signaling (referenced in M9-FIX-04).
2. **Doc comments** on each constant cite the spec section that establishes it.
3. **Numerical range** — pick one consistent space, e.g. `-32000` to `-32099` for application-level errors, leaving the JSON-RPC reserved range (`-32700`, `-32600..-32603`) untouched. Document the partition.
4. **`decode_result`** uses `MALFORMED_RESULT` instead of `INVALID_PARAMS`.
5. **`UiRpcResult::from_method_and_result`** (`ui_protocol.rs:1025`) can reconstruct an `UnsupportedCapability` variant — it currently can't, per the type review.
6. **`RpcError`** carries a structured `data: Option<serde_json::Value>` so error-specific payload (e.g., `{"recorded_decision": "approve"}` for `APPROVAL_NOT_PENDING`) round-trips. Spec §10 implies this.
7. **Tests:** for every new code, a golden round-trip; for `APPROVAL_NOT_PENDING` specifically, the test asserts that an error body with `data.recorded_decision = "approve"` deserializes correctly and exposes the decision via a typed accessor.

## Files & lines

- `crates/octos-core/src/ui_protocol.rs:48–58` — current `rpc_error_codes` module.
- `crates/octos-core/src/ui_protocol.rs:124` — `RpcRequest.id: String` (separate fix; see notes below).
- `crates/octos-core/src/ui_protocol.rs:280` — `decode_result` mis-categorizes.
- `crates/octos-core/src/ui_protocol.rs:1025` — `UiRpcResult::from_method_and_result` missing branch.
- Spec ref: `~/home/octos/api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md` § 10 (error taxonomy).

## Notes

- `RpcRequest.id: String` violating JSON-RPC 2.0 (id can be string|number|null) is a
  related but separate fix. Out of scope for M9-FIX-02; track in a follow-up issue.
  Reason for the split: changing the id type touches every server handler and every
  client; wider blast radius than the error code work warrants.
- The `data` field on RpcError is the right venue for `recorded_decision` (per spec
  example for `-32011`). Don't invent a `RecordedDecisionResponse` envelope.

## Tests to add

```rust
#[test]
fn approval_not_pending_carries_recorded_decision() {
    let err = RpcError::approval_not_pending(ApprovalDecision::Approve);
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], -32011);
    assert_eq!(json["data"]["recorded_decision"], "approve");
}

#[test]
fn cursor_out_of_range_round_trip() { /* emit + decode */ }

#[test]
fn decode_malformed_result_returns_malformed_result_not_invalid_params() {
    /* feed bad JSON to decode_result; assert error.code == MALFORMED_RESULT */
}

#[test]
fn unsupported_capability_result_round_trips() {
    /* construct UiRpcResult::UnsupportedCapability and decode it */
}
```

## Out of scope

- Server-side handler emission of these codes. M9-FIX-03/04/06 will use them.
- Renaming or removing existing error codes. Additive-only.
- `RpcRequest.id` shape fix (separate issue).

## Implementer briefing

1. `cd ~/home/octos-m9-fix-02`. Verify branch.
2. Read `~/home/octos/api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md` § 10 carefully. List every named error in the spec. Cross-check against `ui_protocol.rs:48–58`. The diff is your work list.
3. Decide the numeric range partition (recommend `-32100..-32199` for octos-application-level; document it in a module comment).
4. Add constants with doc-comments quoting the spec line that requires each.
5. Add a `RpcError::data` field (`Option<serde_json::Value>`) if it doesn't exist; ensure it serde-survives both presence and absence.
6. Helper constructors per code: `RpcError::unknown_session(id)`, `RpcError::cursor_out_of_range(cursor, ledger_head)`, `RpcError::approval_not_pending(decision)`, etc. Compact but explicit.
7. Fix `decode_result` to use `MALFORMED_RESULT`.
8. Add the `UnsupportedCapability` decoding branch to `UiRpcResult::from_method_and_result`.
9. Add the four tests above plus one round-trip per new constant.
10. `cargo test -p octos-core --lib` and `cargo check -p octos-cli` clean.
11. Commit logically. Conventional commits.

Constraints:
- ≤ 350 LOC new.
- No deletions of existing codes (additive only).
- `cargo fmt` + `cargo clippy --workspace -- -D warnings` clean.

When done: status note listing every new code with its numeric value and the spec section it's anchored to.
