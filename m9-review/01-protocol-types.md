# M9 UI Protocol v1 — Type-Design Review

**File**: `/Users/yuechen/home/octos/crates/octos-core/src/ui_protocol.rs` (2,718 lines)
**Spec**: `/Users/yuechen/home/octos/api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md`
**Reviewer scope**: type-design only (versioning, serde, identity, errors, capabilities). Runtime correctness excluded.

## Verdict — amber, leaning red

The bones are sensible: typed RPC envelopes, UUIDv7 newtypes for most ids, golden literal tests, additive feature flags, two open string registries (`approval_kind`, `approval_scope`) for forward-compat at the right places. But several design decisions actively brick the v1alpha1 promise of "additive fields allowed inside one version" (spec §4) and contradict the spec's own forward-compat rule "clients must not assume unknown enum variants are impossible forever". `progress/updated` lives in the method registry but not in `UiNotification`, the spec error taxonomy is entirely absent from the code, multiple closed enums have no `#[serde(other)]`, and the diff payload has no streaming or size cap. The "M9 branch is not stable" claim is fair on the type-design layer: the contract is internally coherent but ships sharp edges that will require breaking changes within v1.

## Top concerns (most damaging first)

- **`UiNotification` cannot decode `progress/updated`.** `methods::PROGRESS_UPDATED` is in `UI_PROTOCOL_NOTIFICATION_METHODS` (`ui_protocol.rs:333`) and tested as part of the golden capability list (line 1831), but the `UiNotification` enum (lines 1577–1590) lacks a `ProgressUpdated` variant, and `UiNotification::from_method_and_params` (lines 1641–1659) returns `method_not_found` for it. Clients pattern-matching on `UiNotification` after `UiNotification::from_rpc_notification(...)` will silently drop or error on every progress event. The author's comment at line 1245 says the split is intentional ("existing clients that exhaustively match the first-wave enum do not need source changes"), but this means the canonical decoder rejects a method the capability list claims to support. Pick one: include it in the enum, or remove it from the registry.

- **Spec error taxonomy is missing from code.** Spec §10 (lines 426–435) defines `unknown_session`, `unknown_turn`, `unknown_approval`, `unknown_preview`, `unknown_task`, `cursor_out_of_range`, `runtime_unavailable`, `permission_denied`. None of these have numeric codes in `rpc_error_codes` (lines 48–58) or constructors on `RpcError` (only standard JSON-RPC plus `METHOD_NOT_SUPPORTED`). Spec also referenced `-32011 APPROVAL_NOT_PENDING` — absent. Clients have no documented numeric way to discriminate "unknown approval id" from generic invalid params. Two implementations (server, octos-tui, octos-app) cannot agree on retry semantics without a shared code map.

- **`RpcRequest.id: String` violates JSON-RPC 2.0.** Lines 124–127 and 240. JSON-RPC 2.0 mandates `id` may be string, number, or null. Restricting the type to `String` means a client/server that legally sends `"id": 42` fails decode with `invalid_request`. This is a wire-level interop bug, not a stylistic one.

- **Closed enums with no `#[serde(other)]` fallback contradict spec §4.** `ApprovalDecision` (line 565), `ApprovalRespondStatus` (lines 599–602, single variant `Accepted`), `DiffPreviewGetStatus`, `DiffPreviewSource`, `DiffPreviewFileStatus`, `DiffPreviewLineKind`, `TaskOutputReadSource`, `TaskRuntimeState`, `UiResultKind`, `InputItem`. Each adds a server-side variant → every old client breaks decode. Spec line 75 explicitly warns against this. The asymmetry is glaring: `approval_kind`/`approval_scope` chose open string registries for the same reason, then closed enums elsewhere ignore the rule.

- **`UiCommand::from_method_and_params` cannot round-trip `UnsupportedCapability`.** `UiRpcResult` includes the `UnsupportedCapability` variant (line 977), but `UiRpcResult::from_method_and_result` (lines 1025–1035) only matches the six known method names and never produces `UnsupportedCapability` from the wire — a server reply of shape `{"unsupported": {...}}` for `session/open` will fail decode against `SessionOpenResult` with `INVALID_PARAMS`. The variant is constructible but not decodable through the typed helper.

- **`decode_result` returns the wrong error code.** Line 280–286 wraps a malformed *result* in `RpcError::invalid_params`. A bad result is server-side or transport corruption, not client params, and should map to `INTERNAL_ERROR` or a transport code, not `-32602`. Confuses clients that retry on `INVALID_PARAMS`.

- **`tool_call_id` is `String`, not a newtype.** Lines 1314, 1324, 1335, 1356, 1111. Spec §5 lists it as a stable identity, peer to `turn_id`/`approval_id`. The asymmetry (UUID newtypes for some, raw strings for others) lets you accidentally pass a `tool_name` where a `tool_call_id` is expected. Same critique applies in lighter form to free-form `kind`/`status` strings on `UiArtifactPaneItem` (lines 874, 881) and `UiWorkspacePaneEntry.kind` (line 858) where the spec lists a registry but the code provides no `pub mod` constants like it does for `approval_kinds`/`approval_scopes`.

