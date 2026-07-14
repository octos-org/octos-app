# W09 — Control-plane protocol (Phase 3 of AGENT-OS)

Design of the Phase-3 **control-plane** verbs the ui-protocol needs so the client
(window manager) and a thin advisory **AMA** can drive session focus / lifecycle /
routing. Grounds every addition in the existing `octos-core` ui-protocol
(`octos/crates/octos-core/src/ui_protocol.rs`) and the server dispatcher
(`octos/crates/octos-cli/src/api/ui_protocol.rs`), and matches the current wire
conventions. **This is a design document — no code changes.** It is the protocol
prerequisite for `AGENT-OS-ARCHITECTURE.md` §7/§11 (Phase 3) and builds on the
Phase-2 client foundation in `W08-MULTI-SESSION-CLIENT.md`.

Status: v1 draft, code-grounded 2026-07-13.

---

## 0. TL;DR

| Verb | Direction | New wire method | Server change? | Reuses existing? |
|---|---|---|---|---|
| `session/focus` | client→server (advisory hint) | `session/focus` | **Optional / thin** | no |
| `session/detach` | client→server | `session/detach` | **Yes (small)** | forwarder map |
| `session/suspend` | client→server | `session/suspend` | **Yes (small)** | detach + cache evict |
| `session/close` | client→server | `session/close` | **Yes (small)** | detach + `session/delete` |
| `session/list` | client→server | *(exists)* — extend result | **Yes (additive)** | `methods::SESSION_LIST` |
| AMA route proposal | server→client notification | `route/proposal` | **Yes** | `session/orchestration` shape |
| AMA route ack | client→server | `route/apply`, `route/reject` | **Yes (thin)** | — |
| App housekeeping | server→client notification | `session/needs_focus`, `session/idle`, `session/status` | **Yes** | notification + emitter tool |
| Speculative open | client→server | *(reuse `session/open`)* | no | `methods::SESSION_OPEN` |
| Cancel speculative turn | client→server | *(reuse `turn/interrupt`)* | no | `methods::TURN_INTERRUPT` |

Biggest tension: **focus is client-owned by design, but every server-side reason to
know focus (cost, hold-first-paint, AMA reassignment) is advisory** — so the control
plane must stay a *hint/proposal* layer that never becomes authoritative, or it
contradicts "client is the window manager" (`AGENT-OS-ARCHITECTURE.md` §3.2).

---

## 1. What already exists (grounding, with citations)

All line numbers are `octos/crates/octos-core/src/ui_protocol.rs` unless noted.

### 1.1 Method-name conventions
Method constants live in `pub mod methods` (`:943`). Two naming families coexist:

- **Core lifecycle verbs use flat slash** — `session/open` (`:947`), `turn/start`
  (`:948`), `turn/interrupt` (`:949`), `session/hydrate` (`:964`),
  `session/rollback` (`:968`), `session/delete` (`:1115`), `session/list` (`:1097`),
  `session/snapshot` (`:1100`).
- **The M12 REST-bridge batch uses a dotted terminal sub-verb** — `session/status.get`
  (`:1104`), `session/files.list` (`:1106`), `session/title.set` (`:1113`).

**House-style decision for W09:** the new control-plane verbs are session *lifecycle*
operations, so they follow the **flat-slash family** (`session/focus`,
`session/detach`, `session/suspend`, `session/close`) — a sibling set to
`session/open` / `session/delete`, not to the REST-bridge readers.

Every command method is also registered in `UI_PROTOCOL_COMMAND_METHODS` (`:1174`) and
every notification in `UI_PROTOCOL_NOTIFICATION_METHODS` (`:1230`); new verbs must be
added to the matching list.

### 1.2 Session identity & cursor
- `SessionKey(pub String)` — canonical form `{profile}:{channel}:{chat_id}#{topic}`
  with helpers `base_key()` / `topic()` / `profile_id()` / `channel()` / `chat_id()`
  (`octos/crates/octos-core/src/types.rs:491`, `:526`–`:566`).
- `UiCursor { stream: String, seq: u64 }` — the per-session resumable ledger cursor
  (`:520`). Durable notifications carry it; the client keys replay off it.
- `TurnId(pub Uuid)` minted `Uuid::now_v7()` (`:527`).

### 1.3 Existing session/turn params & results (the shapes to imitate)
```rust
// :1731
pub struct SessionOpenParams {
    pub session_id: SessionKey,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub sandbox: Option<SessionSandboxParams>,
    #[serde(skip_serializing_if = "Option::is_none")] pub after: Option<UiCursor>,   // replay bracket
}
// :3975 / :3926
pub struct SessionOpenResult { pub opened: SessionOpened }
pub struct SessionOpened {
    pub session_id: SessionKey,
    /* active_profile_id?, workspace_root?, context?, context_state?, */
    #[serde(skip_serializing_if = "Option::is_none")] pub cursor: Option<UiCursor>,  // resume point
    #[serde(default = "UiProtocolCapabilities::first_server_slice")] pub capabilities: UiProtocolCapabilities,
    /* panes?, reasoning_effort? */
}
// :1826 / :4019
pub struct TurnInterruptParams { pub session_id: SessionKey, pub turn_id: TurnId }
pub struct TurnInterruptResult {
    pub interrupted: bool,
    #[serde(skip_serializing_if = "Option::is_none")] pub reason: Option<String>,           // e.g. "turn_id_mismatch"
    #[serde(skip_serializing_if = "Option::is_none")] pub terminal_state: Option<String>,   // "completed"|"errored"|"interrupted"
    #[serde(skip_serializing_if = "Option::is_none")] pub ack_timeout: Option<bool>,
}
// :2828 / :2834 — session/list ALREADY EXISTS
pub struct SessionListParams {}
pub struct SessionListResult { pub sessions: Value }   // = GET /api/sessions rows (SessionInfo[])
// :2974 / :2982 — session/delete ALREADY EXISTS
pub struct SessionDeleteParams { pub session_id: String }
pub struct SessionDeleteResult {}                      // void
```

