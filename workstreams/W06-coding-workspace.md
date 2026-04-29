# W06 — Coding workspace

## Mission

Own the `CodingScreen`: a two-pane workspace where the agent works and the human
referees. Approvals queue on the left, preview pane on the right. Everything
coding needs that a chat thread doesn't (typed approvals, diffs, command /
network / filesystem previews, task output tail) lives here. No embedded editor
— Octos' coding mode is approval-first.

This is the screen that distinguishes Octos from the chat-only `aichat` lift.
The point of M3 is "you can watch and gate an agent that writes files and runs
shells, without leaving the app".

## Header

| Field | Value |
|---|---|
| Lane | C — Producers |
| Milestone | M3 |
| Depends on | W05 (approval card + DiffView), W04 (task model + output endpoints), W02 (app shell) |
| Lifts from | `aichat:404–411, 510–543` (CodeView from `makepad-code-editor`) for diff hunks |
| Reference | `octos-web/src/coding/coding-workspace-page.tsx`; `octos/docs/OCTOS_TUI_ARCHITECTURE_2026-04-24.md` §3 |

## Scope

**In.** `CodingScreen` two-column layout under `CurrentScreen::Coding` in the
`main_area` PageFlip (per `04-IA-AND-NAVIGATION.md` §"CodingScreen"). An
`ApprovalQueue` (`PortalList` of W05 `ApprovalCard`s) with an "approve all of
kind X" affordance. A right-side `PageFlip` over five child views: `DiffView`
(W05), `CommandPreview`, `NetworkPreview`, `FileSystemPreview`, `OutputTail`.
Task output drill-down (`task/output/read` + `task/output/delta`) into
`OutputTail`. A `CodingViewState` slice on `AppState`.

**Out.** A code editor for the user to write in — that's the agent's job.
Project initialization wizards. The web's full inspector (workspace tree, git
history); these are useful but not load-bearing for M3 — defer until queue
+ preview is solid.

## Layout

