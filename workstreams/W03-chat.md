# W03 — Chat experience

## 1. Mission

Ship the chat surface that proves the aichat-reuse thesis. User picks a session, types,
watches the assistant stream with full markdown / code / math / mermaid / diagram
fidelity, cancels cleanly, switches sessions, reconnects mid-turn without dupes. M1
carry vehicle. We do not re-design chat; we transplant aichat's ChatList + composer +
streaming pipeline almost verbatim, then rewire persistence and dispatch to Octos.

## 2. Header

| Field | Value |
|---|---|
| Lane | B (Chat experience) |
| Depends on | W01 (transport), W02 (app shell) |
| Lifts heavily from `aichat` | Yes — see `05-AICHAT-REUSE-MAP.md`. ~70% verbatim port; ~30% net-new (SQLite cache, REST hydrate, Octos `AgentEvent` mapping) |
| Owner | one B-lane agent |
| Milestone | M1; follow-up streaming-final pass after W04 |

## 3. Scope

In:

- ChatList + PortalList User/Assistant templates — `aichat:343–614, 1774–1881`.
- Composer (multiline TextInput + actions row, send/cancel/clear) — `aichat:994–1112,
  2291–2369`.
- Streaming pipeline (delta accumulation, mid-stream remend, fade-in) —
  `aichat:1799–1819, 501–509, 2576–2630`.
- Sub-renderers: CodeView, MathView, DiagramView, MermaidSvgView (`aichat:1384–1722`),
  Splash.
- Per-instance font-fallback overrides on every Markdown / CodeView
  (`aichat:368–448, 485–582`) — required for CJK, Unicode arrows, math, emoji.
- History hydrate at session-open: REST `GET /api/sessions/{id}/messages` then
  `session/open { after: cursor }` (`03-PROTOCOL-CONTRACT.md` § Reconnect).
- Per-session SQLite cache warming the thread before REST returns; replaces
  `aichat_history.json` (`aichat:1144`, `01-ARCHITECTURE.md` § 6).

Out: tools / TaskDock, file browser, viewers — W04. Approvals, diff preview — W05.
Coding / Studio / Slides / Sites — W06 / W07. Auth, profile picker — W08. Sidebar
session list, connection indicator — W02.

Out for M1:

