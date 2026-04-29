# M9 Review — Server Handler (`crates/octos-cli/src/api/ui_protocol.rs`)

## Verdict: **AMBER, leaning RED**

Structure is reasonable and contract stores are well-tested in isolation. The
runtime layer (turn lifecycle, registry, ledger) has multiple race windows,
unbounded process-global state, no WS integration tests, and silently swallows
nearly every send-side error. "M9 isn't stable" is well-founded.

## Top concerns (most damaging first)

1. **Process-global, never-evicted ledger.** `event_ledger()` (147–152) is a
   `OnceLock<Arc<UiProtocolLedger>>` with `EVENT_LEDGER_RETAINED_PER_SESSION =
   1024` (46). Inside: `HashMap<SessionKey, SessionLedger>` under
   `std::sync::Mutex` (`ui_protocol_ledger.rs:73`). **Sessions never
   removed.** Every `SessionKey` ever observed retains up to 1024 events for
   process lifetime. No compaction/TTL/LRU. Restart wipes it (breaks replay
   across restart); during a long daemon it grows unboundedly in session
   count. Largest single stability risk.

2. **`active_turns` and `contract_stores` are also global singletons**
   (133–145). Stale `Responded` `ApprovalEntry` rows live forever;
   `pending_for_session` (`ui_protocol_approvals.rs:137`) walks every entry
   ever recorded each reconnect.

3. **#634 race not structurally prevented, only accidentally absent.** In
   `run_standalone_turn` (1311–1340), agent task writes JSONL inside
   `sessions.lock().await` then sends `done`; outer loop emits
   `TurnCompleted` (1380). Writes precede `done` today only because the
   channel is FIFO and the agent is the sole `done` producer. No explicit
   "JSONL committed → `turn/completed`" invariant; any refactor that buffers
   progress, moves the write off the agent task, or parallelizes the final
   emit reopens #634. If the connection drops between JSONL commit and the
   `TurnCompleted` send, the message is durably persisted but no
   `turn/completed` is ever emitted.

4. **`turn/interrupt` is NOT idempotent** despite spec § requiring "explicit
   and idempotent". Lines 1091–1110: stale-turn returns `interrupted:false`
   (indistinguishable from "no such turn"); mismatched `turn_id` against a
   different active turn returns hard `invalid_params`. Line 1118 emits a
   synthetic `TurnError` only when the turn was still active — TOCTOU where
   the turn naturally completes between lookup (1093) and remove (1109)
   yields **both `turn/completed` and `turn/error`** for the same `turn_id`.

5. **#632/#636 — not present in WS today, structurally fragile.**
   `TurnStarted` is appended before the agent task spawns (1251);
   `progress_context` is created with `turn_id` from `params` (1357), so
   `turn_id` binds on every progress-derived notification.
   `UiProtocolApprovalRequester` (1294) captures `turn_id` once. The
   late-binding pattern that caused #632 isn't triggered, but no test pins
   the invariant — a refactor reopens it.

6. **Send errors silently swallowed.** ~40 `let _ = send_*(...).await`
   sites; `send_json` collapses serde + WS errors to `Result<(), ()>`
   (1699–1706). Asymmetric TCP failure (writer broken, reader alive) keeps
   the agent running and writing JSONL for a client that will never see
   results. No "WS dead → abort turn" path except via `ws_rx.next()` (302).

7. **No backpressure — silent drops.** `BoundedChannelReporter::report`
   uses `try_send` (180) on a 1024-bound channel. Slow client → events
   dropped, no indicator on the wire. `turn/completed` delivered as if
   nothing missed; replay shows gaps (drops were never appended).

8. **Resource leak on connection drop.** `_abort_guard: AbortOnDrop` (1351)
   aborts only the agent task, not the outer `run_standalone_turn` future.
   If the outer is dropped, the agent is killed but `clear_active_turn`
   (1606) never runs — registry holds an entry with a dead `AbortHandle`.
   With concern 2, this leaks per disconnect-cycle.

