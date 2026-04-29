# M9 Branch — Scope and Stability Review (`coding-green-m9-local-20260428`)

## Headline

Branch `coding-green-m9-local-20260428` in `/Users/yuechen/home/octos` is a 17-commit, 280-file diff against `main`. By `git diff --stat`: **27,596 insertions / 48,764 deletions** (net **-21,168 lines**), but `git log --numstat` shows **30,252 added vs 620 deleted** real production deltas — the gap between the two is dominated by the deletion of `swarm-app/` (≈ 6,289 lines, 15 files including `package-lock.json` at 4,232 lines and a removed React app) and the rewrite/shrinkage of `octos-bus/api_channel.rs` (-2,620), `octos-bus/session.rs` (-1,712), and `octos-agent/src/agent/loop_runner.rs` (-1,200-ish). The bulk of net-new code (≈ +20 kLOC) is concentrated on the AppUi/UI Protocol v1 subsystem and its server handlers. **All 17 commits are date-stamped within the last 5 days (2026-04-24 → 2026-04-28)**, with **11 of them landed on 2026-04-28 alone** — most after a single large mega-commit `70c756d Add coding green M9 protocol and sandbox work`. The branch is genuinely fresh; over a third of last-day commits are TUI/tmux harness fix-forwards layered on top of that mega-commit. Test coverage on the new code is moderate (test:code = 0.22).

## Change Inventory by Area

| Area | Files | Lines (touched) | Notes |
|---|---:|---:|---|
| Agent runtime (`crates/octos-agent/`) | 76 | 20,896 | Includes large rewrites of `agent/execution.rs` (1,757), `agent/loop_runner.rs` (1,293), and net-new `permissions.rs` (+494), `network_policy.rs` (+258), `fs_policy.rs` (+220) |
| Server handlers (`crates/octos-cli/src/api/`) | 18 | 9,234 | Net-new `ui_protocol*.rs` family: `ui_protocol.rs` 2,949, `ui_protocol_progress.rs` 573, `ui_protocol_task_output.rs` 532, `ui_protocol_approvals.rs` 465, `ui_protocol_diff.rs` 395, `ui_protocol_ledger.rs` 328 |
| Other crates (`crates-other`) | 33 | 5,354 | Pipeline executor, supervisor, etc. |
| `swarm-app/` (deleted) | 15 | 6,289 | Removed (incl. a 4,232-line lockfile) |
| Other CLI (`crates/octos-cli/src/` non-api) | 22 | 5,836 | `session_actor.rs` 3,280 churned (now 7,498 LOC), `swarm.rs` removed (-1,891) |
| Docs / API spec (`docs/`, `api/`) | 14 | 5,004 | All net-new specs/handoff docs incl. `OCTOS_UI_PROTOCOL_V1_SPEC` 483, `M9_ISSUE_STACK` 1,111, `M9_SANDBOX_PARITY_ADDENDUM` 698, `OCTOS_TUI_CODEX_TMUX_LIVE_COMPARISON_RUNBOOK` 489 |
| Bus / session (`crates/octos-bus/`) | 4 | 4,764 | Heavy deletion: `api_channel.rs` shrunk by ≈ 2,620, `session.rs` by ≈ 1,712; `resume_policy.rs` rewritten (180 lines touched) |
| E2E tests (`e2e/`) | 26 | 3,942 | 10+ net-new `live-*.spec.ts` plus `e2e/tmux/run.sh` (370) and Rust fixtures |
| Core protocol (`crates/octos-core/`) | 4 | 3,155 | Net-new `ui_protocol.rs` (2,718) and `app_ui.rs` (349) |
| Scripts (`scripts/`) | 12 | 3,117 | tmux harnesses, see §5 |
| App skills | 4 | 1,723 | `deep-search/src/main.rs` -1,436 (deletion-heavy) |
| Sandbox (`crates/octos-sandbox/`) | 1 | 17 | Tiny touch — sandbox impl actually lives under `octos-agent/src/sandbox/` (see `sandbox/macos.rs` ±64, `sandbox/windows.rs` ±263, `sandbox/mod.rs` ±206) |
| Other / infra | 51 | 7,029 | `.github/workflows/ci.yml` (239), Cargo.lock churn, dashboard, docker, etc. |

