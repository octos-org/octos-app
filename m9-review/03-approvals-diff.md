# M9 Review — Approvals + Diff Preview (M9.2 / M9.3)

**Scope.** `crates/octos-cli/src/api/ui_protocol_approvals.rs` (465 lines), `crates/octos-cli/src/api/ui_protocol_diff.rs` (395 lines), and the approval/diff handlers in `crates/octos-cli/src/api/ui_protocol.rs`. Spec: `api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md`.

## Verdict

**RED.** The store-level idempotency primitive is correct and well-tested, but the surrounding system is missing required behavior on every other axis: no approval-scope enforcement (security gap), no audit log / no `approval/decided` ledger event, no reconnect replay of post-disconnect decisions, hard-coded risk, weak path sanitization, race on interrupt, and zero e2e coverage. The single-node store is sound; the protocol contract around it is not production-ready.

## Top concerns (most damaging first)

1. **[SECURITY] `approval_scope` is silently dropped.** `ApprovalRespondParams` carries `approval_scope` (`request`/`turn`/`session`) and `client_note` (`octos-core/src/ui_protocol.rs:572-580`), but `PendingApprovalStore::respond` (`ui_protocol_approvals.rs:38-77`) never reads either field. There is no policy table, no per-tool/per-session allow rule, no future-call gating. A client can send `approval_scope: "session"` and the server records nothing — the next identical tool call will re-prompt, OR worse, a downstream consumer that *thinks* scope is enforced will wave the call through. The spec calls scope "advisory in v1alpha1" (`SPEC:278`), but the current implementation does not even *log* it. This is the canonical "advertise capability, silently no-op" bug.

2. **[SECURITY] No audit trail.** No `tracing::info!`, no ledger entry, no `approval/decided` notification anywhere. `handle_approval_respond` (`ui_protocol.rs:1131-1163`) responds and returns; the decision lives only in `ApprovalEntryState::Responded` in RAM. On process restart the entry is gone — there is no record that user X approved command Y at time T. For a human-in-the-loop trust boundary this is unacceptable, and it also means concern #4 (reconnect replay of decision) cannot be fixed cheaply.

3. **[SECURITY] Risk is hard-coded to `medium`.** `approval_event_from_tool_request` at `ui_protocol.rs:243` writes `event.risk = Some("medium".to_owned())` for every shell command regardless of payload. No manifest lookup, no destructive-verb heuristic. `rm -rf /` and `echo hello` are both medium. A malicious tool emitting its own `ApprovalRequestedEvent` (path at `ui_protocol.rs:1452-1455`, where progress events get re-stuffed into the store) can advertise any risk it wants — `apply_progress_contract_side_effects` simply trusts the upstream notification. Render hint `default_decision: "deny"` partly mitigates, but `danger: Some(false)` is also hard-coded and is the more dangerous lie.

4. **Reconnect replays the *request* but loses the *decision*.** `pending_for_session` (`ui_protocol_approvals.rs:137-153`) only returns entries in `Pending` state. If client A disconnects, client B (same session) reconnects after the user decided via another path, the responded entry is filtered out — but **so is the decided fact**. The new client never learns the approval was decided; it must wait for the runtime to surface a downstream `tool/completed`. The store has the decision (`ApprovalEntryState::Responded { decision }`); the replay path simply doesn't surface it as an `approval/decided` event because no such event exists in the protocol. Test `session_open_replays_pending_approval_after_reconnect_without_cursor` (`ui_protocol.rs:2629`) only covers the still-pending case.

