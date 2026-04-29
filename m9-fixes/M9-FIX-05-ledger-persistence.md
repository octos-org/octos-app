# M9-FIX-05 — M9.6 ledger persistence + eviction

| | |
|---|---|
| Severity | Serious |
| Wave | 4 (alone) |
| Files | `crates/octos-cli/src/api/ui_protocol_ledger.rs` (and ui_protocol.rs touch points) |
| Branch | `fix/m9-05-ledger-persistence` |
| Worktree | `~/home/octos-m9-fix-05` |
| Estimated | 3 dev-days |
| Conflicts | Touches both core handler files; runs alone in Wave 4. |

## Problem

From `m9-review/02-server-handler.md`:

> Three process-global, never-evicted singletons (ledger, active turns, contract stores).
> The ledger is `OnceLock<Arc<UiProtocolLedger>>` storing 1024 events per session, with no
> compaction/TTL/LRU and no persistence — sessions accumulate forever during a long-running
> daemon and replay breaks across restart.

Effects:

- Memory leak: every session ever opened keeps its ledger forever.
- Restart loses replay: cursors persisted by clients become invalid; clients must REST-hydrate.
- Approvals (M9.2) "survive reconnect" only as long as the daemon doesn't restart.
- Production fleet validation is essentially "hope nothing restarts."

## Acceptance criteria

This is the largest single fix in the M9 stabilization sweep. Pick **one** of two paths
based on team capacity. Both paths share the eviction + observability components.

### Path A — durable backing (recommended for "stable")

Persist the ledger to disk per-session:

1. **On-disk format**: append-only log per session at `<data_dir>/ui-protocol/<session_id>/ledger-<epoch>.log`. JSON-Lines or bincode (pick one; document).
2. **Recovery**: at daemon startup, scan `<data_dir>/ui-protocol/`; for each session dir, replay the log into the in-memory ledger. Bound recovery: only the last N (4096?) events per session. Older log files truncated/rotated.
3. **Write-ahead**: every durable notification appends to disk before signaling the wire. (Closes #634-style "emit before commit" race for the ledger path.)
4. **Eviction**: LRU on in-memory cache; disk log is the source of truth.
5. **Cursor validity across restart**: a cursor from before restart resolves correctly if the log range still covers it; otherwise returns `CURSOR_OUT_OF_RANGE`.

### Path B — explicit "lost on restart" contract

Document and enforce that ledgers are RAM-only and lost on restart:

1. Add a server capability `ledger.durable.v1: false` advertised in `session/open` response.
2. Clients receiving `false` must treat any post-restart cursor as invalid and re-hydrate via REST.
3. Add LRU + TTL eviction on the in-memory ledger:
   - Per-session ring buffer of 1024 events (already there) PLUS
   - Session eviction after 1 hour of inactivity, OR if total active sessions exceed 1024.
4. Document in spec § 9 that ledger durability is a capability, not a guarantee.

### Eviction (both paths)

- Per-session ring buffer cap: configurable, default 4096 events.
- Total session cap: configurable, default 1024 active.
- Idle eviction: configurable, default 1 hour of no activity.
- Counters: `ledger.sessions.active`, `ledger.sessions.evicted`, `ledger.events.dropped` (cap reached), `ledger.bytes.in_memory`, `ledger.bytes.on_disk` (Path A only).

### Tests

- `ledger_per_session_capacity_enforced`
- `ledger_idle_session_evicted_after_ttl`
- `ledger_active_session_cap_enforced`
- Path A: `ledger_recovers_after_simulated_restart`
- Path A: `ledger_disk_log_rotates_on_size_threshold`
- Path B: `ledger_durability_capability_advertised_correctly`

## Files & lines

- `crates/octos-cli/src/api/ui_protocol_ledger.rs` — main work.
- `crates/octos-cli/src/api/ui_protocol.rs:133–152` — singleton declarations.
- `crates/octos-cli/src/api/ui_protocol.rs:586` — `replay_after` (cursor resolution path).
- `crates/octos-core/src/ui_protocol.rs` — capability flag if Path B is chosen.

## Recommendation

**Path A** if the M9 work is moving toward a "production-ready" claim within 2 weeks.
**Path B** if the team accepts ledger volatility for now and prefers smaller scope. Path B
is honest and a strict improvement over status quo (which advertises durability without
delivering it).

Document the choice + rationale in the spec.

## Out of scope

- Distributed / cross-process ledger (sharing across multiple daemon instances). This stays single-process.
- Encryption-at-rest of the ledger. Add a separate workstream if compliance requires.
- Per-tenant ledger quotas. Likely needed for SaaS deployments; deferred.

## Implementer briefing

1. `cd ~/home/octos-m9-fix-05`. Wait for Wave 3 to merge.
2. Pick Path A or Path B; document the choice in a 1-page ADR at `~/home/octos/docs/M9-LEDGER-DURABILITY-ADR.md`.
3. Implement eviction first (both paths share it).
4. Implement persistence (A) or capability (B).
5. Add observability counters.
6. Tests per the list above. Run a 1-hour soak: spam 10K events across 10 sessions, restart, verify recovery.
7. Update spec § 9 to document the chosen contract.

Constraints:
- ≤ 1200 LOC new (Path A) or ≤ 400 LOC (Path B).
- No regression in latency for happy-path traffic. Profile a turn round-trip before/after; <5% delta.
- `cargo fmt` + `cargo clippy --workspace -- -D warnings` clean.

When done: ADR document + status note with chosen path + soak-test results.
