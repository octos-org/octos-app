# M9 Branch Code Review — Verdict

Date: 2026-04-28. Reviewers: 5 parallel deep-dives across protocol types, server handler, approvals + diff, tests + issues, scope + stability. Branch: `coding-green-m9-local-20260428` in `~/home/octos`. Surface: ~30K lines added across 144 files, M9 protocol code ≈ 8.5 kLOC across `crates/octos-core/src/ui_protocol.rs` (2,718) + `crates/octos-cli/src/api/ui_protocol*.rs` (5,830).

## Headline

**The "M9 is not stable" claim is correct.** Five independent reviews each landed amber-or-red on their slice. The protocol types layer is the strongest area; the server handler runtime, the approvals-and-diff path, and the e2e test coverage are each independently below what a "stable" claim requires. Net verdict: **release-candidate, not GA.**

`octos-app` (the native client this team built today) **works against this branch** — the live smoke test passes — but it works because we exercised the happy path. The unstable parts of M9 are concentrated in failure modes (reconnect, interrupt, double-respond, slow client, restart, scope enforcement), which are exactly the modes a desktop client triggers more than a TUI does.

## Per-slice verdicts

| Reviewer | Verdict | Top concern |
|---|---|---|
| 01 — Protocol types | **amber → red** | `progress/updated` is in the method registry but **missing from the `UiNotification` enum** — clients can't decode it. Spec §10 error taxonomy has zero matching codes (`unknown_session`, `cursor_out_of_range`, `permission_denied`, `APPROVAL_NOT_PENDING -32011` all absent in `rpc_error_codes`). `RpcRequest.id: String` violates JSON-RPC 2.0 (id can be string OR number OR null). Closed wire enums lack `#[serde(other)]` despite spec line 75 forbidding the assumption. |
| 02 — Server handler runtime | **amber → red** | Three process-global, never-evicted singletons (ledger, active turns, contract stores). M9.6 in-memory ledger with **no compaction, no TTL, no LRU, no persistence** — sessions accumulate forever in a long-running daemon and replay breaks across restart. **`turn/interrupt` violates the spec's own "idempotent and explicit" requirement** — TOCTOU race emits both `turn/completed` and `turn/error` for the same `turn_id`. `let _ = send_*().await` × ~40 silently swallows WS send errors. Backpressure is silent drop (`try_send`) with no on-wire indicator — replay shows gaps. **Zero WS integration tests in `tests/`.** |
| 03 — Approvals + diff | **red** | **`approval_scope` is silently ignored** — capability advertised, field carried on the wire, never read by `respond`. No policy table, no future-call gating. Risk hard-coded to `"medium"` for every shell command (`ui_protocol.rs:243`). No audit trail (no tracing, no ledger entry, no `approval/decided` event). Reconnect replays the request but loses the decision. Diff path has TOCTOU between proposal and apply; `notice.path` fed to display strings without sanitization. **Zero e2e tests touch approvals.** |
| 04 — Tests + open issues | **red on coverage, amber on bug burden** | **0% wire coverage.** None of the 8 specs labeled "M9-era" actually exercise WS protocol methods — they hit `/api/chat` SSE and assert against the rendered DOM. `session/open`, `turn/start`, `turn/interrupt`, `approval/respond`, `diff/preview/get`, `task/output/read` — all untested at the wire. **Zero fault-injection** (drop mid-turn, stale cursor, double respond, etc.). Open issues `#632`, `#634`, `#636`, `#618` are active SSE-side races whose root causes carry into the M9 ledger. M8 fix-first checklist (`#536`, `#537`, `#538`, `#539`, `#540`, `#541`, `#543`) all open and unverified. |
| 05 — Scope + stability | **release-candidate, not stable** | 17 commits in 5 days; 11 commits on 2026-04-28 alone. **No soak time.** One mega-commit `70c756d` landed +8,042 lines in a single shot. M9 sub-ticket doc claims 24/27 done but markers are author-asserted, not validated. M8 fix-first checklist has **no `[done]` markers anywhere** — it's a static prescription, not a tracking doc. 5 large operator scripts added (`compare-tui-coding-ux-tmux.sh` 1,145 LOC, etc.) — **not in CI**. One production `unreachable!()` in brand-new `permissions.rs`. Test:code ratio 0.22. |

Read the individual reports for line citations and concrete repro paths.

## Cross-cutting themes

Pattern repeats across reviewers:

1. **Open registries advertised, closed enforcement.** Spec says "treat unknown variants as ignorable forward-compat"; code uses closed `enum`s without `#[serde(other)]` (review 01). Spec advertises `approval_scope`, code drops the field (review 03). Spec promises capability negotiation, server handler advertises features it doesn't actually enforce (review 03 risk-hardcoded).
2. **Best-effort error handling.** ~40 silently-swallowed WS sends (review 02), capability fields ignored (review 03), backpressure as silent drop (review 02). Failure modes leave clients in disagreement with the server.
3. **Memory & restart.** Three never-evicted singletons (review 02). No persistence story for the ledger, the active-turns registry, or the diff store. Restart loses pending approvals; long-running daemon leaks RAM. Issue #634 ("done before commit") suggests the persistence boundary is fuzzy across the M9 path too, not just SSE.
4. **Test surface is upside-down.** 11K test lines, 0 of them exercise the M9 wire. The TUI tmux drivers are operator-friendly but not in CI (review 05). All reviewer concerns above are *easy to discover with a wire-level e2e* — the absence of one is the primary stability risk multiplier.

## What's genuinely well done (don't lose this)

Each reviewer found real strengths worth preserving:

- Centralized JSON-RPC method-name constants + golden serde literal tests (01).
- UUIDv7 newtypes for ids with `Default` impls (01).
- Open string registries for `approval_kind` and `approval_scope` with explicit fallback test at line 2257 (01).
- Idempotent double-respond returns the recorded decision (`ui_protocol_approvals.rs:38–77`) — the right pattern, even if scope+audit around it are missing (03).
- Capability schema is split from feature flags so the handshake can evolve (01).
- Zero TODO/FIXME/HACK markers in M9-net-new files (05).
- Doc-internal M9 sub-ticket sign-off (24/27) — visible audit trail even if not all validated (05).

This branch is *coherent*. It's not a kludge. The bones are sound. The instability is in scope completion + test coverage, not in fundamental design.

## What it would take to honestly call M9 stable

In rough priority:

1. **Wire-level e2e tests** for every protocol method + the four standard fault-injection cases (drop mid-turn, stale cursor, double respond, interrupt-after-complete). Until at least one of these exists in CI, the "stable" claim cannot be defended on evidence.
2. **Fix `progress/updated` round-trip** — it's in the registry but not the enum, so any client written against the typed surface drops these notifications. Two-line fix; no excuse.
3. **`approval_scope` enforcement** — either delete the field or wire it to a policy table. Today it's a security/usability lie.
4. **Spec §10 error code parity.** Add the named error codes to `rpc_error_codes`. Make idempotent retry semantics machine-readable.
5. **Ledger persistence + eviction.** Pick one: (a) durable backing store, or (b) explicit "lost on restart" contract documented in spec. Either way, fix the unbounded RAM growth.
6. **Resolve the M8 fix-first checklist.** Spec §12 says M9 should not freeze over these; today none are confirmed shipped.
7. **Close active SSE races (#632 / #634 / #636 / #618)** before claiming the WS path inherits no analogous bugs. Reviewer 02 found the same late-binding pattern is structurally possible on the WS side.
8. **TUI tmux harness in CI.** The five operator scripts that drive end-to-end coverage today aren't in any CI lane. Make them blocking on protocol-touching PRs.

## Implications for `octos-app`

We built `octos-app` against this branch. Honest impact:

| Concern from this review | How `octos-app` is exposed |
|---|---|
| `progress/updated` missing from enum | Our store reduces nothing today on this notification (we silently buffer) — so we don't lose data, we just can't render rich progress. Fixed when M9 fixes the enum. |
| `turn/interrupt` race emits two terminal events | Our store should accept the second one as a no-op via the cursor; not yet tested. Add a contract test. |
| Backpressure silent drop | We trust durable cursor. Server gap means our "we caught up via replay" assumption can lie. Need a `replay_lossy` warning. |
| `approval_scope` ignored server-side | Our UI surfaces a scope dropdown. Today it's a UI-only lie. Disable scope until the server enforces. |
| Risk hardcoded "medium" | Our risk badge is meaningless. Either trust nothing or render "unspecified." |
| No audit trail | Our client should keep its own decision log so a reconnect can verify. We don't yet. |
| Ledger in-memory + no restart | Our cursor on disk → server restart → cursor invalid → we re-hydrate via REST snapshot. Already designed for this; need a contract test. |

Five of these are workarounds the client can do; two (scope, risk) need server fixes before the corresponding UI is honest.

## Recommendation

`coding-green-m9-local-20260428` is a **strong release candidate** — coherent design, well-laid foundation, no kludges. It is **not stable** by any honest reading of the word. The gap is concentrated in test coverage and three or four scope-enforcement / error-handling lapses. With ~5 days of focused fix-list work plus the wire-level e2e suite, it gets to honestly-stable. Without that work, "stable" is a marketing claim, not an engineering one.

For `octos-app`'s purposes: **good enough to ship dogfood internally**, **not good enough to put in front of paying users until the test surface lands and the scope/audit lapses close.** That mirrors the server team's own posture (issue #573 is on hold; only coding-only M9.8A is in scope).

Read the per-slice reports for the actionable line-cited details.