5. **Race: respond vs. turn-interrupt.** `handle_turn_interrupt` (`ui_protocol.rs:1084-1129`) calls `active.abort.abort()` and emits a `turn/error` with code `interrupted`, but it does **not** drain pending approvals for that turn. The `oneshot::Receiver` held by `request_approval` (`ui_protocol.rs:216`) is `await`ed inside the aborted task; tokio drops it cleanly, so `recv` returns `Err`, mapped to `ToolApprovalDecision::Deny` via `unwrap_or`. So far benign. **But** the `ApprovalEntry` stays in the store as `Pending` forever — `request_runtime` inserted it (`ui_protocol_approvals.rs:115-135`) and nothing removes it on abort. A late `approval/respond` from a slow client will succeed against a turn that no longer exists, the `oneshot::send` will fail silently (`ui_protocol_approvals.rs:62`), and `runtime_resumed: false` is returned with `accepted: true`. The client thinks the approval went through; nothing did. There is also no `approval/cancelled` event in the protocol.

6. **`materialize_file_mutation_diff` race + path-traversal smell.** `ui_protocol.rs:1479-1504` shells out to `git diff -- <relative_path>`. (a) The diff is generated **at progress-event time**, not snapshotted at proposal time, so a concurrent write between proposal and approval changes the displayed diff — the user approves diff D1 but the server applies whatever the FS says at apply time. (b) `notice.path` arrives via the tool/progress event and is fed directly to `PathBuf::from(&notice.path)`, then `is_absolute`-tested. There is no normalization or sandboxing: a notice with `path: "../../../etc/passwd"` is joined to `current_dir()` and passed to `git diff --`. Git will refuse paths outside the repo, but the *display* string in the resulting `DiffPreview.title` (`ui_protocol_diff.rs:114`) preserves the raw path. UI tooling that renders that title (or `DiffPreviewFile.path`, which is also unsanitized) is exposed to spoofing.

7. **`diff/preview/get` source-of-truth confusion.** `PendingDiffPreviewStore::get` (`ui_protocol_diff.rs:21-42`) returns the cached `DiffPreview` keyed by `preview_id`. The cache is populated **once** when `apply_progress_contract_side_effects` fires (`ui_protocol.rs:1452-1477`); after that it never refreshes. So if the file is deleted, modified, or renamed before the user approves, the preview is correct (snapshot semantics) — good. But there is **no eviction**, no LRU, no per-session cap. `entries: HashMap<PreviewId, DiffPreview>` (`ui_protocol_diff.rs:17`) grows for the life of the process. With `MAX_DIFF_PREVIEW_BYTES = 256 KB` (`ui_protocol.rs:44`) per entry, this is a slow leak, not a fast one, but a long-running daemon will accumulate.

8. **Per-line truncation but no per-file or per-preview cap.** `truncate_utf8` clips the *raw diff text* before parsing (`ui_protocol.rs:1502`), but `parse_unified_diff_preview_files` (`ui_protocol_diff.rs:138-232`) never bounds files-per-preview, hunks-per-file, or lines-per-hunk. A pathological 256 KB diff (e.g., one file, single-character lines) yields ~128k `DiffPreviewLine` structs, each serialized fully into the result. A small preview cap on top of byte-truncation would be cheap insurance.

9. **Unknown `approval_id`, wrong scope, unknown decision — uneven typing.** Unknown id returns typed `APPROVAL_NOT_FOUND` with `kind: "approval_not_found"` (`ui_protocol_approvals.rs:185-196`) — good. Cross-session probe also collapses to NOT_FOUND (`ui_protocol_approvals.rs:50-52`), which is good for non-disclosure. But: an unknown `decision` string (anything outside `approve`/`deny`) is rejected at serde-deserialize time as a generic JSON-RPC `invalid_params`, with no `kind` taxonomy and no enumeration of the valid values. An unknown `approval_scope` value is silently accepted (no validation against `approval_scopes::{REQUEST,TURN,SESSION}`) — see concern #1.

10. **Lock granularity.** `RwLock<HashMap<...>>` for both stores (`ui_protocol_approvals.rs:34`, `ui_protocol_diff.rs:17`). `respond` takes write for the duration of `oneshot::send` (`ui_protocol_approvals.rs:42-66`). `send` is non-blocking, so this is fine in practice; but the broader `pending_for_session` walks the whole map under a read lock (`ui_protocol_approvals.rs:137-153`) — O(n) per session/open, which scales poorly across sessions on a shared server. Also `expect("...store poisoned")` will crash the *whole connection-handler* if any unrelated thread panics under the lock.