Void/echo results are objects, not `null` (see `SessionDeleteResult {}` `:2982` and
its rationale comment `:2979`); new void results follow suit.

### 1.4 Durable vs ephemeral split (load-bearing for replay)
The server appends notifications to a per-session durable ledger
(`octos/crates/octos-cli/src/api/ui_protocol_ledger.rs`: `UiProtocolLedger::durable`
`:110`, `append_notification` assigns a monotonic `seq` `:899`, `replay_after`
`:2020`). The client classifies frames on receipt: **only `message/delta` is
ephemeral**; everything else is durable and cursor-bearing
(`octos-app/crates/octos-app-transport/src/proto.rs:169` `is_ephemeral_method`,
split into `TransportEvent::DurableNotification{payload,cursor}` vs
`EphemeralNotification{payload}` at `octos-app-transport/src/lib.rs:185`).

**Consequence for W09:** any new control-plane *notification* the client must not miss
across reconnect (route proposals, `needs_focus`) has to be **durable** (ledger-backed,
cursor-bearing). Focus/detach/suspend *replies* are RPC results, not ledger events, so
they are never replayed — the client re-establishes that state itself on reconnect.

### 1.5 Server multiplexing & the per-session forwarder (the key enabler)
- Per-connection map `SharedLiveForwarders = Arc<Mutex<HashMap<SessionKey, JoinHandle>>>`
  (`ui_protocol.rs:269`). **Each open session already has its own live pump task.**
- `handle_session_open` (`:8493`) subscribes to the session's broadcast
  (`ledger.subscribe` `:8516`), replays durable events after `params.after`
  (`:8581`), then `spawn_live_forwarder` (`:8650`) pumps new events to the socket.
  Re-opening a session **aborts the prior forwarder** and restarts from a fresh
  baseline (`:8812`).
- Connection teardown `abort_live_forwarders` (`:5329`) drains **all** forwarders,
  aborts each, then `prune_subscriber_if_idle` reclaims the broadcast sender.
- **This is exactly the primitive `detach`/`suspend`/`close` need:** abort *one*
  entry + prune, instead of all-or-nothing on disconnect.

### 1.6 One-active-turn-per-session invariant
Process-global `active_turns: HashMap<SessionKey, ActiveTurn>` gates turns.
`handle_turn_start` rejects a second concurrent turn with
`RpcError::invalid_request("a turn is already running for this session")`
(`:10369`–`:10401`). `handle_turn_interrupt` (`:11137`) is the only cancel path;
interrupt is **not rollback** — already-streamed deltas and tool side-effects persist
(`AGENT-OS-ARCHITECTURE.md` §9, §10(c)). Any control verb that touches a busy session
must decide: reject, or interrupt-first-then-proceed.

### 1.7 Runtime cache = the real "suspend"
`SessionRuntime` lives in a **TTL + LRU soft-cap cache**
(`octos/crates/octos-cli/src/runtime/cache.rs`: `get_or_init` `:254`, LRU eviction
`:457`, background idle-TTL sweep). Eviction ≠ "activity stopped"; it is **cache loss**,
and the session rehydrates from the JSONL ledger on next `session/open`
(`AGENT-OS-ARCHITECTURE.md` §1, §10(d)). So a "suspend" verb is not building new
lifecycle machinery — it is at most a *hint to evict now* over a mechanism that
already evicts on its own.

### 1.8 Error codes & capability gating
- `rpc_error_codes` (`:416`): `METHOD_NOT_SUPPORTED -32004` (`:425`),
  `UNKNOWN_SESSION -32100` (`:432`), `UNKNOWN_TURN -32101` (`:434`),
  `CURSOR_OUT_OF_RANGE -32110` (`:466`), `PERMISSION_DENIED -32120` (`:472`),
  `UNSUPPORTED_CAPABILITY -32130` (`:476`), `RUNTIME_NOT_READY -32140` (`:479`),
  `RATE_LIMITED -32160` (`:486`), `RESOURCE_NOT_FOUND -32170` (`:495`).
  Constructors: `invalid_request` (`:695`), `invalid_params` (`:706`),
  `not_found` (`:857`), `method_not_supported` (`:1653`).
- Feature flags gate every additive surface: `pub const UI_PROTOCOL_FEATURE_* : &str`
  (`:101`–`:241`, e.g. `state.session_hydrate.v1` `:117`), negotiated into
  `SessionOpened.capabilities` (`:3956`) and checked per-connection before dispatch
  (server pattern: `features.session_hydrate` `:1104`, `has_ui_feature`).

**W09 introduces two feature flags** (additive; old clients ignore, old servers reject
with `method_not_supported`):
```rust
pub const UI_PROTOCOL_FEATURE_CONTROL_SESSION_LIFECYCLE_V1: &str = "control.session_lifecycle.v1"; // focus/detach/suspend/close + list.runtime_state
pub const UI_PROTOCOL_FEATURE_CONTROL_ROUTING_V1: &str          = "control.routing.v1";            // route/proposal + apply/reject + housekeeping
```

