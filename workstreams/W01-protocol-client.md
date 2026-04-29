# W01 — Protocol Client & Transport

## Mission

Own the boundary between the network and the rest of `octos-app`. Build the
`octos-app-transport` crate (WebSocket + reconnect + REST snapshot client) and the
`OctosUiAgent` adapter that makes the UI protocol look like the Makepad `Agent` trait `aichat`
already consumes. M1 exit: a chat turn round-trips against a real Octos server with streaming,
cancellation, and reconnect-after-drop. W02/W03/W04 build on the typed event stream this
workstream emits.

## Header

| Lane | Depends on | Lifts from | Owner role |
|---|---|---|---|
| A — Spine | M0 (`octos-core` pinned) | `octos-core::ui_protocol`; `octos-tui`'s `transport/{ws,protocol}.rs`; `aichat/libs/makepad_ai/src/agent.rs` | Senior Rust generalist; `tokio-tungstenite`, `reqwest`, JSON-RPC framing, Makepad `Cx::post_action` |

## Scope

In:

- `octos-app-transport` crate: WebSocket dialer, JSON-RPC v2 framing, REST snapshot client,
  exponential reconnect with jitter, heartbeat, request-id correlation, capability handshake.
- `app/backend/octos_ui.rs`: implements `Agent` (`agent.rs:82–116`) by translating
  `create_session`/`send_prompt`/`cancel_prompt` into `session/open`/`turn/start`/`turn/interrupt`
  and converting `RpcNotification`s into `AgentEvent`s.
- Cursor persistence as a callback into the store (W04 owns SQLite).
- Legacy `POST /api/chat?stream=true` behind `--legacy-rest-chat`, off in M1 happy path.
- Contract tests against a fake WS server.

Out: `AppState` reducer (W04). Auth (W08; transport accepts a bearer at construct time).
Approval / TaskDock / file-viewer UI (W04/W05). Multi-window — one connection per process for M1.

## Architecture & implementation plan

**Connection state machine.**
`Idle → Dialing → Handshaking → Live ↔ Reconnecting → Live(replay) → Failed`.

- `Dialing` calls `connect_async` to `wss://…/api/ui-protocol/ws` with `Authorization: Bearer`
  and `X-Profile-Id`; query string carries per-session feature flags
  (`pane_snapshots=1`, `approval_typed=1`), read server-side by
  `ConnectionUiFeatures::from_headers_and_query` (`ui_protocol.rs (cli):282`).
- `Handshaking` sends `session/open` (`ui_protocol.rs:543`); the result (`SessionOpenResult` →
  `SessionOpened`, `:922–942`) supplies the active profile id and an optional `panes`.
- `Live`: typed-channel I/O; cursor tracker watches every cursor-bearing notification (`:62`)
  and persists via callback.
- `Reconnecting`: socket close, ping timeout, or send failure trigger backoff.
- `Live(replay)`: after reopen with `after: cursor`, the server replays ledgered events
  (`ui_protocol.rs (cli):586`) then re-emits `session/open`. We bracket the window with
  `TransportEvent::ReplayStarted`/`ReplayFinished` so the store pauses "is streaming" UI
  without losing it.

**Ephemeral vs durable routing.** Each `TransportEvent::Notification` carries
`durability: Durable | Ephemeral`. `message/delta` (`:1304`) is `Ephemeral` per spec — never
replayed, never committed. Every other method in `UI_PROTOCOL_NOTIFICATION_METHODS` (`:321`) is
`Durable` with a ledger cursor. The store treats durable events as commits; ephemeral folds
into `AppState.ephemeral.streaming_text` and drops on `turn/completed`. Transport never
buffers ephemeral across reconnect.

**Capability handshake** (two-step). Pre-WS `GET /api/version` retrieves
`UiProtocolCapabilities` (`:369–453`); compare against `UI_PROTOCOL_V1` /
`UI_PROTOCOL_SCHEMA_VERSION = 1` (`:17,20`), fail closed on mismatch. Per-session: only
request the WS query flags whose features were advertised
(`UI_PROTOCOL_FEATURE_APPROVAL_TYPED_V1`, `UI_PROTOCOL_FEATURE_PANE_SNAPSHOTS_V1`, `:29,32`).
W04/W05 key off the same flags so we never render fields the server didn't promise.

**Reconnect algorithm.** Full-jitter exponential backoff:
`rand(0..2^min(attempt-1,5))` seconds, ceiling 30 s, abandon after 5 min cumulative. On
reopen send `session/open { after: last_cursor }`. If the response is `RpcError` with
`INVALID_PARAMS` (`:52`) and the message implies stale cursor (currently text-matched against
`replay_after`), drop the cursor, hit `GET /api/sessions/{id}/messages` for REST hydrate,
reopen with `after: None`. No stitching — contract is replay-or-rehydrate.

