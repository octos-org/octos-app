# M9-FIX-04 — WS send-error handling + backpressure indicator

| | |
|---|---|
| Severity | Serious |
| Wave | 3 |
| Files | `crates/octos-cli/src/api/ui_protocol.rs` |
| Branch | `fix/m9-04-send-backpressure` |
| Worktree | `~/home/octos-m9-fix-04` |
| Estimated | 2 dev-days |
| Conflicts | Wave 3 with M9-FIX-07 (same file, different concerns) |

## Problem

From `m9-review/02-server-handler.md`:

> ~40 `let _ = send_*().await` call sites silently swallow WS send errors.
> Asymmetric TCP failure leaves the agent running and writing JSONL for a client that never sees results.
>
> Silent backpressure via `try_send` (line 180) drops events with no on-wire indicator.
> `turn/completed` is delivered as if nothing was missed; replay shows gaps because
> dropped events were never appended.
>
> `ws_tx` lock held across `send().await` (line 1704) — a slow client wedges all RPC and
> notification traffic for that connection.

Three distinct but related issues in the WS send path:

1. **Errors swallowed.** Send failures go unnoticed; turn keeps progressing.
2. **Backpressure invisible.** Drops happen silently; replay-via-cursor lies (the cursor implies durability).
3. **Slow-client wedge.** Holding the send lock across the network await means one slow consumer blocks unrelated traffic.

## Acceptance criteria

1. **Structured send-error handling.** All `send_*().await` call sites use a typed result. Three policies:
   - **Lifecycle (turn lifecycle, errors, RPC results):** errors propagate up, turn aborts cleanly, ledger entry marked `delivery_failed`.
   - **Notifications (durable):** errors logged at WARN with `tracing::warn!(method, code, "ws send failed")`, ledger still records (so replay catches up).
   - **Notifications (ephemeral, e.g., `message/delta`):** errors logged at DEBUG, dropped silently — these are explicitly non-durable per spec.
2. **Backpressure indicator on the wire.** When `try_send` would drop a durable notification, instead emit a `protocol/replay_lossy { session_id, dropped_count, last_durable_cursor }` notification. Clients can react (refetch via REST snapshot).
3. **No lock held across `await`.** `ws_tx` is converted to either:
   - An `Arc<Mutex<WsSink>>` where the lock is acquired, the message is written to a per-connection bounded channel, then released — and a separate background task drains the channel into the actual socket.
   - Or a separate writer task with `mpsc::channel<WsMessage>` from any source; the lock-then-await pattern is gone.
4. **Tests:**
   - `send_error_propagates_for_lifecycle_messages`
   - `send_error_logged_for_durable_notifications`
   - `slow_client_does_not_wedge_other_connections`
   - `bounded_channel_full_emits_replay_lossy`

## Files & lines

- `crates/octos-cli/src/api/ui_protocol.rs:180` — `try_send` call site.
- `crates/octos-cli/src/api/ui_protocol.rs:1704` — `ws_tx` lock + await.
- `~40 sites`: grep `let _ = .*\.send.*await` in this file.

## Implementation guide

Recommended structure (sketch):

```rust
struct WsConnection {
    writer: mpsc::Sender<WsMessage>,
    metrics: Arc<ConnectionMetrics>,
}

impl WsConnection {
    fn send_durable(&self, msg: UiNotification, cursor: UiCursor) -> Result<(), SendError> {
        let n = WsMessage::Notification(msg, Some(cursor));
        match self.writer.try_send(n) {
            Ok(_) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.metrics.record_drop();
                self.emit_replay_lossy_async();   // best-effort but typed
                Err(SendError::BackpressureDrop)
            }
            Err(TrySendError::Closed(_)) => Err(SendError::Closed),
        }
    }

    fn send_ephemeral(&self, msg: UiNotification) -> Result<(), SendError> {
        match self.writer.try_send(WsMessage::Notification(msg, None)) {
            Ok(_) => Ok(()),
            Err(TrySendError::Full(_)) => Err(SendError::BackpressureDrop),  // silent — caller knows it's ephemeral
            Err(TrySendError::Closed(_)) => Err(SendError::Closed),
        }
    }
}

// dedicated writer task
async fn ws_writer_loop(mut sink: WsSink, mut rx: mpsc::Receiver<WsMessage>) {
    while let Some(msg) = rx.recv().await {
        if sink.send(msg.into_frame()).await.is_err() { break; }
    }
}
```

Channel capacity: 1024 messages per connection (tunable per session size). On full, `replay_lossy` is emitted opportunistically (also via `try_send` — if even that's full, log and continue; client will diverge cursor and re-hydrate via REST).

## Notes

- M9-FIX-05 (ledger persistence) is independent but synergistic: replay relies on the ledger having the durable notification even when the wire dropped it. With both fixes, `replay_lossy` recovery is correct.
- Metrics: count `ws.send.error.lifecycle`, `ws.send.error.durable`, `ws.send.drop.backpressure`, `ws.send.drop.closed`. Gauge: `ws.connection.queue_depth` per session. (This fixes the "no observability for slow clients" gap from review 02.)

## Tests

```rust
#[tokio::test]
async fn slow_client_does_not_wedge_other_connections() {
    /* boot two mock WS clients; client A read-pauses for 30s.
       Verify client B receives notifications during that window. */
}

#[tokio::test]
async fn bounded_channel_full_emits_replay_lossy() {
    /* fill writer channel by pausing the sink; emit 2000 durable notifications;
       verify a `protocol/replay_lossy` is queued before the channel re-drains */
}
```

## Out of scope

- Reconnect / resume on the client side. The protocol gives clients enough to recover (cursor + replay_lossy notification). Client work is in `octos-app`.
- Per-message rate limiting (separate concern).

## Implementer briefing

1. `cd ~/home/octos-m9-fix-04`. Wait for M9-FIX-03 to land (rebase if needed).
2. Read all `let _ = send_*` sites in `ui_protocol.rs`. Categorize each as lifecycle / durable / ephemeral.
3. Define `SendError` and a typed `WsConnection` wrapper. Replace `Arc<Mutex<WsSink>>` direct-await pattern with channel + writer-task.
4. Add `protocol/replay_lossy` to `UiNotification` (use the M9-FIX-01 pattern). Coordinate with M9-FIX-01 reviewer if this conflicts.
5. Update every send site to use the typed wrapper.
6. Add the three tests above plus per-policy unit tests.
7. Update `crates/octos-core/src/ui_protocol.rs:333+` to register `protocol/replay_lossy` method name.
8. `cargo test -p octos-cli --lib` and `cargo check --workspace` clean.

Constraints:
- ≤ 600 LOC new + modified.
- Zero behavior change for happy-path traffic — only failure modes change.
- `cargo clippy` clean.

When done: status note listing every send site categorized + the replay_lossy trigger criteria.