### 1.9 Client-side surface to extend
- `OutboundCommand` (`octos-app-transport/src/lib.rs:136`): `OpenSession`,
  `OpenSessionFresh`, `ListSessions`, `HydrateSession`, `StartTurn`, `InterruptTurn`,
  `SendApprovalResponse`, `Disconnect`, … — new verbs add variants here.
- `TransportEvent` (`lib.rs:180`) and `AppUiCommand` / `AppUiEvent`
  (`octos-core/src/app_ui.rs:113`, `:172`) are the app-facing command/event surface.
- Per-session cursors already exist: `SharedState.cursors: CursorStore`
  (`proto.rs:44`); `OpenSession` resumes from the session's own cursor (`proto.rs`
  build_outbound), `OpenSessionFresh` opens without a bracket — this is the W08 fix.

---

## 2. Design principles for the additions

1. **Advisory, never authoritative.** The client owns focus/window/lifecycle truth
   (`AGENT-OS-ARCHITECTURE.md` §3.2). Server control verbs are *hints* (focus) or
   *resource operations* (detach/suspend/close); AMA output is *proposals*. No server
   verb may override a client decision.
2. **Correctness independent of focus.** Events are session-keyed and fan out per
   forwarder regardless of which session is foreground (`§1.5`). Focus must remain a
   pure scheduling/priority/UX signal; if any correctness path starts depending on it,
   the design has regressed.
3. **Reuse the forwarder + cursor machinery.** detach/suspend/close are per-session
   variants of the existing all-forwarders teardown (`§1.5`); resume is just
   `session/open { after: cursor }` — no new replay path.
4. **Preserve invariants.** One-active-turn-per-session (`§1.6`) and cancel≠rollback
   (`§1.6`) are hard constraints every verb states its behavior against.
5. **Additive & capability-gated.** New methods/notifications/fields are optional,
   flag-gated, and forward/backward compatible (unknown fields ignored; unknown
   methods → `method_not_supported`).

---

## 3. The verbs

For each: **direction · params · result · JSON · errors · idempotency ·
cursor/replay interaction · server-vs-client · invariant conflicts.**

### 3.1 `session/focus` — *advisory foreground hint*

**Is focus purely client-side? Argument.** Yes, by default — and it should stay that
way. The window manager (client) already knows which session is foreground; the server
does not need it for delivery correctness (`§1.5`). Phase 2 deliberately keeps *all*
sessions subscribed with no server focus concept (`W08-MULTI-SESSION-CLIENT.md` §"Key
decisions"). **However**, three server-side concerns *benefit* from knowing focus, and
none of them requires authority:

- **Cost/scheduling** — deprioritize model calls for background sessions; give the
  foreground turn the fast lane (`AGENT-OS-ARCHITECTURE.md` §9 "Cost").
- **Hold-first-paint** — on low-confidence routing the server could withhold the first
  visible delta of a background/speculative turn (`§9` "Premature output").
- **AMA reassignment** — the AMA proposes a new foreground; the *client* applies it and
  then tells the server "this is foreground now" so scheduling follows.

**Verdict:** ship `session/focus` as an **optional advisory hint** (client→server
request), gated on `control.session_lifecycle.v1`, that a server MAY ignore entirely
(a conformant server may treat it as a no-op returning `{ "acknowledged": true }`).
It sets a per-connection "foreground session" marker used only for scheduling/telemetry
and the optional hold-first-paint policy. It is **not** required for the client to
switch windows — the client can foreground purely locally.

- **Direction:** client → server (request/result).
- **Params / Result (Rust-ish, house style):**
```rust
pub const SESSION_FOCUS: &str = "session/focus";

pub struct SessionFocusParams {
    pub session_id: SessionKey,
    /// Optional: focus a sub-topic bucket (mirrors session/open.topic :1735).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    /// Advisory intent so the server can tune scheduling. Registry, open set.
    /// "user" (explicit switch) | "ama" (applied proposal) | "restore" (reconnect).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}
pub struct SessionFocusResult {
    /// Always true when the session exists and belongs to this connection's scope.
    pub acknowledged: bool,
    /// Echo of the session the server now considers this connection's foreground.
    pub session_id: SessionKey,
}
```
- **JSON:**
```json
// → request
{"jsonrpc":"2.0","id":"c12","method":"session/focus",
 "params":{"session_id":"_main:local:weather","actor":"user"}}
// ← result
{"jsonrpc":"2.0","id":"c12","result":{"acknowledged":true,"session_id":"_main:local:weather"}}
```
- **Errors:** `UNKNOWN_SESSION -32100` if the session was never opened on this
  connection; `PERMISSION_DENIED -32120` if the session is outside the connection's
  profile scope (reuse `validate_session_scope`, `ui_protocol.rs:10290`);
  `METHOD_NOT_SUPPORTED -32004` if the feature isn't negotiated.
- **Idempotency:** fully idempotent — focusing the already-focused session is a no-op
  returning the same result. Last-write-wins per connection.
- **Cursor/replay:** none. Focus is connection-local scheduling state, never
  ledger-backed. On reconnect the client re-asserts focus after re-opening its
  sessions (an `actor:"restore"` focus call); nothing to replay.
- **Server vs client:** **can be a pure client convention for v1** — the client just
  foregrounds locally and skips the RPC. Promote to a thin server verb only when
  scheduling/hold-first-paint is actually implemented. Recommend: **define the wire
  method now, implement server side as a no-op ack, wire real scheduling later.**
- **Invariant conflicts:** none. Does not touch `active_turns` or forwarders.

### 3.2 `session/detach` — *stop streaming this session to this connection; keep runtime + ledger*

**Do we need a real detach? Cost/benefit.** Phase 2 keeps every background session
subscribed with a live forwarder (`W08` §"Key decisions") — N broadcast receivers, N
pump tasks, and unbounded client-side buffering per background app. On a phone with a
handful of apps that is fine; the W08 doc itself flags "no-detach bandwidth cost" and
"memory of N buffering sessions" as the risks (`W08` §Risks). `detach` converts the
all-or-nothing teardown (`abort_live_forwarders` `:5329`) into a **per-session** abort:

- **Benefit:** bounded live fan-out and battery/bandwidth — a backgrounded app stops
  pushing deltas over the wire; the client relies on its cursor + `session/open` replay
  when the user returns.
- **Cost:** on reattach the client must replay from cursor (already supported, zero new
  machinery). A turn that completes while detached is captured durably in the ledger and
  replayed on reattach — no loss.
- **Server-state implication:** the session's `SessionRuntime` stays cached (still
  warm), only the *delivery* to this connection stops. This is the cheap, safe half of
  "background a session."

