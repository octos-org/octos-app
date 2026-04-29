# W05 — Approvals & diff preview

## Mission

When an agent asks to mutate the workspace — shell, write, network, sandbox-escalate — the user
sees a typed card explaining exactly what's being asked, views the file diff hunk-by-hunk, and
approves / denies / scopes in a click. The response round-trips idempotently so a flaky network
never strands a turn or double-applies a decision. Capabilities (`approval.typed.v1`,
`pane.snapshots.v1`) negotiate at session-open; UI degrades to plain `{title, body}` when typed
payloads aren't available. W05 owns the approval card, diff preview widget, queue surface
CodingScreen embeds in M3, and two protocol calls (`approval/respond`, `diff/preview/get`).

## Header

| Field | Value |
|---|---|
| Lane | B |
| Milestone | M2 |
| Depends on | W01 (transport, capabilities), W04 (task-dock + REST hydrate patterns) |
| Lifts from | `aichat`'s `CodeView` (makepad-code-editor) — same widget renders diff hunks |
| Net new | `ApprovalCard`, `DiffView`, capability negotiation, scoped-respond client |
| Owner | one Lane B agent, parallel with W04 once its task-dock skeleton lands |

## Scope

In: `ApprovalCard` (typed-payload-aware: diff / command / network / fs / sandbox-escalation)
with risk badge, render-hint labels, primary/secondary buttons, scope dropdown. `DiffView`
(file tree + hunk list, `CodeView`-rendered). Capability probe at session-open with graceful
degradation. Wire clients for `approval/respond` + `diff/preview/get` with idempotent retry.
`ApprovalQueueWidget` (used by W06). AppState slice keyed by `ApprovalId`.

Out: server-side approval policy. The spec calls `approval_scope` *advisory* and forbids the
server from "silently creat[ing] persistent allow rules"
(`OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md:278`); we surface, server decides. Sub-account /
multi-tenant approvals (admin, in web). Splash inline approvals (sandboxed in M2).
Cross-session audit trail.

## Capability negotiation

Probe at `session/open`. Server advertises via `SessionOpenedResult.capabilities.supported_features`
(`octos-core/src/ui_protocol.rs:23`, schema version 2). Flags: `UI_PROTOCOL_FEATURE_APPROVAL_TYPED_V1`
(`:29`), `UI_PROTOCOL_FEATURE_PANE_SNAPSHOTS_V1` (`:32`). Client sends them in
`X-Octos-Ui-Features` at WS handshake; server's `ConnectionUiFeatures`
(`octos-cli/src/api/ui_protocol.rs:67`) keys off `has_ui_feature(headers, query, …)` (`:74`).
We trust the reply.

`approval.typed.v1=true`: full card. `=false`: `{title, body}` + Approve/Deny only — per spec
compatibility (lines 369–376) the server omits `typed_details`. `pane.snapshots.v1=false` hides
"Show diff": no `diff/preview/get` to hydrate, and embedded full diffs are disallowed (lines
374–376). With typed=true, **unknown** `approval_kind` or `typed_details.kind` must still fall
back to generic (lines 372–373). We pattern-match the known set
(`command | diff | filesystem | network | sandbox_escalation`, `octos-core:34–40`); else
generic. Forward-compat we cannot regress on.

## Approval card design

Top-to-bottom: **risk badge** (pill from `ApprovalRequestedEvent.risk`, `octos-core:1490`;
match `low | medium | high | critical`, unknown = grey); **tool_name + title** (`:1484–1485`,
always present); **body** switched on `typed_details.kind` (`:1432–1446`); **render hints**
(`ApprovalRenderHints`, `:1466`); **buttons** (primary default Approve, secondary Deny, disable
on click); **scope dropdown** when `approval.typed.v1=true`.

Body branches map to the typed-details Rust types:

- `command` → `ApprovalCommandDetails` (`:1346`): argv, command_line (mono), cwd, env_keys;
  optional `ApprovalSandboxDetails` sub-block (`:1360`).
- `diff` → `ApprovalDiffDetails` (`:1372`): operation, file_count, additions/deletions, "View
  diff" keyed on `diff.preview_id`.
- `filesystem` → `ApprovalFilesystemDetails` (`:1387`): operation, paths, `outside_workspace`,
  writable_roots.
- `network` → `ApprovalNetworkDetails` (`:1397`): hosts, ports, urls.
- `sandbox_escalation` → `ApprovalSandboxEscalationDetails` (`:1416`): from/to endpoints,
  requested_permissions, justification, suggested_prefix_rule.

Render hints: `primary_label`/`secondary_label` override text; `default_decision` pre-focuses;
`danger=true` styles primary red; `monospace_fields` (dotted paths e.g.
`typed_details.command.command_line`) render in code font (fixture `:2135–2136`). Scope values
from `octos-core:42` (`approval_scopes::REQUEST | TURN | SESSION`, default `request`), sent as
raw string in `ApprovalRespondParams.approval_scope` (`:577` — `Option<String>`, not enum, for
forward-compat).

