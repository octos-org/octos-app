# M9-FIX-10 — Risk + path sanitization

| | |
|---|---|
| Severity | Minor (security-adjacent) |
| Wave | 2 |
| Files | `crates/octos-cli/src/api/ui_protocol.rs` (specifically `materialize_file_mutation_diff` + risk emission) |
| Branch | `fix/m9-10-risk-path-sanitization` |
| Worktree | `~/home/octos-m9-fix-10` |
| Estimated | 1 dev-day |
| Conflicts | Wave 2 with M9-FIX-03 (same file, different functions) |

## Problem

From `m9-review/03-approvals-diff.md` findings #3 + #6:

> Risk hardcoded to `"medium"` at `ui_protocol.rs:243` for every shell command. Malicious
> tool can also push its own `ApprovalRequestedEvent` via the progress side-effect path
> (`ui_protocol.rs:1452–1455`) and the server trusts upstream risk verbatim.
>
> Diff preview / file-mutation path: `materialize_file_mutation_diff`
> (`ui_protocol.rs:1479–1504`) builds the diff from current FS at progress-event time
> (TOCTOU between proposal and apply), and `notice.path` is fed to display strings without
> sanitization (path-spoof).

Two concrete hardening items:

1. **Risk** is stamped `"medium"` for every shell command, regardless of what the command actually does. UI's risk badge is meaningless.
2. **Tool-emitted risk** flows through to clients without server-side validation. A tool can claim its own delete-rm-rf-slash is `"low"`.
3. **`notice.path`** in approval display strings can contain `../` traversal or unicode trickery to spoof a file path.

## Acceptance criteria

1. **Risk derivation from manifest**: tools declare risk in their manifest; server reads from manifest, not from tool-emitted payload. Default if absent: `"unspecified"` (NOT `"low"` — explicit "we don't know").
2. **Tool-emitted risk override is ignored** (or logged at WARN if present, then overwritten with manifest value). Comment in code explaining why.
3. **Path sanitization**: a `sanitize_display_path` helper applied to every path-shaped string before it's embedded in approval `title`/`body`/`typed_details.command_preview`. Removes:
   - `../` traversal sequences (canonicalize to display the actual resolved path or reject if escapes the workspace).
   - Unicode RTL override and other ambiguity chars.
   - Multi-byte zero-width chars.
4. **`materialize_file_mutation_diff` TOCTOU mitigation**: bind a snapshot of the file at proposal time, not at preview-fetch time. The proposal-to-apply window cannot read from FS twice and conflict.
5. **Tests**:
   - `tool_emitted_risk_is_ignored_in_favor_of_manifest`
   - `risk_default_is_unspecified_when_manifest_silent`
   - `sanitize_display_path_strips_traversal`
   - `sanitize_display_path_strips_rtl_override`
   - `materialize_file_mutation_diff_uses_snapshot_at_proposal_time`

## Files & lines

- `crates/octos-cli/src/api/ui_protocol.rs:243` — risk hardcoded.
- `crates/octos-cli/src/api/ui_protocol.rs:1452–1455` — tool-emitted approval emission path.
- `crates/octos-cli/src/api/ui_protocol.rs:1479–1504` — `materialize_file_mutation_diff`.
- New: `crates/octos-cli/src/api/ui_protocol_sanitize.rs` (~150 LOC).

## Notes

- "Manifest-driven risk" requires reading from `ToolDefinition.risk` or equivalent.
  If the manifest doesn't have a risk field today, add it as part of this fix (small
  change to `octos-core::ToolDefinition`).
- "Snapshot at proposal time" implies storing the FS state in the approval entry. Use
  the `PendingDiffPreviewStore` (mentioned in finding #7 — it's unbounded; that's a
  separate cleanup item but not blocking).

## Tests

```rust
#[test]
fn sanitize_display_path_strips_traversal() {
    assert_eq!(sanitize_display_path("../../etc/passwd"), "etc/passwd"); // canonicalized
}

#[test]
fn risk_default_is_unspecified_when_manifest_silent() {
    let tool = ToolDefinition { risk: None, ..Default::default() };
    assert_eq!(server_risk_for(&tool), RiskLevel::Unspecified);
}

#[tokio::test]
async fn materialize_file_mutation_diff_uses_snapshot_at_proposal_time() {
    /* propose at t1, modify file on disk at t2, fetch preview at t3,
       assert preview shows the t1 content, not t2 */
}
```

## Out of scope

- Manifest schema overhaul. Just add an optional `risk` field.
- Tool sandbox enforcement based on risk (would block destructive ops). Separate workstream.
- Diff size limits / streaming. Tracked under M9-FIX-05 if needed.

## Implementer briefing

1. `cd ~/home/octos-m9-fix-10`. Coordinate with M9-FIX-03 (same file, different functions).
2. Add `ToolDefinition.risk: Option<RiskLevel>` to `octos-core` if absent.
3. Implement `sanitize_display_path` in `ui_protocol_sanitize.rs`.
4. Patch the risk emission site to read manifest → fall back to `Unspecified`.
5. Patch the file-mutation diff path to bind a snapshot at proposal time.
6. Tests per the list.

Constraints:
- ≤ 300 LOC new.
- The `Unspecified` default may surface in client UIs as a visible badge (intentional).
- `cargo fmt` + `cargo clippy --workspace -- -D warnings` clean.

When done: status note demonstrating the path-sanitize examples + a snapshot-vs-current-FS race repro.