`04-IA-AND-NAVIGATION.md` §"CodingScreen" shows the picture. Two columns inside
a `GlassPanel` matching the chat shell. Left `width: 380` (slightly wider than
the web's `360` to fit risk badges + diffs); right is `Fill`. No second row in
M3 — drop the web's lower diff/inspector strip; consolidating into the right
`PageFlip` removes a focus loop and keeps the screen readable at 900×700.

The left column is itself split: a "pending" `PortalList` on top (oldest-first,
scrolls), a divider, and a "history" `PortalList` of decisions made this turn
(compact). Empty pending list collapses to a centered empty state in the
chat-shell style (`aichat:966–984`).

The right `PageFlip` key derives from the focused approval (its
`approval_kind`) or, if a task is focused via the W04 dock, from the task id.
Animation-free in M3.

## Approval queue

A `PortalList` of pending `ApprovalRequestedEvent`s. Each row is the W05
`ApprovalCard` (typed-payload-aware, same affordances). Cite W05 for per-card
design — we do not redesign here.

W06 adds:

- **Focus.** Selecting a card sets `CodingViewState.selected_approval` and
  drives the right `PageFlip`. Click or arrow keys.
- **Batch affordance.** When ≥2 pending approvals share an `approval_kind`,
  a pill at the top of the queue offers `Approve all 3 "filesystem.write"`.
  One click emits N `approval/respond` with `approval_scope: "request"` (no
  `"session"` from this — needs more design; see open questions).
- **History.** Decided approvals collapse to single-line summaries
  (`✓ filesystem.write src/foo.rs`); clickable to re-open in the right pane.
  Cleared on `turn/started`.

## CommandPreview / NetworkPreview / FileSystemPreview

Three small Makepad widgets, each consuming one branch of `ApprovalTypedDetails`
from `approval.typed.v1` (`octos-web/src/coding/app-ui-protocol.ts:81–108`):

| Widget | Reads | Renders |
|---|---|---|
| `CommandPreview` | `details.command` | `cwd`, `command_line` (or `argv.join(" ")`) in monospaced `code_view`, env-var line if present |
| `NetworkPreview` | `details.network` | method + URL, host, body summary if present |
| `FileSystemPreview` | `details.filesystem` | `operation` badge, single `path` or list of `paths` (small `PortalList` for >5) |

Each renders a **risk badge** sourced from the parent event's `risk` field —
amber chip for `medium`, red chip for `high`, no chip for `low`/unset. Tone
styles lift verbatim from W05's card risk badge.

When `typed_details.kind` is unrecognised (older server, unknown kind), the
preview falls back to the rendered Markdown body. Covers protocol drift
(`02-API-DRIFT.md`).

## Task output drill-down

When the user clicks a task in the W04 task dock (mounted under `CodingScreen`
the same as under `ChatScreen`), the `PageFlip` flips to `OutputTail` and the
dispatch loop fires:

1. `task/output/read` with `task_id`, `limit_bytes: 4000`, `cursor: <last>`.
2. Subscribe to `task/output/delta` for that `task_id`.

`OutputTail` is a `ScrollYView` over the appended bytes, monospaced, with
sticky-tail (auto-scrolls when at bottom; stops when the user scrolls up).
Cap at a 12 KB rolling buffer (matches `octos-web`
`use-coding-app-ui.ts:131`). Cursor offset per task held on
`CodingViewState.output_cursor`.

## AppState slice

```rust
pub struct CodingViewState {
    pub selected_approval: Option<ApprovalId>,
    pub preview_pane: CodingPreviewPane,
    pub batch_pill_dismissed: HashSet<String>, // approval_kind
    pub output_cursor: HashMap<TaskId, u64>,
    pub history_decided_in_turn: Vec<ApprovalId>,
}

pub enum CodingPreviewPane {
    Empty,
    Diff(PreviewId),
    Command(ApprovalId),
    Network(ApprovalId),
    FileSystem(ApprovalId),
    OutputTail(TaskId),
}
```

Lives in `AppState.ephemeral` — does not roundtrip through the protocol, does
not persist. Reset on `Logout` and on session change.

## Deliverables

1. `app/src/coding/screen.rs` — `CodingScreen` widget; two-column layout,
   mounts queue + preview-pane PageFlip.
2. `app/src/coding/approval_queue.rs` — pending / history `PortalList`s,
   batch pill.
3. `app/src/coding/preview/{command,network,filesystem}.rs` — typed-payload
   widgets.
4. `app/src/coding/preview/output_tail.rs` — sticky-tail, 12 KB buffer.
5. `app/src/coding/state.rs` — `CodingViewState` and reducer actions
   (`SelectApproval`, `SetPreviewPane`, `BatchApproveKind`, `OutputCursorAdvance`).
6. Wire `CodingScreen` into `main_area` PageFlip in W02's app shell.
7. Capability handshake: assert `approval.typed.v1` is in the server's
   `ui_features`; if not, fall back to body-Markdown and toast a warning.

## Tests & verification

- **Golden screenshots** (W10 harness): empty queue; diff approval focused;
  command approval focused with batch pill; task selected with output
  streaming.
- **Approval-flow integration test.** Receive 3 approvals → focus card 1
  (DiffView renders) → approve → focus card 2 (CommandPreview renders) →
  batch-deny remaining 2 of same kind → assert 4 outbound `approval/respond`
  calls in order. Runs against the W10 fake transport.
- **Output-tail smoke.** 50 KB staged → tail caps at 12 KB; live deltas
  append without dropping; sticky-tail toggles with user scroll.
- **Capability fallback.** No `approval.typed.v1`: body-Markdown renders;
  no panic on missing `typed_details`.
- **Reducer replay.** Same shape as W04's reducer-replay test.

## Exit criteria

- A user can open `CodingScreen` mid-session, see a queued typed approval,
  focus it, see the appropriate preview, approve/deny, and the card moves to
  history.
- Clicking a running task while on `CodingScreen` streams live output into
  `OutputTail`; reconnect within 30 s resumes from the last cursor without
  missing bytes.
- The screen survives 50 sequential approvals without UI lag or memory growth
  beyond the buffer cap (W10 load smoke).
- Golden screenshot suite green. Capability-fallback case renders.

## Risks

- **Burst approvals (50 in a row).** Virtualised `PortalList`, batch-by-kind
  pill, soft cap that collapses any kind with >10 pending into a grouped row.
  If still not enough, M4 adds "approve all in this turn" — protocol supports
  it via `approval_scope: "turn"` (`app-ui-protocol.ts:25–27`).
- **Oversized network / filesystem previews.** Cap at 256 lines or 8 KB; show
  "+ N more"; route to a modal only on user click.
- **Focus-stealing.** Never auto-shift `selected_approval` on new approvals —
  append only. If user is scrolled into history, surface a "3 new" badge.

## Open questions

- "Auto-focus next pending after a decision" — minimises clicks but
  conflicts with the focus-stealing avoidance. Decide after dogfooding one
  real coding session.
- `approval_scope: "session"` UX: probably a kebab-menu item ("always approve
  writes under `src/` in this session") on the card, but the affordance is
  not designed. Flag for M4.
- When the user navigates away from `CodingScreen` mid-stream, do we keep
  `task/output/delta` alive? Web does. Probably yes for parity; measure cost
  first.
- Diff hunk rendering: W05 plans to use `CodeView` for inline card diffs.
  Same instance expanded for the right pane, or a separate widget tuned for
  big diffs? Resolve once W05's first cut lands.
- Tab bar at top of preview pane (Diff / Command / Network / Filesystem /
  Output)? Default: no — focused card is the cue. Revisit if testers get lost.