## Concrete attack/race scenarios

- **A) Scope spoof.** Tool emits `shell` proposal. User clicks "Approve for session". Client sends `approval_scope: "session"`. Server records nothing about the scope. Tool fires same command again next turn. Server emits a fresh `approval/requested`. User now believes they only had to approve once and either (i) re-approves out of habit (UX failure) or (ii) the UI auto-approves based on its *own* memory (per-client trust enforcement) which is now the only enforcement layer. If a second client connects, that client never saw the scope decision and will block the call with no audit reference. **This is the most dangerous current state**: scope is split-brain between clients with no server source of truth.

- **B) Interrupt + ghost approval.** User starts long shell, mid-turn it requests approval, user clicks Interrupt. `turn/interrupt` aborts the task; `oneshot::Receiver` drops; tool's `await` resolves to Deny. **But the `ApprovalEntry` lives on as `Pending`.** User in another tab sees the approval card still active, clicks Approve. `respond` succeeds with `runtime_resumed: false` and `accepted: true`. Client UI marks "approved"; nothing happened. A user reasonably concludes their command ran.

- **C) TOCTOU on diff preview.** Proposal arrives at T0 with diff D1 (cached). Background process modifies the same file at T1 (preview cache is stale on disk but cache still holds D1). User approves at T2. Tool actually executes at T3 against current FS state, producing D3 != D1. User approved D1 visually, server applied D3. The cache *behaves* like a snapshot but the *runtime* doesn't lock the file between proposal and apply. Spec says diffs come from `diff/preview/get` (`SPEC:283-292`) — but spec doesn't say execution is bound to that snapshot; current code does not bind it.

- **D) Path-spoof in `DiffPreview.title`.** Malicious tool emits `UiFileMutationNotice { path: "/etc/passwd", operation: "write" }`. `materialize_file_mutation_diff` returns None (not in repo); fallback `file_from_mutation_notice` (`ui_protocol_diff.rs:119-126`) preserves the raw path. The approval card and diff preview show `write /etc/passwd` and an empty hunk list, which a careless user might approve. The actual tool invocation may be writing somewhere else entirely; nothing cross-checks `notice.path` against the tool's actual call args.

- **E) Approval store unbounded growth.** Long-running session, dozens of approvals; only `respond` mutates state, only `remove_pending` (gated on Pending state) removes anything. Responded entries are kept forever to support idempotent reject-on-double-respond — but there is no TTL. After 10k approvals over a multi-day session, the map is large and `pending_for_session` scans all of them on every reconnect.

## Test coverage gaps