Final `git diff --stat` line: `280 files changed, 27596 insertions(+), 48764 deletions(-)`.

## M9 Sub-Ticket Status (`docs/OCTOS_M9_ISSUE_STACK_2026-04-24.md`, 1,111 lines)

The doc's own roll-up at line 87–98 declares:

| Bucket | Count | Items |
|---|---:|---|
| Signed off (current coding-green scope) | 24 | M9.1–M9.7, M9.9–M9.25 |
| Scoped web coding app signed off | 1 | M9.8A |
| Held by product decision | 1 | broad M9.8 |
| Implementation open | 0 | — |
| Validation pending | 2 | strict M9.24 chat-first rerun; M9.26 npm/network recovery live case |

Per-ticket status markers (`docs/OCTOS_M9_ISSUE_STACK_2026-04-24.md`):
- M9.1 (UI Protocol v1) — implemented for all six commands (line 171)
- M9.2 (Approval) — implemented for UI Protocol turns (206)
- M9.3 (Diff preview) — implemented (232)
- M9.4 (Task output tail/read) — implemented; runtime disk-output caveat (260)
- M9.5 (Rich progress schema) — implemented (290)
- M9.6 (Unified ledger) — implemented as bounded in-memory ledger (316)
- M9.7 (octos-tui MVP) — sibling repo `/Users/yuechen/home/octos-tui` (350)
- M9.8 (broad web migration) — held (378)
- M9.9 (tmux harness) — implemented, all 7 sub-issues signed off (420)
- M9.10–M9.13 (sandbox parity) — implementation+test evidence (line 103)
- M9.14, M9.15 — typed approval panels / Windows AppContainer probe (105–108)
- M9.16 (skill budget gate) — first impl landed (488)
- M9.17 (Codex-class TUI) — implemented for release slice (549); UPCR-2026-002 accepted
- M9.18–M9.25 — implemented per per-ticket markers (599, 635, 685, 723, 762, 798, 853, 902)
- M9.8A (web coding app) — implemented in sibling `octos-web` (980)
- M9.26 (coding-session prompt) — landed 2026-04-28; **validation gap**: npm/network recovery live case still pending (1081–1102)

Net: by the doc's own accounting, **0 implementation-open tickets, 2 validation-pending**.

## M8 Fix-First Checklist Status (`docs/OCTOS_M8_FIX_FIRST_CHECKLIST_2026-04-24.md`, 299 lines)

The checklist contains **no `[done]` / `[in flight]` markers** — it is written as a static prescriptive document with seven workstreams (lines 33–263) and a hard gate (282–298). Items:

1. Reconcile M8.8 with M8.2/M8.4 (`execution.rs` ToolContext) — line 33
2. Fix resume sanitizer partial-resolution bug (`resume_policy.rs`) — 71
3. Worktree-missing as hard resume refusal (`session_actor.rs`, `session.rs`) — 101
4. Wire M8.7 into real spawn/task runtime — 131
5. Make profiles/AgentDefinitions authoritative — 166
6. Finish concurrency audit (mark spawn `Exclusive`) — 207
7. Close cache/compaction/resume hand-off gaps — 238

The doc states M8 must be green **before** opening M9.1. The branch nevertheless contains M9.1–M9.26 implementation. Commit `33d86b3 M8.6: Structured resume pipeline (runtime-v0.2)` (2026-04-24) and the heavy churn in `crates/octos-bus/src/resume_policy.rs` (180 lines) and `octos-cli/src/session_actor.rs` (3,280 lines touched) suggest some M8 fix-first items got addressed inline rather than landed first. **Whether each of items 1–7 is actually green is not surfaced anywhere as a status checkbox** — verifying that requires a separate audit.