- **Direction:** client → server (request/result).
- **Params / Result:**
```rust
pub const SESSION_DETACH: &str = "session/detach";

pub struct SessionDetachParams {
    pub session_id: SessionKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
}
pub struct SessionDetachResult {
    pub detached: bool,
    /// The client's authoritative resume point: the session's current ledger head.
    /// Client stores this and replays from it on the next session/open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<UiCursor>,
    /// True if a turn was still active at detach time (informational — detach does
    /// NOT interrupt it; the turn keeps running and lands in the ledger).
    #[serde(default)] pub turn_active: bool,
}
```
- **JSON:**
```json
// → request
{"jsonrpc":"2.0","id":"c20","method":"session/detach","params":{"session_id":"_main:local:travel"}}
// ← result
{"jsonrpc":"2.0","id":"c20","result":{"detached":true,"cursor":{"stream":"_main:local:travel","seq":142},"turn_active":false}}
```
- **Server implementation:** remove this session's entry from `SharedLiveForwarders`,
  `handle.abort()`, `handle.await`, `ledger.prune_subscriber_if_idle(&session_id)` —
  i.e. the single-session slice of `abort_live_forwarders` (`:5346`–`:5352`). Read the
  session's current ledger head for the returned `cursor`.
- **Errors:** `UNKNOWN_SESSION -32100` (not open on this connection);
  `PERMISSION_DENIED -32120` (scope); `METHOD_NOT_SUPPORTED -32004` (feature).
- **Idempotency:** idempotent. Detaching an already-detached session returns
  `detached:false` with the current head cursor (or `detached:true` again — pick one;
  recommend `detached:false, cursor:<head>` so double-detach is observably a no-op).
- **Cursor/replay:** the returned `cursor` is the contract. Reattach = `session/open`
  with `after` = that cursor (`SessionOpenParams.after` `:1744`), which replays every
  durable event since detach (`handle_session_open` replay loop `:8581`). Ephemeral
  `message/delta` produced while detached is **not** replayed (by design `§1.4`) — but
  the terminal `turn/completed` + persisted assistant message are durable, so the final
  card is never lost, only the intermediate token stream.
- **Invariant conflicts:** none — detach explicitly does **not** interrupt the active
  turn (`turn_active` is informational). This preserves one-active-turn and cancel≠
  rollback: the turn keeps running server-side to durable completion.

### 3.3 `session/suspend` — *detach + release the server runtime (evict now)*

**Do we need suspend distinct from detach? Evaluation.** Mostly **no, for v1** — the
runtime cache already evicts idle sessions on TTL/LRU (`§1.7`), so "free an idle app's
memory/model context" happens automatically without a verb. `session/suspend` adds
exactly one capability over `session/detach`: **proactively trigger eviction now**
instead of waiting for the TTL sweep. That is a minor optimization with one real
subtlety the automatic path also faces: **you must not evict mid-turn.**

Recommendation: **model suspend as `detach` + an explicit `release_runtime` request**,
and gate the runtime release on the session being quiesced. Concretely, either
(a) ship `session/suspend` as a thin verb, or (b) fold it into
`session/detach { release_runtime: true }`. This doc specifies the standalone verb for
clarity but notes (b) is a legitimate simplification.

- **Direction:** client → server (request/result).
- **Params / Result:**
```rust
pub const SESSION_SUSPEND: &str = "session/suspend";

pub struct SessionSuspendParams {
    pub session_id: SessionKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    /// What to do if a turn is still active:
    ///   "reject" (default) — fail with SESSION_BUSY, do not detach
    ///   "interrupt"        — turn/interrupt first, then suspend on terminal
    ///   "defer"            — detach now, evict runtime only after the turn ends
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_active_turn: Option<String>,
}
pub struct SessionSuspendResult {
    pub suspended: bool,
    /// Resume point (same contract as detach).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<UiCursor>,
    /// Whether the server actually dropped the cached runtime (false if deferred
    /// because a turn was active and on_active_turn = "defer").
    #[serde(default)] pub runtime_released: bool,
}
```
- **JSON:**
```json
// → request
{"jsonrpc":"2.0","id":"c31","method":"session/suspend",
 "params":{"session_id":"_main:local:shopping","on_active_turn":"reject"}}
// ← result
{"jsonrpc":"2.0","id":"c31","result":{"suspended":true,"cursor":{"stream":"_main:local:shopping","seq":88},"runtime_released":true}}
```
- **Server implementation:** run the `detach` teardown (`§3.2`), then request the
  runtime cache to drop this key (a new `cache.evict(&key)` over the existing map in
  `runtime/cache.rs`). Eviction is "never correctness-critical" per the cache's own doc
  (`cache.rs:457` comment), so this is safe; the only guard is the active-turn check.