**Heartbeat.** WS `Ping` every 20 s; link dead if no `Pong` or any frame within 35 s. We
drive the timer ourselves so any inbound frame counts as liveness.

**Legacy REST fallback.** `LegacyChatTransport` (`cfg(feature = "legacy-rest")`) synthesizes
`TransportEvent`s from SSE `data:` frames emitted by `chat_streaming` (`handlers.rs:307`).
Used only when WS upgrade returns 426/501 or `--legacy-rest-chat` is set; approvals /
tasks / diff previews unavailable, downstream UI degrades to "chat only".

## Crate / module layout

`octos-app-transport` (Makepad-free, `cargo test` runnable):

```
src/
  lib.rs        # builder, re-exports
  config.rs     # TransportConfig { url, token, profile_id, capabilities, backoff }
  conn.rs       # ConnectionState machine, run loop
  ws.rs         # tokio-tungstenite framing, ping/pong
  rest.rs       # reqwest: sessions, messages, files, version
  rpc.rs        # RpcRegistry: id → oneshot<Result<UiRpcResult, RpcError>>
  events.rs     # TransportEvent (out) + OutboundCommand (in)
  reconnect.rs  # backoff + jitter
  cursor.rs     # last-applied per session
  legacy.rs     # cfg(feature = "legacy-rest") fallback
  errors.rs     # TransportError
```

Public surface: `TransportHandle::connect(config) -> (CommandTx, EventRx, JoinHandle)`.

`app/backend/octos_ui.rs` (Makepad-aware): holds the channel pair plus a `Cx::post_action`
bridge; implements `Agent` by routing through `OutboundCommand`s and mapping each
`TransportEvent::Notification` into `AgentEvent`. We add `AgentEvent::ProtocolNotification`
upstream in `makepad_ai` (small patch tracked in W10) for tool/approval/task events that
don't fit existing variants.

From `octos-core`: every public type in `crates/octos-core/src/ui_protocol.rs` — no fork.
`Cargo.toml` carries a path dep during dev, git tag in CI.

## Interfaces / contracts

**Methods sent (client → server):**

| Method | Params (file:line) | Result (file:line) |
|---|---|---|
| `session/open` | `SessionOpenParams` `ui_protocol.rs:543` | `SessionOpenResult` `:933` |
| `turn/start` | `TurnStartParams` `:552` | `TurnStartResult` `:945` |
| `turn/interrupt` | `TurnInterruptParams` `:559` | `TurnInterruptResult` `:956` |
| `approval/respond` | `ApprovalRespondParams` `:572` | `ApprovalRespondResult` `:605` |
| `diff/preview/get` | `DiffPreviewGetParams` `:628` | `DiffPreviewGetResult` `:656` |
| `task/output/read` | `TaskOutputReadParams` `:634` | `TaskOutputReadResult` `:729` |

W01 owns plumbing for all six; W04/W05 own the UI behind the latter four. M1 exercises only
the first three.

**Notifications consumed (server → client):** the full set of
`UI_PROTOCOL_NOTIFICATION_METHODS` (`ui_protocol.rs:321–335`), decoded via `UiNotification`
(`:1577`) using `UiNotification::from_rpc_notification` (`:1630`). Unknown methods log at
warn and drop, per the contract's forward-compat rule.

**Channel API exposed to `app/backend`:**

```rust
pub enum OutboundCommand {
    OpenSession  { session_id, profile_id, after: Option<UiCursor> },
    TurnStart    (TurnStartParams),
    TurnInterrupt(TurnInterruptParams),
    ApprovalRespond { params, reply: oneshot::Sender<…> },
    DiffPreviewGet  { params, reply: oneshot::Sender<…> },
    TaskOutputRead  { params, reply: oneshot::Sender<…> },
    HydrateSessionMessages { session_id, reply: oneshot::Sender<…> },
    Disconnect,
}

pub enum TransportEvent {
    Connection(ConnectionState),
    Notification { payload: UiNotification, durability, cursor: Option<UiCursor> },
    ReplayStarted,
    ReplayFinished { applied: usize, head: Option<UiCursor> },
    Error(TransportError),
}
```

`reply` channels carry the typed `UiRpcResult` variant (`ui_protocol.rs:970`). Lifecycle
methods (`OpenSession`/`TurnStart`/`TurnInterrupt`) surface success through `Connection` /
`Notification` events, not a one-shot — their effect is on session state, not a UI payload.

