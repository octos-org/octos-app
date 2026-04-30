# octos-app-transport — STATUS

**Second W01 pass: runtime async loop landed.** `cargo check -p octos-app-transport`
clean on stable; `cargo test -p octos-app-transport` runs 14 unit tests plus the
`contract_tool_started` integration test (a `tokio-tungstenite` mock server
pushes a `tool/started` notification through `ws::spawn` and asserts a
`TransportEvent::DurableNotification` arrives — exercises upgrade handshake,
JSON-RPC framing, notification typing, and the durable/ephemeral router).

## What's wired up

- **`ws::spawn`** spawns a tokio task driving the connection state machine
  (`Idle → Dialing → Handshaking → Live ↔ Reconnecting → Failed`). Dial uses
  `tokio_tungstenite::connect_async` with `Authorization: Bearer <token>` +
  `X-Profile-Id: <profile_id>` headers; scheme rewrite (`http/https → ws/wss`)
  appends `/api/ui-protocol/ws`. Each transition emits
  `TransportEvent::ConnectionState`.
- **Inner `select!`** arbitrates (a) the outbound `OutboundCommand` channel,
  (b) the inbound WS `next()` stream, (c) a 30-s heartbeat tick (`Ping` out;
  inbound `Ping` answered with `Pong`).
- **Frame router** parses `RpcEnvelope`. Notifications go through
  `UiNotification::from_method_and_params`; `is_ephemeral_method` (only
  `message/delta` per the contract) routes to `EphemeralNotification`,
  everything else to `DurableNotification` with the cursor pulled from
  `params.cursor` (when present). Successful `Response`s for lifecycle
  methods (`session/open`, `turn/start`, `turn/interrupt`) emit
  `RpcResult(LifecycleResult::*)`; non-lifecycle responses resolve the
  pending oneshot for the originating `OutboundCommand`. `ErrorResponse`s
  resolve oneshots with `Err(RpcError)` and emit `TransportEvent::RpcError`.
- **Capability handshake.** On the first successful `session/open` response,
  `Capabilities::parse` reads either `supported_features: [..]` or a
  `{name: bool}` map and emits `TransportEvent::CapabilityNegotiated`.
- **Reconnect.** `next_backoff` (full-jitter exponential, ceiling 30 s) plus
  a 5-min cumulative budget; on exhaustion the task transitions to
  `ConnectionState::Failed` and exits.
- **Cursor on reconnect.** `OutboundCommand::OpenSession` with `after: None`
  re-attaches the in-memory cursor for the session (set on every durable
  notification). REST hydrate fallback for stale-cursor `INVALID_PARAMS` is
  not yet wired — see open TODOs.
- **Backpressure.** The events channel is bounded at 64; the WS task uses
  `try_send` and logs a `warn!` rather than blocking when the receiver
  stalls (per the contract: "Inbound frames must NEVER block").

## Module status

| File | Status |
|---|---|
| `src/lib.rs` | unchanged (public API frozen) |
| `src/jsonrpc/mod.rs` | `RpcEnvelope`, `RpcRegistry` (`next_id`/`register`/`complete`/`cancel_all`), `serialize_request`. Internal-typed `JsonRpcId = String`; `JsonRpcError` carries `Decode/UnknownId/Server/Cancelled` |
| `src/rest/mod.rs` | `RestClient::{list_sessions, messages_for, delete_session, upload, file_url, version_probe, my_content}` against `reqwest`. Bearer + `X-Profile-Id` headers built once; `decode<T>` reads bytes then parses, surfacing non-2xx as `RestError::Status { status, body }`. `file_url` returns `ResolvedFileUrl { bare, with_token }` so W04 can pick |
| `src/ws/mod.rs` | full state machine + dispatch as above |
| `src/capability/mod.rs` | `Capabilities::parse(&Value)` accepts either `supported_features` array or `{feature: bool}` map shape |
| `src/cursor/mod.rs` | in-memory `CursorStore` (`get/set/delete`); `CursorPersist` trait + `NoopCursorPersist` impl. SQLite impl is a TODO comment for W04 |
| `tests/contract_tool_started.rs` | one `#[tokio::test(flavor = "current_thread")]` mock-server contract test |

## Open TODOs (third pass)

1. **Structured RPC error variants.** `RestError` has the new `Network` /
   `DecodeBody` / `Other` variants, but `JsonRpcError` still carries a free
   `String` for `UnknownId`. We probably want a typed `StaleCursor` variant
   keyed off `INVALID_PARAMS + replay_after` so the reducer can drive REST
   hydrate without text-matching.
2. **Full cursor recovery flow.** Today we re-attach the last in-memory
   cursor on `OpenSession`. Missing: stale-cursor detection (server
   `INVALID_PARAMS` w/ `replay_after`), cursor drop, REST `messages_for`
   hydrate, re-open with `after: None`. W01 § "Reconnect algorithm" sketches
   the bracket — wiring it requires a reducer-side hook to reset session
   state, so it lands with W04.
3. **REST retry policy.** `list_sessions` / `messages_for` / `version_probe`
   currently fail on the first network error. We should add a small retry
   loop (3 attempts, full-jitter, ≤ 5 s total) for the cold-start probes;
   anything still failing after that bubbles up as a banner.
4. **`tool/result` wire shape.** AppUI has no stable `tool/result` command yet,
   so the transport intentionally does not emit that non-contract method.
   Add it only after the AppUI contract defines the method and payload.
5. **Capability flags on the upgrade query string.** W01 plan calls for
   `?pane_snapshots=1&approval_typed=1` query params; today the headers do
   the work and the server ignores the query. Add when W04 needs the
   per-session toggle.
6. **`progress/updated` notification variant.** Lives in `octos-core` as a
   standalone `UiProgressEvent` (ui_protocol.rs:1249); not yet a
   `UiNotification` variant. Currently dropped with a `warn!` — forward-
   compat per contract, will be wired when the upstream variant lands.

## Next slice

`OctosUiAgent` adapter (`app/backend/octos_ui.rs`), the `Agent` impl on top
of these channels, and the `octos-cli` mock-server contract tests for the
reconnect-fault-injection matrix (W01 § Tests & verification).