- **Errors:** `SESSION_BUSY` (see §5, new code) when `on_active_turn:"reject"` and a
  turn is active; plus `UNKNOWN_SESSION`, `PERMISSION_DENIED`, `METHOD_NOT_SUPPORTED`.
- **Idempotency:** idempotent — suspending an already-suspended (evicted) session
  returns `suspended:true, runtime_released:false` (nothing left to release) with the
  head cursor.
- **Cursor/replay:** identical to detach — resume via `session/open { after }`. The
  extra cost vs detach is **rehydration latency** on resume (the ledger must be replayed
  into a fresh `SessionRuntime` via `SessionRuntime::bootstrap`, `cache.rs:254`). The
  wire replay is the same; only server-side warm-up differs.
- **Invariant conflicts:** the active-turn interaction is the whole subtlety —
  evicting a runtime with a live turn would abort it non-deterministically (worse than
  `turn/interrupt`, no terminal event guarantee). The `on_active_turn` param makes the
  policy explicit; default `"reject"` is the safe choice and preserves one-active-turn +
  clean terminal emission.

### 3.4 `session/close` — *tear down the window; retention policy for the ledger*

**Why not just `session/delete`?** `session/delete` **already exists** (`:1115`,
handler `:13645` delegating to REST `delete_session`) and is **destructive** — it
removes the persisted session/history. "Close the window" in a tabbed-OS is different:
the user closes an app view but the app (its ledger/history) may deserve to survive so
it can be reopened. So `session/close` is a *lifecycle* verb with an explicit
**retention** choice, and `purge` retention is defined to be exactly `session/delete`.

- **Direction:** client → server (request/result).
- **Params / Result:**
```rust
pub const SESSION_CLOSE: &str = "session/close";

pub struct SessionCloseParams {
    pub session_id: SessionKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    /// Ledger retention after close:
    ///   "retain" (default) — detach + evict runtime; JSONL ledger + history KEPT;
    ///                        session can be reopened later (rehydrates from ledger).
    ///   "purge"            — equivalent to session/delete: history removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention: Option<String>,
    /// If a turn is active: "interrupt" (default) or "reject".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_active_turn: Option<String>,
}
pub struct SessionCloseResult {
    pub closed: bool,
    /// "retain" | "purge" — what the server actually did.
    pub retention: String,
    /// Present only for retain: the head cursor at close (reopen resumes here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<UiCursor>,
}
```
- **JSON:**
```json
// → close-but-keep-history (default)
{"jsonrpc":"2.0","id":"c40","method":"session/close",
 "params":{"session_id":"_main:local:weather","retention":"retain"}}
// ← result
{"jsonrpc":"2.0","id":"c40","result":{"closed":true,"retention":"retain","cursor":{"stream":"_main:local:weather","seq":204}}}
```
- **Server implementation:** default (`retain`) = interrupt any active turn
  (`handle_turn_interrupt` semantics), run the `detach` teardown, evict the runtime —
  i.e. `suspend` with a forced runtime release. `purge` additionally calls the existing
  delete path (`handle_session_delete` `:13645`). No new destructive logic beyond wiring.
- **Errors:** `SESSION_BUSY` (if `on_active_turn:"reject"` and busy); `UNKNOWN_SESSION`;
  `PERMISSION_DENIED`; `METHOD_NOT_SUPPORTED`. Purge inherits delete's REST error mapping
  (`rest_status_to_rpc_error` `:13674`).
- **Idempotency:** closing an already-closed session returns `closed:false` (retain) or
  is a no-op 404-mapped success for purge (deleting a gone session). Safe to retry.
- **Cursor/replay:** `retain` behaves like `suspend` for reconnect (reopen with
  `after`). `purge` invalidates the cursor permanently — a subsequent `session/open`
  with the old `after` returns `CURSOR_OUT_OF_RANGE -32110` (or opens a fresh empty
  session), and the client must drop the app record. The client must therefore treat
  `purge` as terminal for that `session_id`.
- **Invariant conflicts:** close is the one verb that *does* interrupt the active turn
  by default — but via the sanctioned `turn/interrupt` path (terminal event guaranteed,
  side-effects still committed = cancel≠rollback honored). It removes the session from
  `active_turns` only through that path, never by force-abort.

### 3.5 `session/list` — *already exists; extend result metadata (additive)*

`session/list` exists (`methods::SESSION_LIST :1097`, `SessionListParams{} :2828`,
`SessionListResult{sessions:Value} :2834`, handler `:13223`). It returns the durable
catalog (the `GET /api/sessions` `SessionInfo[]`) — i.e. **every persisted session**,
not "the live sessions on this connection." That distinction matters:

- **Which sessions are *open/foreground/background*** is **client-owned** window state
  (`AGENT-OS-ARCHITECTURE.md` §5) — the server should not be the source of truth for it.