## Recent Fixup Pattern (last 14 days)

Branch-only fixup-flavored commits (4 of 17 = 23.5%): `Harden TUI tmux prompt submission`, `Fix SMTP/Feishu password UI…`, `fix(ops): restore deploy, smtp, daemon logging`, `fix(voice skill): probe ports for ominix-api`. Plus three more 2026-04-28 commits that read as fixups even without "fix" in the verb: `Use ASCII state labels in TUI tmux waits`, `Avoid stale active matches in TUI tmux wait`, `Accept done state in TUI UX harness`. **All of those touch tmux harness / TUI gating code, not the protocol or session-handler core.** No commit this branch reads "regression", "race", or "wip" against `crates/octos-cli/src/api/ui_protocol*.rs`, `crates/octos-bus/`, or `crates/octos-core/`. Repo-wide the last 14 days have ≈ 30 fix/regression commits but most pre-date the branch (SMTP, dashboard, LLM routing, search-API, M4 CI), and only the four listed above sit on this branch.

## TODO / FIXME / panic count in M9 files

For the 11 net-new M9-era files (`crates/octos-core/src/ui_protocol.rs`, `app_ui.rs`; `crates/octos-cli/src/api/ui_protocol{,_approvals,_diff,_progress,_task_output,_ledger}.rs`; `crates/octos-agent/src/{permissions,network_policy,fs_policy}.rs`):

- `TODO` / `FIXME` / `XXX` / `HACK` count: **0**.
- `panic!` / `unreachable!` / `unimplemented!` / `todo!` count: **9** total, all of them inside `#[cfg(test)]` test arms (`expected … notification` panics) **except one** at `crates/octos-agent/src/permissions.rs:213` (`PermissionMode::DangerFullAccess => unreachable!()`). That one is in production code; it is reachable by definition if a future variant uses that arm.
- Adjacent (heavily M9-touched) `crates/octos-cli/src/session_actor.rs:1424` still carries `TODO(M8.4): after FileStateCache lands, populate its …` — direct evidence that an M8 fix-first item is still threaded through M9-merged code.

## Stability Heuristic Table

| Metric | Value |
|---|---:|
| Total commits on branch | 17 |
| Total commits in last 14 days (all on branch) | 16 (one same-day) |
| Files changed | 280 |
| Insertions / deletions (`diff --stat`) | +27,596 / −48,764 |
| Numstat add / del | +30,252 / −620 |
| Top 1: `swarm-app/package-lock.json` | 4,232 (deletion) |
| Top 2: `crates/octos-cli/src/session_actor.rs` | 3,280 |
| Top 3: `crates/octos-cli/src/api/ui_protocol.rs` | 2,949 (net-new) |
| Top 4: `crates/octos-bus/src/api_channel.rs` | 2,796 (mostly deletion) |
| Top 5: `crates/octos-core/src/ui_protocol.rs` | 2,718 (net-new) |
| Test+E2E lines | 11,139 across 66 files |
| Production code lines (.rs/.ts/.tsx, non-test) | 49,674 across 155 files |
| Test:code ratio | **0.22** |
| Branch-only fixup-style commits | 4 / 17 ≈ **23.5%** (7 / 17 ≈ **41%** if "Harden", "Avoid stale", "Accept done state" count as fixups) |
| Validation-pending M9 tickets | 2 |
| Open M9 issues per 1k LOC of core protocol code (3,067 lines in `octos-core/src/{ui_protocol,app_ui}.rs`) | **2 / 3 kLOC ≈ 0.65** |

## Scripts (§5 of the prompt)

