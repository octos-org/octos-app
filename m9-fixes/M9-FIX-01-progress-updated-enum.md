# M9-FIX-01 — `progress/updated` notification decode + serde fallbacks

| | |
|---|---|
| Severity | **Blocker** |
| Wave | 1 |
| Files | `crates/octos-core/src/ui_protocol.rs` |
| Branch | `fix/m9-01-progress-updated-enum` |
| Worktree | `~/home/octos-m9-fix-01` |
| Estimated | 0.5 dev-day |
| Conflicts | Wave 1 light overlap with M9-FIX-02 (same file, different sections) |

## Problem

From `m9-review/01-protocol-types.md` finding #1:

> `UiNotification` cannot decode `progress/updated`. The method name is in the registry
> at `ui_protocol.rs:333` but the enum at `ui_protocol.rs:1577–1590` is missing the
> `ProgressUpdated` variant; `from_method_and_params` returns `method_not_found` when a
> server emits it. **Every typed-Rust client silently drops these notifications.**

UPCR-2026-002 lists the rich progress schema (token/cost counters, retry/backoff,
file mutation metadata) as a first-class notification stream. The spec at
`OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md` § 7 declares it. The registry constant
`PROGRESS_UPDATED` exists. The enum doesn't.

Adjacent finding from review #4 (closed wire enums):

> Multiple closed wire enums (`ApprovalDecision`, `ApprovalRespondStatus`,
> `DiffPreviewFileStatus`, `InputItem`, etc.) lack `#[serde(other)]` despite spec line
> 75 forbidding the assumption that "unknown variants are impossible forever."

The two are bundled because both are pure-types serde fixes in the same file.

## Acceptance criteria

1. `UiNotification` enum in `ui_protocol.rs` includes a `ProgressUpdated(ProgressUpdatedEvent)` variant that round-trips JSON-RPC `method: "progress/updated"`.
2. `ProgressUpdatedEvent` struct mirrors the spec's rich progress schema: at minimum
   `session_id`, `turn_id`, plus a `payload: serde_json::Value` for forward-compat
   (or typed nested structs if the spec lists them).
3. `from_method_and_params` and `to_method_and_params` (or whatever helpers exist) handle the new variant.
4. **Forward-compat.** Add `#[serde(other)]` (or a string-based fallback variant) to:
   - `ApprovalDecision`
   - `ApprovalRespondStatus`
   - `DiffPreviewFileStatus`
   - `InputItem` (if it's a closed enum)
   - `RiskLevel`
   - Any other closed `#[derive(Deserialize)] enum` exposed on the wire — grep for
     `#[derive(.*Deserialize.*)]` then check each enum.
5. **Tests.** Existing golden tests at the bottom of `ui_protocol.rs` cover the new variants. Specifically:
   - Round-trip a `ProgressUpdated` notification with a representative payload.
   - Decode an `ApprovalDecision` value of `"future_decision_kind"` → falls through to the new fallback variant, not Err.

## Files & lines (citations from the review)

- `crates/octos-core/src/ui_protocol.rs:333` — `PROGRESS_UPDATED` method name registered.
- `crates/octos-core/src/ui_protocol.rs:1577–1590` — `UiNotification` enum (variant missing here).
- `crates/octos-core/src/ui_protocol.rs:~1604` — `from_method_and_params` (search for the match arm; add the new arm).
- Closed enums to audit: search `\benum\b.*\{` and the `Deserialize` derives.

## Tests to add

In the file's `#[cfg(test)] mod` block:

```rust
#[test]
fn progress_updated_round_trip_minimal() { /* emit + decode with payload = {} */ }

#[test]
fn progress_updated_round_trip_with_typed_fields() { /* token/cost/retry shape */ }

#[test]
fn approval_decision_unknown_falls_through() { /* "future_kind" → Unknown variant */ }

#[test]
fn closed_enums_have_serde_other_fallback() {
    for value in &["future_status", "v2_kind"] { /* every audited enum */ }
}
```

## Out of scope

- Server-side emission of `progress/updated`. That's M9.5 and not covered here.
- Octos-app client rendering of progress events. Followup; client today buffers them silently.
- Removing the `closed` shapes for non-wire enums (e.g., internal-only types).

## Implementer briefing for the swarm

You're a Rust protocol-types reviewer + author. The brief:

1. `cd ~/home/octos-m9-fix-01` (worktree must exist; supervisor created it).
2. Confirm you're on branch `fix/m9-01-progress-updated-enum` with `git status`.
3. Open `crates/octos-core/src/ui_protocol.rs`. Read the `UiNotification` enum (line 1577-ish) AND the existing helper functions for method-name conversion. Read the spec at `~/home/octos/api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md` § 7 for the `progress/updated` payload shape.
4. Add the `ProgressUpdated` variant and its event struct. Wire all three helpers (parse, emit, method-name). Update the `for_each_notification_method` test or its equivalent.
5. Audit closed enums for `#[serde(other)]`. Pattern: any `enum` with `#[derive(... Deserialize ...)]` and concrete variant names should have a fallback variant. Use `Unknown(String)` if you need to capture the raw string (preferred) or `#[serde(other)] Unknown` if you don't.
6. Add tests under the existing `mod tests` at the bottom of the file. At least 4 new tests per the spec above.
7. Run: `cargo test -p octos-core --lib` — must pass.
8. Run: `cargo check -p octos-cli` — must pass (downstream references mustn't break).
9. Commit each logical step (variant add, then enum fallbacks, then tests). Conventional commit messages: `fix(m9): wire progress/updated through UiNotification` etc.
10. Append a one-paragraph status to `~/home/octos-app/m9-fixes/INDEX.md` "What this index tracks" table. Don't open a PR; the supervisor reviews the branch.

Constraints:
- ≤ 250 LOC of new code.
- Zero behavior change to existing variants (additive only).
- Tests must be golden — exact serialize_json strings, not just `.is_ok()`.
- `cargo fmt` clean. `cargo clippy --workspace -- -D warnings` clean.

When done, write a short status note to the supervisor: variant added, fallbacks added per audit list, tests added, all green. Cite the exact line numbers added/modified.