- **Which sessions have a *warm runtime* vs are *evicted*, and whether a *turn is
  active*** is **server-owned** and currently *not surfaced* by `session/list`.

So the only gap is server-owned runtime liveness. Fill it **additively** — do not
change the request, add optional per-row fields (unknown-field-compatible with old
clients):
```rust
// Additive fields folded into each SessionInfo row (server-owned truth only):
//   "runtime_state": "active" | "cached" | "evicted"   // from runtime/cache.rs presence + active_turns
//   "active_turn_id": "<uuid>" | null                   // from active_turns map (:10370)
//   "head_cursor": { "stream": "...", "seq": N } | null // ledger head, so a reconnecting
//                                                        //   client can seed replay without re-open
```
Gate the extra fields on `control.session_lifecycle.v1`; clients that didn't negotiate
it get today's exact rows. **The list of "suspended vs live" the task asks for is the
join of client window state (open set) with these server `runtime_state` fields** — the
client computes it; neither side owns it alone.

- **Direction:** client → server (unchanged). Idempotent read. No cursor interaction
  (it is a snapshot, not a ledger stream). Errors unchanged (auth/scope).
- **Server vs client:** the *enumeration* exists; only the *metadata enrichment* is a
  (small, additive) server change.

### 3.6 AMA routing & fan-out hooks

The AMA is a **peer session on the same connection** whose output must be a *typed
decision, not content* (`AGENT-OS-ARCHITECTURE.md` §3.3). Today an AMA "decision" would
arrive as an ordinary `turn/completed` assistant message — content the client would
have to parse out of prose. Phase 3 needs three typed surfaces.

#### 3.6.a `route/proposal` — server→client notification (durable)
The AMA session emits a structured routing proposal on **its own** session stream. Model
it on the existing session-level `SessionOrchestrationEvent` (`:4563`) shape.
```rust
pub const ROUTE_PROPOSAL: &str = "route/proposal";   // add to methods + NOTIFICATION list
// New UiNotification variant: RouteProposal(RouteProposalEvent), method() => ROUTE_PROPOSAL
pub struct RouteProposalEvent {
    /// The AMA session that produced this proposal.
    pub session_id: SessionKey,
    /// Correlates apply/reject and de-dupes retries. Client echoes it back.
    pub request_id: String,
    /// "stay" | "switch" | "open" | "close"  (open set; unknown => ignore)
    pub decision: String,
    /// Target app/session the decision refers to (absent for "stay").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_app_id: Option<String>,
    /// Optional seed text/context for "open" (NOT a system prompt — see app-package
    /// seed blocker, §7 of the architecture doc, out of scope here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    /// [0.0, 1.0]; the client uses this for hold-first-paint / auto-apply thresholds.
    #[serde(default)] pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}
```
```json
{"jsonrpc":"2.0","method":"route/proposal",
 "params":{"session_id":"_main:local:ama","request_id":"r-91","decision":"switch",
           "target_app_id":"travel","confidence":0.82,"cursor":{"stream":"_main:local:ama","seq":57}}}
```
- **Direction:** server→client **notification**, **durable** (ledger-backed, carries a
  `cursor` like every non-delta event `§1.4`) so a proposal that lands during a blip is
  replayed on the AMA session's reconnect and never silently dropped.
- **Idempotency:** the client dedupes by `request_id`; replayed proposals are
  idempotent (apply once). A superseded proposal is simply one the client rejects/ignores.
- **Authority:** advisory only. The client MAY auto-apply above a confidence threshold
  or always require explicit UI; either way *the client decides* (§principle 1).

#### 3.6.b `route/apply` / `route/reject` — client→server (thin ack, advisory)
So the AMA can be rate-limited, learn, and stop re-proposing, the client reports what it
did with a proposal. These are advisory bookkeeping, not commands to the AMA.
```rust
pub const ROUTE_APPLY: &str  = "route/apply";
pub const ROUTE_REJECT: &str = "route/reject";
pub struct RouteAckParams {
    pub session_id: SessionKey,   // the AMA session
    pub request_id: String,       // echoes RouteProposalEvent.request_id
    /// For apply: the session the client actually foregrounded/opened, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_session_id: Option<SessionKey>,
    /// For reject: short reason ("low_confidence"|"user_override"|"rate_limited"|...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
pub struct RouteAckResult { pub recorded: bool }
```
- **Direction:** client→server request/result. Idempotent per `request_id`
  (double-apply records once). No cursor/replay — it is a control ack, not a ledger event.
- **Server role:** feed the AMA's rate limiter / confidence calibration; may throttle
  future proposals (`RATE_LIMITED -32160` is available if the AMA is spamming, though
  the primary defense is server-side rate limiting, not per-call errors).

#### 3.6.c App housekeeping — server→client notifications (durable, permissioned, rate-limited)
Per `AGENT-OS-ARCHITECTURE.md` §7 the app session may *request* (not force) escalation.
Add typed, durable notifications an app agent can emit via a dedicated tool:
```rust
pub const SESSION_NEEDS_FOCUS: &str = "session/needs_focus";
pub const SESSION_IDLE: &str        = "session/idle";
pub const SESSION_STATUS: &str      = "session/status";   // distinct from session/status.get (:1104)
pub struct SessionHousekeepingEvent {
    pub session_id: SessionKey,
    /// "needs_focus" | "idle" | "status"
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Urgency hint for needs_focus; client still decides.
    #[serde(default)] pub priority: u8,
}
```
- **Direction:** server→client notification, **durable** so a `needs_focus` isn't lost
  across reconnect. Advisory — the client is free to ignore.