9. **`ws_tx` lock held across `send().await`** (1704). One slow send stalls
   every concurrent send — approval RPC results queue behind unrelated
   notifications. A stuck client wedges the session.

10. **`expect("...poisoned")` on every store access** (`ui_protocol_approvals.rs`
    44/83/100/122/144/159/177; ledger 97/126). Stores are global — one panic
    poisons the mutex for the process.

## Race / deadlock candidates

- `active_turns` → `connection_turns` lock-order: `clear_active_turn` (1606)
  drops between; `abort_connection_turns` (1630) takes reverse. Safe via
  explicit drop, fragile to refactor.
- Sessions mutex held across awaits + sync I/O (607, 1271, 1315). Two
  concurrent turns on different sessions serialize on one mutex — same
  shape as the 5–9 s lag in #634.
- Replay-vs-live boundary: ordering preserved only because `SessionOpened`
  is appended (615) between `replay_after` (586) and live emission.
  Narrowly acceptable.
- All locks are unbounded `await` — no try-lock, no timeout. Poisoned
  global mutex is fatal forever.

## Issue status in the WS path

- **#634**: not triggered (writes precede `done`); invariant implicit, not
  enforced. Pin with `done_event_emits_after_jsonl_commit`.
- **#632/#636**: not present — `turn_id` binds before any emission. No test
  pins this; late-binding refactor would re-introduce.

## Strengths

- Clean module boundaries (ledger / approvals / diff / progress).
- Typed cursor errors (`cursor_stream_mismatch`, `cursor_expired`) with
  structured `data`.
- `validate_session_scope` (435) thorough, well-tested.
- Frame-size limit (43) + explicit parse-error path.
- Pending-approval replay on reconnect (596) — real durability story.
- Typed JSON-RPC error codes with `kind` discriminators.

## Suggested fixes (narrow, actionable)

1. **Bound the ledger.** Add `last_touched_at`, idle TTL (~1 h), compaction
   task; or LRU-cap `SessionKey` count. Same for approvals/diff previews.
2. **Persist ledger (or high-water cursor)** so `after` works across
   restart; today restart breaks every open session's replay.
3. **Make `turn/interrupt` idempotent.** Bounded ring of recently-completed
   `(session_id, turn_id)` → return `interrupted:false,
   status:"already_completed"`. Drop the synthetic `TurnError` on
   interrupt-vs-completion race (1109–1118).
4. **Promote send failure to turn abort** — abort the agent task when
   `send_notification` fails on the active turn's sink.
5. **Replace `try_send` with backpressure** — `send().await`, or count drops
   and emit `Warning` so the client knows the stream is incomplete.
6. **Replace `expect("...poisoned")` with `internal_error` RPC frames.**
7. **Add WS integration tests.** None today. Cover: replay-after-reconnect,
   double `turn/start`, `turn/interrupt` after natural completion, approval
   survives disconnect, ledger eviction, slow-client backpressure,
   agent-panic emits `turn/error`, JSONL-committed-before-`turn/completed`.
8. **Per-session task with own write queue** — pull JSONL writes off the
   global sessions mutex.

## Open questions for the author

- Is the global ledger by design, or was per-connection considered? Eviction
  policy?
- `turn/interrupt` after natural completion: spec says idempotent. Current
  behavior (`invalid_params` for stale id, double-emit on race) is neither.
  Intended?
- Silent drop on the progress channel — intentional? Documented to clients?
- If the agent panics, `recv() -> None` falls through with no
  `turn/completed` or `turn/error`. Add `code:"agent_panic"` fallback?
- Why hardcode `EVENT_LEDGER_RETAINED_PER_SESSION = 1024`? A tool-heavy
  turn can clip the start of the current turn from replay.
- `unreachable!` at 624 — refactor of `append_notification`'s return shape
  panics the connection.