- No e2e tests touch approvals at all (`grep approval /Users/yuechen/home/octos/e2e/tests/*.ts` returns nothing).
- Unit tests cover idempotent double-respond, cross-session probe, reconnect-replay-of-request — solid for what's there. But missing: 
  - scope enforcement (would fail because not implemented),
  - `approval/respond` after `turn/interrupt` (concern #5 / scenario B),
  - decision retained across reconnect (concern #4),
  - unknown `decision` value returns typed error,
  - unknown `approval_scope` value returns typed error,
  - diff cache eviction / cap,
  - path traversal in `notice.path`,
  - concurrent `respond` from two clients (the test at `ui_protocol_approvals.rs:340-365` is sequential),
  - `runtime_resumed: false` actually surfaced when the receiver is gone.

## Strengths

- The store-level invariant — exactly one `Responded` transition, second response gets typed `APPROVAL_NOT_PENDING` with `recorded_decision` echoed back — is correct and tested (`ui_protocol_approvals.rs:54-76`, `ui_protocol_approvals.rs:438-464`).
- Cross-session approval probe collapses to NOT_FOUND (avoids ID-existence oracle) (`ui_protocol_approvals.rs:50-52`).
- Pending-replay on reconnect deduplicates against ledger replay using `replayed_approval_ids` (`ui_protocol.rs:587-600`) — prevents duplicate cards.
- Diff parser is reasonable (handles renames, new/deleted files, line numbering) (`ui_protocol_diff.rs:138-232`).
- UTF-8-safe truncation (`ui_protocol.rs:1514-1524`) — no mid-codepoint cuts.
- `runtime_resumable` flag correctly distinguishes runtime-blocked vs. legacy stored approvals (`ui_protocol_approvals.rs:19,65,130`).
- Typed-feature gating (`approval.typed.v1`) is opt-in via header (`ui_protocol.rs:74,239`) — non-negotiated clients see only generic fields.

## Suggested fixes (narrow, actionable)

1. **Wire `approval_scope` to a `SessionPolicy` table** keyed by `(session_id, tool_name, scope_signature)`. On `respond` with scope=`session`/`turn`, insert an allow rule with TTL = session lifetime / current turn. On every new `request_approval`, check the table and short-circuit if matched. Scope=`request` continues to be one-shot. This is the minimum to stop scenario A.

2. **Add an `approval/decided` ledger event.** Append on every `respond` with `{approval_id, decision, scope, client_note, decided_at, decided_by_profile_id}`. This gives reconnect replay (concern #4), audit trail (concern #2), and a hook for downstream consumers — all in one move.

3. **On `turn/interrupt`, drain pending approvals for that turn.** In `handle_turn_interrupt` (`ui_protocol.rs:1084`), after `active.abort.abort()`, iterate `pending_for_session`, filter by `turn_id`, set `state = Responded { decision: Deny }`, drop `response_tx`, emit an `approval/cancelled` (or reuse `approval/decided` with a `cancelled` decision variant). Closes scenario B.

4. **Validate `approval_scope` against `approval_scopes::{REQUEST,TURN,SESSION}`** at deserialize time or in `respond`. Return typed `INVALID_PARAMS` with `kind: "unknown_approval_scope"` and `valid_scopes: [...]`. Same for any future decision values.

5. **Compute risk from manifest, not constants.** Move the hard-coded `"medium"` (`ui_protocol.rs:243`) into a `tool_manifest.risk()` lookup keyed by tool name + heuristic on argv (e.g., `rm -rf`, `sudo`, network reach). Stop trusting upstream notification risk verbatim in `apply_progress_contract_side_effects`.

6. **Sanitize `notice.path` before display.** Reject paths containing `..` segments after normalization, or render a "outside workspace" warning in `DiffPreview.title`. Cross-check the path against tool argv when both are available.

7. **Bound the diff preview store.** LRU with a per-session cap (e.g., 64 entries) and a global cap. Evict on `turn/completed` for that turn.

8. **Hold an FS snapshot from proposal to apply.** Either copy the file into an in-memory blob at proposal time, or take a git index snapshot. Compare at apply time and refuse if mismatch (concern #6 / scenario C).

9. **Don't `expect` on poisoned locks.** Replace with explicit `match` returning `RpcError::internal_error` so a panic in one thread doesn't crash other connections.

10. **Add e2e specs.** Minimum: double-respond, reconnect-with-decided, scope-enforcement, interrupt-with-pending-approval, malformed decision string.

## Open questions for the author

- Was the scope no-op intentional pending UPCR-2026-001 v2, or an oversight? (Spec language "advisory" is ambiguous.)
- Why is there no `approval/decided` event in the protocol? Is the assumption that `tool/completed` carries the decision implicitly?
- What's the policy for scope persistence across process restart — RAM-only forever, or eventually persisted to the durable ledger?
- For diff preview: is the current "snapshot at proposal time" intentional, and if so, where is the apply-time check that the snapshot still matches? (I see no such check.)
- Are the `risk` and `danger` defaults intended to be conservative ("worst case medium"), or should they reflect actual tool semantics? Manifest is a stronger answer.
- Is there a plan to gate `approval/respond` by the same auth identity that opened the session? Today `connection_profile_id` is validated against `session_id` profile, but there's no record of *which user* approved — only which connection.