- **Permissions + rate limits (mandatory):** a compromised/injected app must not spam
  focus or suppress switches (`§7`, `§9` "Self-escalation authority"). These emitters
  live behind a per-session **tool permission** (the app-package tool policy — the
  app-manifest blocker, out of scope for W09) and a **server-enforced rate limit**;
  over-limit emissions are dropped server-side (not surfaced as client errors) and
  counted. This is a server change: a `request_focus` / `emit_status` tool that appends
  the typed durable notification to the session ledger, subject to the rate limiter.

#### 3.6.d Speculative open & cancel — reuse existing verbs (no new methods)
- **Open a session speculatively:** reuse `session/open` (`methods::SESSION_OPEN`).
  "Speculative" is a *client/AMA policy* (open, maybe start a turn, be ready to discard),
  not a wire distinction. The only genuinely new requirement for a *seeded* speculative
  app is the **app-package seed param on `session/open`** — that is the separate Phase-3
  blocker in `AGENT-OS-ARCHITECTURE.md` §7/§13(1) and §10(b), and is **out of scope for
  this control-plane doc**; W09 assumes `session/open` gains that field elsewhere.
- **Cancel a speculative turn:** reuse `turn/interrupt` (`methods::TURN_INTERRUPT`,
  `:949`). It is already idempotent, returns the terminal state
  (`TurnInterruptResult` `:4019`), and honors cancel≠rollback (`§1.6`) — exactly the
  speculative-cancel semantics needed. Both `session_id` and `turn_id` are client-known.
  **No new verb.** The client must still assume already-streamed deltas/side-effects of
  the wrong app may be visible (`§9` "Cancel ≠ rollback") and visually supersede them.
- **Reassign foreground:** `route/proposal` (§3.6.a) → client applies →
  `session/focus` (§3.1, advisory) → `route/apply` (§3.6.b). No verb *forces* foreground.

---

## 4. Server-change vs pure-client-convention

| Addition | Server change | Could be client-only? | Notes |
|---|---|---|---|
| `session/focus` | Optional (no-op ack v1; scheduling later) | **Yes for v1** | Client can foreground locally; verb only earns its keep once scheduling/hold-first-paint exists. |
| `session/detach` | **Yes, small** | No | Needs per-session forwarder abort + subscriber prune (`§1.5`). Client cannot stop server→socket push by itself. |
| `session/suspend` | **Yes, small** | Partly | Detach is the server part; runtime release is a `cache.evict` call. TTL/LRU already evicts, so this is "evict now." |
| `session/close` (retain) | **Yes, small** | No | = interrupt + detach + evict; wiring over existing paths. |
| `session/close` (purge) | **Exists** | — | Delegates to `session/delete` (`:13645`). |
| `session/list` base | **Exists** (`:13223`) | — | No change to enumerate. |
| `session/list` metadata | **Yes, additive** | No (server-owned) | `runtime_state` / `active_turn_id` / `head_cursor` per row. |
| `route/proposal` | **Yes** | No | New durable notification + `UiNotification` variant; the AMA turn must emit typed output. |
| `route/apply` / `route/reject` | **Yes, thin** | No | Feeds the AMA rate limiter; without a server sink it is a client-only no-op (acceptable stub). |
| `session/needs_focus` / `idle` / `status` | **Yes** | No | New durable notifications + an emitter tool + permission + rate limit. |
| Speculative open | **None** | Yes | Reuse `session/open` (+ app-package seed, tracked separately). |
| Cancel speculative turn | **None** | Yes | Reuse `turn/interrupt`. |

---

## 5. New error code (single addition)

Reuse existing codes everywhere possible. One genuinely new condition —
suspend/close against a busy session under `"reject"` — deserves a typed code rather
than an opaque `invalid_request`:
```rust
// octos-core ui_protocol.rs rpc_error_codes (sibling of UNKNOWN_TURN -32101):
pub const SESSION_BUSY: i64 = -32112;   // an active turn blocks this lifecycle op
```
All other failures map to existing codes: `UNKNOWN_SESSION -32100`,
`PERMISSION_DENIED -32120`, `CURSOR_OUT_OF_RANGE -32110`, `RATE_LIMITED -32160`,
`METHOD_NOT_SUPPORTED -32004`. The existing turn-busy string
`invalid_request("a turn is already running for this session")` (`:10398`) should be
migrated to `SESSION_BUSY` for consistency (optional, non-breaking if kept as message).

---

## 6. Invariant interactions (explicit)

- **One active turn per session (`§1.6`).** `detach` leaves the turn running;
  `suspend`/`close` must quiesce it first (`on_active_turn`), and only via
  `turn/interrupt`. No verb spawns or force-aborts a turn outside the sanctioned path.
- **Cancel ≠ rollback (`§1.6`).** `close`/speculative-cancel stop *future* work only;
  streamed deltas + tool side-effects persist. The client-side rule is "visually
  supersede, never assume erasure" (`AGENT-OS-ARCHITECTURE.md` §9).
- **Durable-notification replay (`§1.4`).** Route proposals and housekeeping events are
  durable so reconnect replays them; focus/detach/suspend/close *replies* are RPC
  results and are re-derived by the client on reconnect (re-open sessions, re-assert
  focus), never replayed.
