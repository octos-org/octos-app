# 03 — UI Protocol v1 Contract Summary

Wire summary of `octos-ui/v1alpha1` as it stands on 2026-04-28. Reference:
`~/home/octos/api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md` (draft, 483 lines), draft Rust types at
`~/home/octos/crates/octos-core/src/ui_protocol.rs`, server handler at
`~/home/octos/crates/octos-cli/src/api/ui_protocol.rs`. Sibling client doc:
`~/home/octos/docs/OCTOS_TUI_ARCHITECTURE_2026-04-24.md`.

This file is meant as the single page a workstream owner reads to understand what their feature
is *allowed* to assume about the wire. Anything not stated here, treat as not-yet-contractual.

## Shape

JSON-RPC 2.0 over a single long-lived WebSocket (`GET /api/ui-protocol/ws`, upgrade). Every
authoritative interactive flow goes through this socket. REST is reserved for snapshot hydrate
(session list, messages, files, workspace contract) and for compatibility with legacy clients.

We negotiate capabilities at session-open time, not per-feature.

## Identity types

All from `crates/octos-core/src/ui_protocol.rs`:

- `SessionId(String)` — server-assigned, stable across reconnects.
- `TurnId(Uuid)` — client-supplied at `turn/start`, echoed in every notification for correlation.
- `ApprovalId`, `PreviewId`, `TaskId`, `ToolCallId` — server-assigned, used in correlated events.
- `UiCursor { stream: String, seq: u64 }` — replay position. The client persists the last applied
  cursor per session and supplies it at reconnect.
- `OutputCursor` — separate cursor namespace for `task/output/read`.

## Methods & notifications

### Session lifecycle

| Direction | Name | Purpose |
|---|---|---|
| → method | `session/open` `{session_id, profile_id?, after?: UiCursor}` → `SessionOpenedResult` | Open or resume. `after` triggers replay; absent = subscribe live |
| ← notif | `session/open` (replay baseline payload) | Sent during replay |

`SessionOpenedResult` carries metadata (id, profile, recently-active turn ids) and an optional
`pane.snapshots.v1` payload (workspace, artifacts, git) when the capability is enabled.

### Turn control

| Direction | Name | Purpose |
|---|---|---|
| → method | `turn/start` `{session_id, turn_id, input: [InputItem]}` → `{accepted: true}` | Begin a turn |
| → method | `turn/interrupt` `{session_id, turn_id}` → `{}` | Abort. Idempotent on already-completed turns |
| ← notif | `turn/started` `{session_id, turn_id}` | First event of the stream |
| ← notif | `turn/completed` `{session_id, turn_id, cursor?}` | Terminal — success |
| ← notif | `turn/error` `{session_id, turn_id, code, message}` | Terminal — failure or interrupted |

`code: "interrupted"` is the documented response to `turn/interrupt`. Other codes (`refused`,
`runtime`, `auth`) are documented but new ones may appear; clients must tolerate unknown codes.

### Live streaming output

| Direction | Name | Purpose | Durable? |
|---|---|---|---|
| ← notif | `message/delta` `{session_id, turn_id, text}` | Streaming assistant tokens | **No** — ephemeral |
| ← notif | `progress/updated` (rich schema) | Token / cost / retry / file-mutation counters | Yes |

Critical: `message/delta` is **explicitly non-durable** (spec lines 415–419). The client uses
deltas to render in real time, but on `turn/completed` it discards the in-flight buffer and
reconciles assistant message text from the next history hydrate or `session_result` event.
This is the same pattern `aichat` already uses (`CHAT_DATA.streaming_text` → committed
`ChatMessage` on completion). We do not invent our own commit logic.

### Tool / task / progress events

All durable, all carry `UiCursor`.

| Direction | Name | Purpose |
|---|---|---|
| ← notif | `tool/started` `{tool_call_id, tool_name, arguments?, …}` | Tool invocation begins |
| ← notif | `tool/progress` `{tool_call_id, progress?, message?, …}` | Optional progress |
| ← notif | `tool/completed` `{tool_call_id, success?, output_preview?, duration_ms?, …}` | Done |
| ← notif | `task/updated` `{task_id, lifecycle_state, runtime_state, summary?}` | Task lifecycle |
| ← notif | `task/output/delta` `{task_id, cursor: OutputCursor, bytes}` | Streaming output |
| → method | `task/output/read` `{session_id, task_id, after?: OutputCursor, limit_bytes?}` → snapshot | Drill-down |

The TaskDock widget (W04) consumes these. ToolCallId correlates `tool/started` →
`tool/progress`* → `tool/completed`. TaskId correlates `task/updated` → `task/output/*`.

### Approval / diff preview

These ride two capabilities:

- `approval.typed.v1` (UPCR-2026-001): payload includes `approval_kind`, `risk`,
  `typed_details`, `render_hints`. Without it, you fall back to `{title, body}`.
- `pane.snapshots.v1` (UPCR-2026-002): enables `diff/preview/get` and pane snapshots.