## Tests & verification

- **Mock-server contract tests** (`tests/contract_*.rs`, `axum` + `tokio-tungstenite`): turn
  happy path; interrupt mid-stream → `TurnErrorEvent { code: "interrupted" }` matching
  `ui_protocol.rs (cli):1124`; stale cursor → REST hydrate fallback; capability downgrade with
  `pane.snapshots.v1` absent.
- **Reconnect fault injection.** Middleware drops the socket at chosen byte offsets: during
  handshake; mid-stream before any cursor; mid-stream after a cursor (must replay exactly the
  missed events); during an in-flight request (errors with `TransportError::Disconnected`,
  registry clears).
- **Cursor-replay property test.** After forced reconnect, `TransportEvent::Notification` has
  strictly increasing `cursor.seq`, no event ≤ `last_applied` reappears, no event in between
  is skipped. 200 random drop points.
- **Live smoke** (`cargo run --example smoke`, not CI): real server, one turn; asserts
  `MessageDelta` + `TurnCompleted`; manual reconnect via `pkill -STOP / -CONT`.

## Deliverables

1. Crate skeleton + public API stubs compiling against `octos-core`. (1 d)
2. REST client: `list_sessions`, `get_session_messages`, `get_file`, `get_version`. (1 d)
3. WS dialer + JSON-RPC framing + `RpcRegistry`. (2 d)
4. Connection state machine, `Idle → Live` happy path. (1.5 d)
5. Reconnect, backoff, cursor replay, stale-cursor REST fallback. (2 d)
6. Capability handshake (version probe + WS query flags). (0.5 d)
7. Heartbeat / ping-pong timer. (0.5 d)
8. `OctosUiAgent` adapter + `AgentEvent::ProtocolNotification` upstream patch. (2 d)
9. Legacy REST fallback (`feature = "legacy-rest"`). (1.5 d)
10. Contract tests + mock-server fixtures. (3 d)
11. Reconnect fault-injection harness. (1.5 d)
12. Live smoke example + runbook. (0.5 d)

Total ~17 d; fits M1 (4 weeks) for one owner with overlap.

## Exit criteria

- [ ] `cargo build -p octos-app-transport` clean on stable.
- [ ] All `tests/contract_*.rs` green against the pinned `octos-core` rev.
- [ ] Reconnect fault-injection matrix green (4/4 drop scenarios).
- [ ] Cursor monotonicity property test green over 200 random drop points.
- [ ] `OctosUiAgent` passes the `aichat` `Agent` test harness (W10).
- [ ] Live smoke: one turn end-to-end with `MessageDelta`, `ToolStarted`/`ToolCompleted`,
      `TurnCompleted`; 20 s server STOP/CONT invisible (no duplicate text, no lost completion).
- [ ] No `unwrap()`/`expect()` outside tests; `cargo doc` clean.

## Risks

- **Cursor rejection rules still settling** (`02-API-DRIFT.md`). Mitigation: contract test
  pinned to a known-good server commit; ask server team for an explicit error code beyond
  `INVALID_PARAMS`.
- **`turn/interrupt` on completed turns.** Handler at `ui_protocol.rs (cli):1084–1129` returns
  `{interrupted: false}` when no active turn — we treat as success, pinned by contract test.
- **`AgentEvent` not extensible.** Adding `ProtocolNotification` is a small upstream PR to
  `aichat/libs/makepad_ai`; if it slips, keep the variant in a local trait re-export.
- **Tokio runtime collision with Makepad.** We run `current_thread` on a dedicated thread
  (mirrors `aichat`); CI audits `Cargo.lock` for `rt-multi-thread` collisions.
- **Capability schema bump.** `UI_PROTOCOL_CAPABILITIES_SCHEMA_VERSION = 2` (`:23`); a bump
  to 3 mid-M1 fails the version probe closed with a banner.

## Open questions

1. Stable error code for "stale cursor"? Currently text-matched. Asked of server team; tracked
   in `06-WORKSTREAMS.md` § Coordination.
2. Cursor persistence: callback into W04's SQLite, or transport owns it? Plan: callback;
   confirm with W04 owner.
3. Multi-session multiplex on one WS — server keeps per-connection turn state keyed by
   `session_id` (`SharedConnectionTurns`, `ui_protocol.rs (cli):297`). M1 opens one; data
   structures stay session-keyed for M2 tabbed sessions.
4. `progress/updated` is registered (`:306`) but not yet a `UiNotification` variant
   (`UiProgressEvent` at `:1249` standalone). M1 forward-compat no-op; W04 wires rendering
   when the variant lands upstream.