- **Per-session cursor (`W08`, `proto.rs:44`).** Detach/suspend/close(retain) all return
  the head cursor; reattach is `session/open { after: <cursor> }` — the exact W08
  reconnect path, extended from "reconnect" to "reattach a backgrounded app."
- **Scope/auth.** Every verb runs `validate_session_scope`
  (`ui_protocol.rs:10290`) so a profile-scoped connection cannot touch another
  profile's session.

---

## 7. Reconnect / replay matrix

| State at disconnect | Client action on reconnect | Server behavior |
|---|---|---|
| Session attached (foreground/bg, subscribed) | `session/open { after: cursor }` per session | Replay durable events since cursor (`:8581`), respawn forwarder (`:8650`). Ephemeral deltas since cursor are lost by design. |
| Session detached | Re-open only if user returns; else keep record, replay later | Runtime warm; replay from stored cursor when reopened. |
| Session suspended (runtime evicted) | `session/open { after: cursor }` when reopened | `bootstrap` rehydrates from ledger (`cache.rs:254`), then replay. Added latency, same wire. |
| Session closed (retain) | Treat as suspended; reopen on demand | Same as suspend. |
| Session closed (purge) / deleted | Drop app record; do **not** reopen | Old cursor → `CURSOR_OUT_OF_RANGE`; history gone. |
| Route proposal in flight | Deduped by `request_id`; replayed on AMA-session reopen | Durable ledger event, replayed like any notification. |

---

## 8. Open questions / risks

- **OQ1 — Is `session/focus` worth a server verb at all?** If scheduling/hold-first-paint
  is not implemented in Phase 3, focus is 100% client-side and the verb is dead weight.
  Recommend: reserve the method name, implement as no-op ack, defer real behavior. (Ties
  to `AGENT-OS-ARCHITECTURE.md` §15 D4.)
- **OQ2 — suspend vs detach+release.** Collapsing `session/suspend` into
  `session/detach { release_runtime }` removes a verb at the cost of a fatter detach.
  Given TTL/LRU already evicts, suspend's marginal value is small; decide before
  implementing.
- **OQ3 — AMA proposal transport.** `route/proposal` assumes the AMA agent can emit a
  *typed* notification rather than assistant content. That needs a server-side emitter
  (a tool or a structured turn output). If we instead parse JSON out of the AMA's
  assistant message, we reintroduce the "content in translation" fragility §3.3 warns
  against. Prefer the typed emitter.
- **OQ4 — Housekeeping abuse.** `needs_focus`/`idle`/`status` are a spam/suppression
  vector (`§9`). They are inert until per-app tool permissions + server rate limits
  exist (the app-manifest blocker). Do not ship the emitter tool before the rate limiter.
- **OQ5 — Detach and background turn completion cost.** A detached app whose turn
  completes still writes the ledger and (briefly) holds a runtime; N background turns
  can still cost tokens/CPU. detach bounds *bandwidth*, not *compute* — compute budgets
  are a separate Phase-4 concern (`§9` "Cost").
- **Risk — cursor invalidation on purge.** Clients must reliably distinguish
  `close(retain)` from `close(purge)`/`delete` and drop records on purge, or they will
  retry `session/open` with a dead cursor and loop on `CURSOR_OUT_OF_RANGE`.
- **Risk — one active turn vs re-route.** Re-routing to an app that is already mid-turn
  hits `SESSION_BUSY`; the client needs an explicit queue/reject policy (`§9` "One active
  turn per session"), which is client-side but must be designed alongside these verbs.

---

## 9. Suggested implementation sequencing

1. **Feature flags + method constants + docs.** Add
   `control.session_lifecycle.v1` / `control.routing.v1`, the `session/{focus,detach,
   suspend,close}` + `route/*` + `session/{needs_focus,idle,status}` constants, register
   them in `UI_PROTOCOL_COMMAND_METHODS` / `UI_PROTOCOL_NOTIFICATION_METHODS`, and add
   `SESSION_BUSY -32112`. Pure additive, no behavior. (host-testable)
2. **`session/detach` (the keystone).** Single-session slice of `abort_live_forwarders`
   (`:5346`) + head-cursor read. Client: `OutboundCommand::DetachSession` + store the
   returned cursor. This alone delivers bounded live fan-out — the W08 no-detach cost fix.
3. **`session/close(retain/purge)` + `session/suspend`.** close(retain) = interrupt +
   detach + `cache.evict`; close(purge) wraps `session/delete`; suspend = detach +
   evict with `on_active_turn`. Reuses (2) + existing interrupt/delete paths.
4. **`session/focus` no-op ack.** Wire method + connection-local foreground marker;
   scheduling deferred.
5. **`session/list` metadata.** Additive `runtime_state`/`active_turn_id`/`head_cursor`
   join over the cache + `active_turns` maps in the existing handler (`:13223`).
6. **AMA routing surface.** `route/proposal` notification + `UiNotification` variant
   (durable), then `route/apply`/`route/reject` acks, then the AMA rate limiter sink.
7. **App housekeeping (last, gated on permissions).** `needs_focus`/`idle`/`status`
   notifications + emitter tool + per-app permission + rate limit — only after the
   app-manifest/permission work lands.

Steps 1–5 are the client-facing multi-window control plane (correctness-critical,
host-testable). Steps 6–7 are the AMA autonomy layer and depend on the app-package seed
+ manifest blockers tracked separately in `AGENT-OS-ARCHITECTURE.md` §13.