## Diff preview widget

Two panes inside `DiffView`: file tree (~25%) + hunk list (~75%). Hunks render through
`CodeView` from `makepad-code-editor`. `aichat` already wires this — see `aichat:404–411` and
`aichat:514–542` (the latter has the per-instance font override preventing `theme.font_code`
from being baked too early). Lift the DSL almost verbatim; only per-line `draw_bg` changes.

`DiffPreview` shape (`octos-core/src/ui_protocol.rs:663–714`): `{session_id, preview_id, title?,
files: [{path, old_path?, status, hunks: [{header, lines: [{kind, content, old_line?,
new_line?}]}]}]}`. Status `:684` (`added | modified | deleted | renamed`); line kind `:710`
(`context | added | removed`).

`CodeView` is single-buffer, so we render one per hunk inside a vertical PortalList. Per-line
colouring lifts `aichat`'s draw-shader override (`aichat:518–541`): pass hunk text to
`CodeView.editor.set_text`, wrap `draw_bg` with a custom shader taking a parallel
`line_kind: u8` array via instance buffer (0=ctx, 1=add, 2=del). Fragment picks `#x14361A99`
added, `#x40121299` removed, transparent context. Token highlighting stays the editor's job.
Renames render `old_path → path` in the tree row. `diff/preview/get` returns the whole preview
in one shot (`octos-cli/src/api/ui_protocol.rs:1165–1197`). With `pane.snapshots.v1=false`:
file list with hunk counts and "Diff preview unavailable" hint.

## Approval response flow

Idempotency on the wire matters. Spec §10 (line 441) requires idempotent commands to declare
it. Server enforces: a second response on a decided approval returns `APPROVAL_NOT_PENDING`
(-32011) with the *recorded decision* in error data
(`octos-cli/src/api/ui_protocol_approvals.rs:198–215`, test `:241–253`). A retry racing a
successful first attempt is therefore safe — `Accepted` or `APPROVAL_NOT_PENDING` carrying our
decision. Without this, WS drops mid-respond would either lose the decision or apply it twice.

With a stable `ApprovalId`:

1. Click Approve → `PendingResponse { decision, scope, attempt }`; buttons disable.
2. Send `approval/respond` (params `octos-core/src/ui_protocol.rs:572`).
3. `Accepted` (`ApprovalRespondResult` `:605`, `runtime_resumed: bool` `:609`) →
   `Decided { decision, runtime_resumed }`.
4. WS drop / timeout: keep `PendingResponse`, retry with same params.
5. `APPROVAL_NOT_PENDING` matching → `Decided`. Conflicting → `Failed { Conflict }`.
6. `APPROVAL_NOT_FOUND` (-32010 `ui_protocol_approvals.rs:11`) → `Failed { Expired }`.

Double-click protection is purely UI: buttons disable on first click, re-enable only on
`Failed`. We don't rely on the server alone — disabled buttons keep the user from spamming.

## AppState slice

```rust
pub struct ApprovalsSlice {
    pub by_id: HashMap<ApprovalId, Approval>,
    pub pending_order: Vec<ApprovalId>,
}
pub struct Approval {
    pub event: ApprovalRequestedEvent,
    pub state: ApprovalState,
    pub diff_preview: Option<DiffPreview>, // hydrated lazily
}
pub enum ApprovalState {
    Awaiting,
    PendingResponse { decision: ApprovalDecision, scope: Option<String>, attempt: u32 },
    Decided { decision: ApprovalDecision, runtime_resumed: bool },
    Failed { reason: ApprovalFailureReason },
}
```

`pending_order` drives the queue; insertion by event arrival, no reordering on state change.
Decided cards collapse but stay in order until cleared by a turn boundary or dismiss.

## Reconnect semantics

`session/open { after: cursor }` triggers cursor replay
(`octos-cli/src/api/ui_protocol.rs:586`). Server replays ledgered notifications including
`approval/requested`, then sends still-pending approvals not in the replay window (`:555–561`,
`:586–600`); `replayed_approval_ids` (a `HashSet<ApprovalId>`) deduplicates. Reducer: upsert
each `approval/requested` by `ApprovalId`; if the entry already has `Decided` or `Failed`,
preserve local state — replay does not overwrite decided ones. For `approval/respond` calls
sent before the drop: if state is still `PendingResponse` after replay, retry; server
idempotency catches duplicates. `runtime_resumed` (`:609`) is the only signal the agent's tool
call has unblocked; persisted in `Decided` so the UI knows whether the turn is still paused.

## Deliverables

1. **Capability negotiation** — extend `OctosUiAgent` (W01) to send `X-Octos-Ui-Features` on
   connect and parse `SessionOpenedResult.capabilities`.