| Direction | Name | Purpose |
|---|---|---|
| ← notif | `approval/requested` `{approval_id, turn_id, tool_name, title, body, typed_details?}` | Server asks for permission |
| → method | `approval/respond` `{approval_id, decision, scope?, client_note?}` → `{}` | Approve / deny / scoped-approve. **Idempotent** |
| → method | `diff/preview/get` `{session_id, preview_id}` → `DiffPreview` | Fetch unified diff parsed into files/hunks |

`decision` values: `approve`, `deny`, plus optional `approval_scope` (e.g., approve-this-once,
approve-for-session, approve-for-tool). Scope set is not yet final — capability-probe before
binding UI to specific scope strings.

### Reconnect

The wire contract is precise:

1. Client persists the last applied `UiCursor` per session (`AppState.cursor`, see
   `01-ARCHITECTURE.md` § 6).
2. On reconnect, send `session/open { after: cursor }`.
3. Server validates: cursor must be owned by the same profile and must be ≤ current head.
   Stale cursors → `cursor_invalid` error → client drops cursor, re-hydrates via REST snapshot,
   re-opens with no `after`.
4. If valid, server replays *ordered* notifications (the in-memory ledger from M9.6) since that
   cursor, then transitions to live mode.
5. Replayed notifications carry their original cursors. Client applies them in order; ephemeral
   `message/delta` from before the drop is **not replayed** (it was non-durable).

We do not stitch heuristics on top of this. If the cursor was valid and replay applied, state is
caught up. If anything fails, drop the local cursor and re-hydrate from REST.

## Authoritative state model

Two grades of authority:

| Grade | Examples | Source of truth | Survives reconnect? |
|---|---|---|---|
| Authoritative (durable) | `tool/started`, `tool/completed`, `task/updated`, `approval/requested`, `turn/completed`, `progress/updated` | Server in-memory ledger + REST snapshots | Yes (cursor-replayed) |
| Ephemeral | `message/delta` tokens before commit | Client RAM only | No (resync on reconnect) |
| Snapshot-only | session lists, file lists, message history, workspace-contract | REST projections | n/a — re-fetched |

If the spec calls a surface "draft" or "non-authoritative", the AppState reducer treats it as a
hint, never as a commit. Per the recon: the unified task ledger is **not yet** fully through the
event stream — for now, authoritative task state comes from `task/updated` snapshots, not from a
streaming ledger.

## Capability negotiation

Server advertises capabilities in `SessionOpenedResult.capabilities`. Currently:

- `approval.typed.v1`
- `pane.snapshots.v1`

Plus the always-on baseline (turn lifecycle, message deltas, tool events, basic approvals).

Clients **must** treat unknown capabilities as ignorable and unknown enum variants as forward
compatibility. We negotiate once per `session/open` and key UI affordances off the result.

## M9 issue stack — what's not blocking, what's blocking

Signed off (we can rely on):

- M9.1 protocol structs + WebSocket routing
- M9.2 approval request/response + survival across reconnect
- M9.3 diff preview contract — deterministic `preview_id`, typed `DiffPreview`
- M9.4 task output tail/read shape (disk routing depends on M8.7 separately)
- M9.5 rich progress schema preserves tool lifecycle, retries, file mutations
- M9.6 in-memory per-session ledger with cursor replay

Not blocking v1 release; **may** affect us:

- M8 fix-first checklist (spec §12) — runtime correctness items: ToolContext propagation,
  resume sanitizer, worktree-missing refusal, profile/manifest authority.
- M9.8 (web client adoption) is on hold; only M9.8A (coding-only web app) in scope server-side.

For octos-app this means: the protocol is implemented and largely stable; the *server* may still
have rough edges on resume / tool context that the client can't paper over. We surface errors
cleanly when they happen, we don't hide them.

## What stays REST (and why we're fine with it)

The protocol explicitly defers four areas — we keep them on REST:

| Area | REST surface | Why it's OK |
|---|---|---|
| Initial session discovery | `GET /api/sessions` | Cold-start scan; no live stream needed |
| Message history hydrate | `GET /api/sessions/{id}/messages` | One-shot, then live takes over |
| File / artifact bytes | `GET /api/files/{handle}` | Bulk transfer, not interactive |
| Workspace contract / pane snapshot bootstrap | `GET /api/sessions/{id}/workspace-contract` | Snapshot, then `pane.snapshots.v1` deltas keep it fresh |

`OctosUiAgent` (W01) owns both transports and presents one surface to the store: it fans REST
hydrate calls and WS notifications into a single typed `Event` stream.

## Cheat sheet for workstream owners

If you're shipping a feature that talks to the server, this is the decision tree:

1. Is it interactive (turn lifecycle, tool, approval, task, diff)? → WebSocket via `OctosUiAgent`.
2. Is it a snapshot read (sessions, messages, files, workspace)? → REST via `octos-app-transport`.
3. Is it admin / settings? → out of scope; link to web dashboard.
4. Is it dev-mode direct-LLM? → `StatelessBackendAdapter` from `aichat`, gated on a flag.

If the answer is "I don't know which one", the protocol contract page (this doc) is wrong; raise
a discrepancy.
