# 00 — Charter

## Vision

A native desktop client for Octos that feels like a high-quality OS application, not a wrapped web
page. Single-binary Rust, GPU-rendered, online or offline-first, sharing the protocol layer with
`octos-tui` and the rendering layer with `aichat`.

It is the **operator console** for the agentic OS: chat with the agent, watch tools run, approve
mutations, browse artifacts, generate content (slides/sites/coding tasks). Admin and account
management stay in the web dashboard for now (see "Out of scope").

## Why now

Three things converge:

1. **UI Protocol v1** (`octos-ui/v1alpha1`) lands the JSON-RPC-over-WebSocket boundary with
   reconnect-safe cursors, durable event ledgers, and typed approval / diff payloads. The web
   client still hits a stitched REST/SSE/WS surface; the native client can target the new contract
   from day one. (See `03-PROTOCOL-CONTRACT.md`.)
2. **`aichat` example matured.** The liquid-glass shell, streaming-markdown pipeline,
   diagram-kit / mermaid / math / Splash renderers, font-fallback handling, and per-character
   fade-in animation are all production-grade in `~/home/aichat/examples/aichat/src/main.rs`.
   Most of the chat surface is a port, not a build. (See `05-AICHAT-REUSE-MAP.md`.)
3. **`octos-core` is shareable.** The protocol types — `RpcRequest/Response/Notification`,
   `TurnId`, `ApprovalId`, `UiCursor`, capability flags — already live in
   `crates/octos-core/src/ui_protocol.rs`. A native Rust client lifts that crate; the TS web app
   can't.

## In scope (M1 → M3)

- **M1 — Walking skeleton (chat-only).** Login, profile selection, session list, single chat
  thread with streaming, markdown + code + math + diagram + mermaid renderers, cancel/clear,
  history persistence. No coding/studio/slides yet. Targets UI Protocol v1 for turn lifecycle;
  REST for session list and history hydrate.
- **M2 — Tools & approvals.** Tool/task progress dock, typed-approval cards
  (`approval.typed.v1`), inline diff preview (`pane.snapshots.v1`), task-output drill-down,
  reconnect-safe cursors.
- **M3 — Producer surfaces.** Coding workspace (approval-driven), Studio (content generation),
  Slides editor, Sites editor. Each is a separate workstream that builds on M2.

## Out of scope (initially)

- **Admin / settings UI.** Profile CRUD, channel config, gateway lifecycle, model catalog,
  metrics, sub-account management — 50+ admin endpoints in `crates/octos-cli/src/api/admin.rs`,
  none in scope. Users link out to the existing web admin dashboard. (Revisit in M4 once the
  protocol stops moving.)
- **Mobile.** Makepad supports iOS/Android, but the IA, gestures, and dependency wiring (`maps`,
  `pdf`, `cef`) are tuned for desktop. Revisit post-M3.
- **Web build.** Makepad has a WASM target, but `aichat` pulls `cef` and large-asset fonts that
  won't fit. Stay desktop-only.
- **Self-hosted onboarding.** The "create account / generate setup script / paste-into-shell"
  flow stays in the web dashboard.
- **Offline mode.** Read-only view of saved sessions while disconnected is fine; mutating
  actions require server reachability.

## Non-goals

- **Feature parity with `octos-web` on day one.** The web app exposes Studio/Slides/Sites with a
  lot of small affordances. We ship the smallest *useful* surface for each producer in M3 and
  iterate.
- **Re-implementing the protocol.** We import `octos-core` and use its types as-is. If we need a
  shape it doesn't have, we land it upstream first.
- **Server-side changes.** This project is client-only. UPCRs (UPCR-2026-001 typed approvals,
  UPCR-2026-002 pane snapshots) are tracked because we depend on them, not because we own them.
- **A multi-purpose Makepad framework.** We may upstream small fixes to `makepad-widgets`, but
  the goal is shipping `octos-app`, not abstracting "the Makepad business app starter kit."

## Success criteria

| # | Criterion | How we measure |
|---|---|---|
| 1 | A power user prefers `octos-app` over `octos-web` for daily chat | manual feedback + session count after 2 weeks |
| 2 | Reconnect-after-network-drop is invisible | inject 30s offline mid-turn → no duplicate / lost messages |
| 3 | First-paint < 800ms on M1 Pro from cold launch | Makepad startup probe |
| 4 | Streaming feels at least as smooth as `aichat` | side-by-side perception test with same prompt |
| 5 | Single-binary install, signed and notarized for macOS | `cargo-packager` artifact opens without Gatekeeper warnings |
| 6 | Protocol-client crate shared with `octos-tui` (no fork) | both clients depend on the same `octos-core` revision |

## Risks (and how each is mitigated, by workstream)

| Risk | Mitigation | Owner workstream |
|---|---|---|
| Protocol still moving (M9 gates not all green) | Pin `octos-core` to a known-good rev; capability-negotiate; keep a REST fallback for uncovered surfaces | W01 (Protocol client) |
| Makepad live-DSL learning curve | Start by *copying* `aichat` widgets; only diverge when the feature requires it | W02 (App shell), W03 (Chat) |
| `aichat` is single-tenant, single-window — Octos is multi-profile, multi-session | Lift `aichat` widgets but rebuild navigation around `octos-core` types from day one | W02, W04 |
| Web dashboard parity creep | Hard line: anything `admin.rs`-shaped opens the web dashboard in a browser | W08 (Auth & tenancy) |
| LLM-emitted Splash blocks could exfiltrate state | Splash sandbox; ban network/file APIs in Splash interpreter | W03 (Chat) — track via Octos sandbox crate |
| API doc drift from `OCTOS_WEB_REST_API.md` | We lock against handler code, not the doc. See `02-API-DRIFT.md`. | W04 (Sessions/files) |

## Audience

This planning tree is read by:

- **Architects / tech leads** sequencing work across multiple agents/contractors.
- **Workstream owners** picking up a single doc (`workstreams/W0x`) and shipping it.
- **Reviewers** checking that scope and interfaces stay consistent.

If you're a future implementer reading just one doc: `06-WORKSTREAMS.md` has the dependency DAG;
read your assigned `workstreams/W0x.md`, then back-fill `01-ARCHITECTURE.md` and `03-PROTOCOL-CONTRACT.md`.