2. **Re-export wire types** from `octos-core::ui_protocol`; add client-only `ApprovalState`,
   `ApprovalsSlice`.
3. **`ApprovalCard` widget** — Makepad live-DSL; all five typed kinds + generic fallback.
4. **`DiffView` widget** — file tree + hunk list; lazy-hydrate via `diff/preview/get`.
5. **`ApprovalQueueWidget`** — card list, standalone in M2 dev harness, embedded by W06 in
   CodingScreen. Filters: All / Pending / Decided.
6. **Wire clients** — `agent.approval_respond(params)`, `agent.diff_preview_get(params)` with
   retry.
7. **Reducer** — `ApprovalAction` variants: `Requested`, `RespondClicked`, `RespondAccepted`,
   `RespondFailed`, `DiffPreviewLoaded`, `Cleared`.
8. **Dev harness** — dev-menu route firing faked `ApprovalRequestedEvent`s for each kind.

## Tests & verification

- **Capability probe.** Fake server omits / includes each flag; UI degrades correctly. Unknown
  flag → ignored without panic.
- **Idempotency.** Mock server: `Accepted`, then `APPROVAL_NOT_PENDING` (-32011) on retry with
  matching `recorded_decision`. Client collapses to `Decided`. Mirrors
  `octos-cli/src/api/ui_protocol_approvals.rs:241–253`. Conflict variant → `Failed { Conflict }`.
- **Diff parse fixtures.** Snapshot tests with three inputs: recorded `diff/preview/get`
  golden file, synthetic unified-diff (if we ever need a client parser), empty / no-hunks
  (rename-only or binary).
- **Reconnect replay.** Drop socket mid-approval, reconnect with `after`-cursor. Exactly one
  `Approval` in `Awaiting`; second `approval/requested` for same `ApprovalId` does not
  duplicate.
- **Render-hints respect.** `default_decision: "deny"` focuses secondary; `danger: true`
  styles primary red. **Generic fallback.** `typed_details.kind = "future_unknown"` → generic
  renders, approve/deny still works.

CI gate: contract tests against live `octos-cli` under W10's harness; unit tests on every
commit.

## Exit criteria

- Agent proposes a file write. Within 200 ms a card appears with risk badge, diff summary,
  "View diff" button.
- "View diff" loads the unified diff in `DiffView`; hunks render green/red; file tree
  highlights changed files; rename and add/delete states display.
- Approve disables buttons, fires `approval/respond`, card flips to Decided in one round-trip.
  Agent resumes (follow-up `tool/started`).
- Killing the WS between click and server response, then reconnecting, yields a single Decided
  card — no duplicate, no lost decision.
- Toggling `approval.typed.v1` off server-side collapses UI to `{title, body}` + Approve/Deny
  without panic.

## Risks

- **Capability flag still moving server-side.** `02-API-DRIFT.md` flags both as "needs
  verification before we lock" (lines 56–61). Mitigation: design around
  `Option<ApprovalTypedDetails>` (already that on the wire at `octos-core:1492`); run probe
  tests on every build against the live server. New shape → branch-on-`Some`, add a case.
- **Capabilities schema version 2** (`octos-core:23`). If server rolls to 3 we'll catch it at
  the wire decode site. Tolerate unknown variants per `03-PROTOCOL-CONTRACT.md`.
- **`runtime_resumed` semantics.** Whether `false` means "agent ignored" vs. "queued, not
  waiting" isn't pinned. Treat `Decided { runtime_resumed: false }` as success; show a subtle
  "queued" indicator; M2 user testing decides.
- **Hunk renderer perf on huge diffs.** One `CodeView` per hunk is fine small; 10k lines and
  200 hunks would allocate 200 editors. Virtualise via PortalList (lifted from
  `aichat:1774–1881`), only mount visible hunks.

## Open questions

- **`approval_scope` value set not finalized.** Spec advertises `request | turn | session`
  (`octos-core/src/ui_protocol.rs:43–45`) but calls scope advisory and forbids server-side
  persistent allow rules (spec line 278). Are additional values ("approve-for-tool",
  "approve-for-tool-prefix") coming, and which mutate server policy vs. UI-only?
- **"Approve for this session" UX.** If the server doesn't honour the dropdown we're lying.
  Render only when the server signals it will honour (needs a new capability), or tooltip
  "advisory; server may re-prompt". Default to tooltip for M2.
- **Approval grouping.** Five files in one edit each generate an approval — group under one
  queue header? Flat queue in M2; revisit in M3.
- **`client_note` audit field** — free-form on `approval/respond` (`octos-core:579`). Should
  the card grow a "reason for denial" textarea? Likely yes; deferred.
- **Diff truncation.** `MAX_DIFF_PREVIEW_BYTES` truncates server-side
  (`octos-cli/src/api/ui_protocol.rs:~1502`); client gets a partial `DiffPreview`. Should the
  server include a `truncated: true` flag?