- **Splash inline UI from LLM responses.** `splash_view` stays wired in the DSL
  (`aichat:415, 549`) — path preserved — but the renderer is gated off and the server
  profile told not to emit ```runsplash. Re-enables in M2 once sandbox signs off
  (`00-CHARTER.md` § Risks; `01-ARCHITECTURE.md` § 9.2).
- **`+` file-attach.** Slot kept for layout; inactive.

## 4. Streaming pipeline

`03-PROTOCOL-CONTRACT.md` § Live streaming output: `message/delta` is **non-durable**
(spec 415–419). We mirror aichat's split between an ephemeral buffer and a durable
message vector because the wire makes that split authoritative.

Per turn:

1. Send: composer dispatches `turn/start`, push User `ChatMessage` into
   `AppState.sessions[s].messages`, set `ephemeral.streaming_text = String::new()` and
   `is_streaming = true` (`aichat:2302–2312`).
2. `turn/started` → reducer marks `Turn::InProgress`. ChatList draws a synthetic
   Assistant row whose body is `streaming_text` (`aichat:1788–1820`).
3. Each `message/delta` → `OctosUiAgent` lifts to `AgentEvent::TextDelta`
   (`agent.rs:58`). Consumer at `aichat:2576–2589` appends and redraws. Fade-in shader
   (`aichat:501–509`) and `streaming_display_with_latex_autowrap_remend`
   (`aichat:1815–1819`) keep layout stable.
4. On `turn/completed`: **drop the ephemeral buffer**. Do not commit `streaming_text`
   into `messages` client-side — `aichat:2601–2630` does this today, and the protocol's
   authority model demands it. Clear, set `is_streaming = false`, call
   `hydrate_messages(s)`. REST is source of truth; guards reconnects, server
   reformatting, tool-call summaries.
5. On `turn/error` (`code: "interrupted"` for cancel; `runtime`, `auth`, etc.): mark
   `Errored`, clear ephemeral, show error bubble, re-enable composer
   (`aichat:2631–2649`).

Refinements vs. aichat:

- aichat's `assistant_message_is_safe_to_store` (`aichat:1329, :2610`) drops
  malformed-diagram replies. Keep the check; apply to REST-hydrated history. If the
  server returns an unparseable diagram: log + warning toast; don't replace the bubble.
- Reconnect mid-stream: WS drops while `is_streaming` → leave partial visible (greyed
  via W01 composer-disabled). On reconnect, apply replay. Per
  `03-PROTOCOL-CONTRACT.md` § Reconnect step 5, ephemeral deltas are **not** replayed.
  When the next durable event arrives, discard and re-hydrate. No stitch logic.

## 5. Persistence & hydrate

Per `01-ARCHITECTURE.md` § 6: server is authoritative; local cache is a startup-warmer.

Session-open dance:

1. `AppState.current = Chat { session: Some(s) }` triggers a render.
2. ChatList draws against the SQLite cache for `s` (rows: `(message_id, role, text,
   created_at, applied_cursor)`). <5 ms for 200 messages.
3. In parallel, `GET /api/sessions/{s}/messages`. On return: replace the in-memory
   `messages` with the REST snapshot, upsert into cache. Clobber, not merge — REST is
   canonical.
4. `session/open { session_id: s, after: cursor }`. If accepted, replay applies; if
   `cursor_invalid`, drop cursor, refetch REST, re-open with no `after`.

Cache invalidation: every `turn/completed` refetches REST and upserts (one round-trip
per turn). `cursor_invalid` drops the session's rows and refetches. Logout drops the
cache database. 30-day TTL for unopened sessions, swept on app start.

File at `~/Library/Application Support/octos-app/sessions.db` (macOS); one shared
`messages` table keyed by `session_id`. Non-authoritative — never read into the
reducer except as the initial draw fallback.

## 6. Composer behaviour

Lifts `aichat:994–1112, 2291–2369`.

- **Enter** sends; **Shift-Enter** newline (multiline TextInput, `aichat:1017–1046`).
- **Esc** cancels — shape from `aichat:2348–2368`, routed to `turn/interrupt`. Composer
  re-enables on `turn/error { code: "interrupted" }`.
- **Clear** blanks input. **Send** is the up-arrow pill (`aichat:1048–1112`).
- **`+`** attach: hidden / no-op in M1; slot kept for M3.
- **Thinking toggle**: dropped — Octos handles thinking server-side per profile. DSL
  line removed; row collapses cleanly.
- **TaskDock toggle**: new, stub view in M1; W04 fills in.

Cancel detail: `aichat:2348` commits the partial on cancel. We **don't** — § 4.4.
Partial is ephemeral; on `turn/error { interrupted }` we drop and re-hydrate.

## 7. Adaptations from aichat

Drop: `BackendType` + `ALL_BACKENDS` + `from_index/to_index` + per-backend
`system_prompt` + multi-LLM `create_agent` switch (`aichat:2332–2337, :2422–2432`) —
Octos serves all LLMs server-side. `read_key_file` / env-var probes (token in keychain).
`aichat_history.json` (`aichat:1144, :2623, :2641`) — replaced by § 5 cache.
`thinking_toggle` + Moonshot `restart_backend` (`aichat:2422–2432`).
`glass_opacity_values` invariants (`aichat:2668–2706`) — keep slider, push tests to W10.

Keep (load-bearing, semantic-equivalent port): streaming pipeline (delta append, remend,
fade-in shader); diagram fence safety (`aichat:1238–1335`);
`unwrap_outer_markdown_fence` (`aichat:1203–1228`); `wrap_bare_latex`
(`aichat:14, 1847`); per-instance font overrides (`aichat:368–448, 485–582`); Cmd-click
`robius_open` (`aichat:2434–2469`).

Adapt `MatchEvent` + `AgentEvent` consumer (`aichat:2413–2515, 2565–2654`): replace
`BackendType` dispatch with profile/session switching. New variants in
`libs/makepad_ai/src/agent.rs` — `ToolStarted`, `ToolProgress`, `ToolCompleted`,
`TaskUpdated`, `ApprovalRequested`, `ProgressUpdated`. W03 consumes `TextDelta`,
`ThinkingDelta`, `TurnComplete`, `PromptError`; rest route to W04/W05 stubs.

## 8. Deliverables

Days, one engineer.

1. **DSL templates** (1d). `aichat:343–614` → `app/src/main.rs`. Verify CJK, arrows,
   math, emoji.
2. **Streaming pipeline** (1d). `aichat:14–16, 1799–1819, 2576–2630` minus
   stateless/history-injection. Wire to `AppState.ephemeral`.
3. **Helpers** (0.5d). `unwrap_outer_markdown_fence`, `wrap_bare_latex`,
   `scan_diagram_fence_status`, `assistant_message_is_safe_to_store` from
   `aichat:1203–1336` → `app/app/chat.rs`.
4. **MermaidSvgView** (1d). `aichat:1384–1722` → `octos-app-render/src/mermaid.rs`.
5. **Composer** (1d). Lift `aichat:994–1112, 2291–2369`; rewire send → `turn/start`,
   cancel → `turn/interrupt`. Drop thinking toggle, hide `+`.
6. **SQLite cache** (1.5d). Schema + migrations + R/W in
   `octos-app-store/src/cache.rs`: `load_session`, `upsert_messages`, `evict_stale`,
   `clear_all`.
7. **Hydrate dance** (1d). REST + WS open in `app/backend/octos_ui.rs`; reducer in
   `octos-app-store`.
8. **`AgentEvent` extensions** (0.5d). Six new variants in
   `libs/makepad_ai/src/agent.rs`.
9. **Reconnect harness** (1d). Mock server drops WS mid-delta; assert no dupes, no
   orphan partial, composer re-enables.
10. **Smoke checklist** (0.5d). `05-AICHAT-REUSE-MAP.md` eight-item run.

Total **9 days**. Critical path 1 → 2 → 5 → 7. Items 4, 6, 8 parallelize.

## 9. Tests & verification

**Ported unit.** Diagram-fence safety suite at `aichat:2658–2889` —
`history_injection_allows_valid_diagram_assistant_messages`,
`_rejects_incomplete_diagram_`, `store_keeps_reply_with_unclosed_non_diagram_fence`,
`outer_markdown_wrapper_is_unwrapped_before_diagram_safety_scan`, et al. Port verbatim
minus `BackendType` cases into `crates/octos-app-render/tests/`.

**New unit (hydrate + cache).** Cold open populates messages from REST and upserts.
Warm open draws cached on first frame, REST replaces in-place without re-scrolling.
`cursor_invalid` drops cursor and refetches. Eviction drops 31-day-stale sessions on
next app start.

**Integration (reconnect-mid-stream).** Fake server (axum + wiremock) accepts
`turn/start`, emits 5 `message/delta` over 500 ms, drops connection. Client reconnects
with `session/open { after: cursor }`. Server replays no deltas (per § 4.5) then
`turn/completed`. Assert: messages has post-REST count; ephemeral cleared; composer
re-enabled.

**Manual smoke** (eight from `05-AICHAT-REUSE-MAP.md`): CJK inline code; Unicode arrows
/ math in prose; mid-stream code blocks don't reflow; fade-in smooth; cancel freezes
partial cleanly; Cmd-click opens OS browser; session-switch mid-stream doesn't bleed
deltas; reconnect lands on correct final.

## 10. Exit criteria

Done at M1 when:

- User types, hits Enter, sees streaming markdown / math / mermaid / diagram / code
  render at the same fidelity as `aichat`.
- Cancel mid-stream is clean: composer re-enables, no orphan partial, no stuck
  "Thinking..." status.
- Switching sessions swaps the thread immediately; cache draws first, REST replaces, no
  flicker.
- Forced WS drop mid-stream reconnects, applies replay, lands on correct final —
  verified by § 9 integration test.
- Ported aichat tests pass; new hydrate / cache / reconnect tests pass.
- Manual smoke checklist passes on macOS.

## 11. Risks

- **Splash sandbox not ready.** Widget stays hooked; interpreter gated off. If sandbox
  slips past M2, chat still works — Splash blocks render as inert code.
- **Reconnect-mid-stream.** Three orthogonal concerns — ephemeral cleanup, REST refetch,
  replay. § 9 integration test blocks M1 exit.
- **REST history shape drift.** `GET /api/sessions/{id}/messages` implied by `octos-web`
  but not pinned in `03-PROTOCOL-CONTRACT.md`. Track under `06-WORKSTREAMS.md`
  "Coordination & open asks".
- **CodeView font override regressions.** `aichat:514–542` overrides `draw_text` and
  `draw_gutter`; easy to miss one and ship CJK-tofu. Smoke checklist catches.
- **Markdown widget API drift.** Pin `makepad_widgets` to aichat's rev; bumps go through
  W09 with a smoke pass.

## 12. Open questions

- **Per-message editing** (in-place edit + resubmit). In `octos-web`, not `aichat`.
  Defer to M3 unless dogfooding demands.
- **Streaming-buffer cap.** 4 MB then refuse appends? Decide by M1 exit.
- **REST hydrate page size.** Assume "everything"; `?limit` wired in.
- **Cache schema versioning.** `PRAGMA user_version`; finalise in W04 with tasks /
  files cache.
- **Session deletion signal.** No `session/deleted` notif. For M1, accept ghost cache
  rows; track with server team.