The new scripts are operator-facing tmux drivers, **not** wired into CI: `grep` of `.github/workflows/`, `scripts/ci.sh`, `scripts/milestone-ci.sh` returns zero matches for `compare-tui-coding-ux-tmux`, `compare-coding-ux-tmux`, `tmux-cli-driver`, `compare-coding-agents`. The five large additions:

- `scripts/compare-tui-coding-ux-tmux.sh` (1,145 lines) — drives real `octos-tui --mode protocol` + Codex through tmux on the same coding fixture; sources `tmux-cli-driver.sh`. Header at file:1–10 declares it the M9.18 / M9.24 release-evidence harness.
- `scripts/compare-coding-ux-tmux.sh` (678) — older sibling driving `octos chat` (line-mode CLI) + Codex.
- `scripts/tmux-cli-driver.sh` (346) — shared tmux primitives (idempotent guard at top), used by both compare scripts.
- `scripts/compare-coding-agents.sh` (291) — non-tmux head-to-head fixture runner.
- `scripts/deepseek-chat-compat-proxy.py` (284), `scripts/analyze-coding-ux-transcripts.sh` (114), `scripts/check-ui-protocol-upcr.sh` (42), `scripts/windows-appcontainer-live-probe.ps1` (126).

These are repeatable harnesses (deterministic `RUN_ID`, fixture-based, JSON summary output) but they are operator-invoked and gated on env credentials (`DEEPSEEK_API_KEY`); they are not part of the green CI lane.

## Verdict — Is This Branch Fit for "Stable"?

**No, not yet — but the case is mixed.** Reasons:

**For "stable":**
- Per the M9 issue stack's own self-assessment, 24 sub-tickets are signed off and 0 are implementation-open.
- Zero `TODO`/`FIXME`/`HACK` markers in the eleven net-new M9 protocol/policy files.
- Test:code = 0.22 with 11k lines of e2e/unit fixtures, including 10+ live-* spec files and a tmux harness.
- Recent fixup commits target tmux-harness wait conditions, not protocol or session-actor core — i.e., the runtime contract is not still being patched.

**Against:**
1. **Recency**: 100% of branch commits land in a 5-day window, 11 of 17 on a single day (2026-04-28) including a single mega-commit `70c756d Add coding green M9 protocol and sandbox work` that contains the bulk of the protocol code. There has been no soak time.
2. **Two validation gaps openly tracked** — strict M9.24 chat-first rerun, and M9.26 npm/network recovery — and the doc itself notes M9.4's runtime disk-output caveat is still live.
3. **M8 fix-first checklist has no done/in-flight markers** — it is impossible to confirm from the doc alone whether items 1–7 are green. The lingering `TODO(M8.4)` at `session_actor.rs:1424` and the >3,200 lines churned in that same file argue the M8 floor was not separately stabilized before M9 landed.
4. **Operator-facing tmux harnesses are not in CI**, so the release-evidence path depends on manual operator runs with `DEEPSEEK_API_KEY`. The "live-parity2-20260427T215456Z-63329" run cited in the doc is one passing run, not a CI signal.
5. **Heavy bus/session deletion** (`api_channel.rs` -2,620, `session.rs` -1,712) plus a 3,280-line churn in `session_actor.rs` is exactly the surface most prone to silent regression; the test-to-code ratio for the bus area in particular is unclear from the global 0.22.
6. The `unreachable!()` at `crates/octos-agent/src/permissions.rs:213` is a sharp edge in a brand-new permissions module.

**Recommendation:** Treat this branch as **release-candidate**, not stable. Before promoting:
- run the strict M9.24 chat-first parity lane and the M9.26 npm/network recovery case (the two openly-pending validations);
- explicitly check off M8 fix-first items 1–7 against the post-merge code (especially items 1, 2, 3, 7);
- wire `scripts/check-ui-protocol-upcr.sh` and at least the default tmux lane into CI;
- soak the branch for at least one more week without protocol-touching commits and watch for follow-up fixups.