- **`DiffPreview` has no size cap and no streaming.** `DiffPreviewGetResult.preview: DiffPreview` (line 659) carries the full unified diff inline; `DiffPreviewFile.hunks: Vec<DiffPreviewHunk>` and `DiffPreviewHunk.lines: Vec<DiffPreviewLine>` are unbounded. A multi-MB diff blocks the WebSocket and starves notifications. There is no truncation flag, no `next_offset` cursor, no per-hunk-size limit. Compare with `TaskOutputReadResult` (line 729) which does carry `truncated`, `next_cursor`, `bytes_read`, `total_bytes` — the diff path skipped that pattern.

- **`UiCursor` semantics are underspecified at the type level.** `UiCursor { stream: String, seq: u64 }` (line 62) — no doc-comment explains whether `stream` is per-session, per-runtime, or global; no `Ord`/`Hash`; no constructor that ties `stream` to `SessionKey`. Spec §9 gives reconnect rules but the type cannot enforce that two cursors from different sessions are non-comparable. `seq: u64` overflow: theoretical, but with no compaction/checkpoint story documented in the doc-comment, every long-lived stream is on a slow countdown.

- **Disclaimers lost in code.** Spec line 3 says "draft spec for M9.1" and §2 says draft surfaces should be marked. Module doc at `ui_protocol.rs:1` says "Draft client/runtime protocol types for M9." but no individual struct (e.g. `UiPaneSnapshot`, `ApprovalTypedDetails`, `TaskOutputReadResult`) carries a draft/non-authoritative doc-comment. A consumer reading rustdoc cannot distinguish stable from in-flight surfaces.

## Strengths

- **Centralized method-name constants** (`pub mod methods`, lines 288–308) and golden literal tests (lines 1798–1840) prevent string drift between client and server. Tests would catch a typo; one source of truth.
- **Open string registries for `approval_kind`/`approval_scope`** (lines 34–46) plus the explicit fallback test `unknown_typed_approval_kind_decodes_for_generic_fallback` (line 2257) — exactly the right shape for cross-version evolution.
- **Capability schema versioning split from protocol schema versioning** (`UI_PROTOCOL_SCHEMA_VERSION = 1`, `UI_PROTOCOL_CAPABILITIES_SCHEMA_VERSION = 2`, lines 20–23) lets the handshake payload evolve without a protocol bump, and the legacy decode test at line 1709 proves additive capability fields are non-breaking.
- **UUIDv7 newtypes** (`TurnId`, `ApprovalId`, `PreviewId`, lines 68–113) give temporal ordering for free and keep ids non-confusable at the type level.
- **`#[serde(skip_serializing_if = ...)]` plus `#[serde(default)]` consistently applied** to optional fields gives clean wire payloads (no `null`-stuffing) without strict-decode risk; absent `deny_unknown_fields` anywhere — deliberately permissive, matching spec §4.

## Suggested fixes (sized, narrow)

1. Add `ProgressUpdated(UiProgressEvent)` to `UiNotification` (line 1577) and route it in `from_method_and_params` (line 1641); or remove `progress/updated` from `UI_PROTOCOL_NOTIFICATION_METHODS`. Pick one. Update the golden test.
2. Extend `rpc_error_codes` with the spec §10 taxonomy (`UNKNOWN_SESSION = -32010`, `APPROVAL_NOT_PENDING = -32011`, etc.) and add typed constructors `RpcError::unknown_session(SessionKey)`, `RpcError::cursor_out_of_range(UiCursor)`, etc. Document each in a single `pub mod rpc_error_codes` block.
3. Change `RpcRequest.id` and `RpcErrorResponse.id` from `String` / `Option<String>` to a new `RpcId` newtype that deserializes from string-or-number-or-null (use `serde_json::Value` with a constrained shape, or a dedicated enum). Update tests.
4. Add `#[serde(other)]` fallback variants to `ApprovalDecision`, `ApprovalRespondStatus`, `DiffPreviewFileStatus`, `DiffPreviewLineKind`, `TaskRuntimeState`, `InputItem` — every closed wire enum that v1 might extend. Or convert them to open string newtypes with a `pub mod` registry.
5. Add streaming/limits to the diff-preview path: `DiffPreviewGetParams { max_bytes: Option<u64>, after_file: Option<u32> }` and `DiffPreviewGetResult { truncated: bool, next_offset: Option<...> }`, mirroring `TaskOutputReadResult`. Treat as a UPCR.

## Open questions for the protocol author

1. Was `progress/updated`'s split out of `UiNotification` reviewed under §4.1 change-control? It looks additive but the spec method list (§6, lines 188–199) does not include it — is there a UPCR I missed?
2. Why is `ApprovalDecision` closed but `approval_kind`/`approval_scope` open? Future work likely needs `Defer`, `ApproveOnce`, `ApproveAndAllowSession` — should `decision` also become an open registry?
3. What is the contract between `task_status`, `runtime_state`, and `lifecycle_state` in `TaskOutputReadResult` (lines 741–743)? The test at lines 2470–2472 sets all three to different values — is that intentional? If so, document the orthogonality; if not, collapse them.
4. `UiCursor.stream` — is it identical to `SessionKey.0`, a sub-stream within a session, or globally unique? The test at line 2660 sets `stream: session_id.0.clone()`; is that the canonical convention or an example?
5. `UiResultKind::UnsupportedCapability` exists in the typed result enum but `UiRpcResult::from_method_and_result` cannot reconstruct it. Is `UnsupportedCapability` only a server-emit shape that bypasses typed decode, or should the decoder peek at `result.unsupported` before routing by method?
