# M9 Stabilization — Workstream Index

Supervisor's master plan for closing the gaps the M9 code review (`m9-review/00-VERDICT.md`)
identified. **Goal:** make `coding-green-m9-local-20260428` honestly stable, with wire-level
test evidence to back the claim. **Scope:** ten focused fix workstreams, designed to be
executed in parallel by the agent swarm with minimal cross-workstream contention.

## Headline

| | |
|---|---|
| Branch | `coding-green-m9-local-20260428` (HEAD `0b8c8434` at 2026-04-28) |
| Working dir | `/Users/yuechen/home/octos` |
| Reviews referenced | `~/home/octos-app/m9-review/00-VERDICT.md` + `01..05` |
| Total workstreams | 10 (`M9-FIX-01..10`) |
| Blocker fixes | M9-FIX-01, 02, 03, 06, 09 |
| Serious | M9-FIX-04, 05, 07, 08 |
| Cleanup | M9-FIX-10 |
| Estimated days (sequential) | ~15 dev-days |
| With swarm parallelism | ~5 calendar days, peak 4 agents |

## Workstream catalog

| ID | Issue | Title | Sev | Estimated | Wave |
|---|---|---|---|---|---|
| M9-FIX-01 | [#639](https://github.com/octos-org/octos/issues/639) | `progress/updated` notification decode + serde fallbacks | blocker | 0.5d | 1 |
| M9-FIX-02 | [#640](https://github.com/octos-org/octos/issues/640) | Spec §10 error code parity | blocker | 1d | 1 |
| M9-FIX-03 | [#641](https://github.com/octos-org/octos/issues/641) | `turn/interrupt` TOCTOU race + spec semantics | blocker | 1.5d | 2 |
| M9-FIX-04 | [#642](https://github.com/octos-org/octos/issues/642) | WS send-error handling + backpressure indicator | serious | 2d | 3 |
| M9-FIX-05 | [#643](https://github.com/octos-org/octos/issues/643) | M9.6 ledger persistence + eviction | serious | 3d | 4 |
| M9-FIX-06 | [#644](https://github.com/octos-org/octos/issues/644) | `approval_scope` enforcement | blocker | 1.5d | 2 |
| M9-FIX-07 | [#645](https://github.com/octos-org/octos/issues/645) | Approval audit trail + `approval/decided` event | serious | 1.5d | 3 |
| M9-FIX-08 | [#646](https://github.com/octos-org/octos/issues/646) | `turn/interrupt` drains pending approvals | serious | 1d | 3 |
| M9-FIX-09 | [#647](https://github.com/octos-org/octos/issues/647) | Wire-level e2e harness + fault injection | blocker | 2d | 1 |
| M9-FIX-10 | [#648](https://github.com/octos-org/octos/issues/648) | Risk + path sanitization | minor | 1d | 2 |

Each `M9-FIX-NN.md` carries the full spec for one workstream.

## Wave plan (file-conflict-free parallelism)

The workstreams are grouped so that within a wave, no two agents touch the same file.
Across waves, the supervisor merges + rebases between rounds.

```
Wave 1 (parallel, ~3 agents):
  M9-FIX-01    octos-core/src/ui_protocol.rs    (enum + serde)
  M9-FIX-02    octos-core/src/ui_protocol.rs    (error codes — different sections)
                  ⚠ light overlap: both edit the same file but different regions.
                  Strategy: 01 lands first (smaller), 02 rebases.
  M9-FIX-09    e2e/tests/                       (entirely new files; no conflict)

Wave 2 (parallel, ~3 agents) — after Wave 1 merges:
  M9-FIX-03    octos-cli/src/api/ui_protocol.rs (turn/interrupt fn)
  M9-FIX-06    octos-cli/src/api/ui_protocol_approvals.rs (scope plumbing)
  M9-FIX-10    octos-cli/src/api/ui_protocol.rs (materialize_file_mutation_diff fn)
                  ⚠ 03 + 10 same file, different functions; rebase order 03 → 10.

Wave 3 (parallel, ~3 agents) — after Wave 2 merges:
  M9-FIX-04    octos-cli/src/api/ui_protocol.rs (send/backpressure cross-cutting)
  M9-FIX-07    ui_protocol_approvals.rs + ui_protocol.rs (audit + new event)
                  ⚠ 04 + 07 share ui_protocol.rs but different concerns; rebase.
  M9-FIX-08    ui_protocol.rs + ui_protocol_approvals.rs (interrupt-drain bridge)
                  ⚠ touches both — Wave 3 last; rebases on 04 + 07.

Wave 4 (single, ~1 agent) — after Wave 3 merges:
  M9-FIX-05    ledger architecture rewrite — largest scope, runs alone.
```

Total wall time with full swarm: 5 calendar days assuming agents finish their wave in 1
day and the supervisor merges overnight.

## Branch + worktree convention

Each workstream gets its own dedicated branch and worktree. Naming:

```
branch:    fix/m9-NN-<slug>           (e.g. fix/m9-01-progress-updated-enum)
worktree:  ~/home/octos-m9-fix-NN     (sibling to ~/home/octos)
base:      coding-green-m9-local-20260428
```

Setup script (run from `~/home/octos`):

```bash
for n in 01 02 03 04 05 06 07 08 09 10; do
  branch="fix/m9-${n}-stub"
  wt="$HOME/home/octos-m9-fix-${n}"
  [ -d "$wt" ] && continue
  git worktree add "$wt" -b "$branch" coding-green-m9-local-20260428
done
```

Each agent's brief instructs it to `cd ~/home/octos-m9-fix-NN`, work there, and
commit on the branch. The supervisor merges by `git fetch` + cherry-pick or PR-style
review against the worktree branch.

## Acceptance gate for "M9 is stable"

The branch carries the "stable" claim only when:

1. All five blocker workstreams (01, 02, 03, 06, 09) have landed and been verified.
2. M9-FIX-09 e2e harness runs in CI as a blocking check on protocol-touching PRs.
3. The four serious workstreams (04, 05, 07, 08) are landed OR explicitly deferred
   with a documented "known limitation" entry in the spec.
4. M9-FIX-10 cleanup has landed (low-effort, no excuse).
5. The "Live wire-level smoke" suite passes against three live e2e servers
   (mini1/mini2/mini3) for 24 consecutive hours with zero false alarms.

Until all five hold, the branch is **release-candidate**, not stable.

## Coordination & risks

- **Dirty working tree at supervision start.** `~/home/octos` has uncommitted
  modifications to `crates/octos-core/src/ui_protocol.rs`. Worktrees branch from
  a commit and ignore the source tree's working state, so they're not affected,
  but the dirty changes should be stashed/committed before merging fixes back.
  Documented in this index; supervisor confirms with user before merge wave.
- **No live server in CI.** M9-FIX-09 lands the wire harness; running it
  requires a real `octos serve` instance. CI has none today. Two options:
  (a) the new harness boots its own short-lived `octos serve` per test run, or
  (b) keep it in nightly only. Workstream 09 picks one with reasoning.
- **Spec drift.** If `OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md` evolves while
  fixes are in flight, the workstream spec citations may go stale. Supervisor
  re-runs the spec-vs-code diff before each merge.
- **Server team review.** The fixes are Rust + protocol contract; landing them
  on the M9 branch should ideally route through the server team. The supervisor
  drafts PRs from the worktree branches but does NOT push or open PRs without
  explicit owner go-ahead.

## What this index tracks

| Workstream | Status | Worktree | Branch | Last update |
|---|---|---|---|---|
| M9-FIX-01 | spec drafted | not yet created | — | 2026-04-28 |
| M9-FIX-02 | spec drafted | not yet created | — | 2026-04-28 |
| M9-FIX-03 | spec drafted | not yet created | — | 2026-04-28 |
| M9-FIX-04 | spec drafted | not yet created | — | 2026-04-28 |
| M9-FIX-05 | spec drafted | not yet created | — | 2026-04-28 |
| M9-FIX-06 | spec drafted | not yet created | — | 2026-04-28 |
| M9-FIX-07 | spec drafted | not yet created | — | 2026-04-28 |
| M9-FIX-08 | spec drafted | not yet created | — | 2026-04-28 |
| M9-FIX-09 | spec drafted | not yet created | — | 2026-04-28 |
| M9-FIX-10 | spec drafted | not yet created | — | 2026-04-28 |

Supervisor updates this table after each wave completes. When all rows show
`landed + verified`, the M9 stabilization sweep is done.
