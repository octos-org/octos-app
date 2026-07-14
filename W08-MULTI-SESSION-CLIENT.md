# W08 — Multi-session client (Phase 2 of AGENT-OS)

Scope for making the **client** hold N live app sessions over the one stdio connection, each
with its **own cursor, stream, and window state**, switchable by **explicit UI**. This is the
foundation phase of `AGENT-OS-ARCHITECTURE.md` (§11, Phase 2 — promoted to *the* foundation by
the codex review). Aligns with the existing W08 "multi-tenancy" workstream and builds on the
W01 transport + W04 cursor scaffolding.

## Boundary
- **In:** N sessions on one connection; per-session cursor/stream/window state; explicit
  launcher/switcher; correct reconnect-replay per session.
- **Out (→ Phase 3):** the AMA, fan-out routing, control-plane verbs (`focus/open/close`),
  session `detach`/`suspend`. Phase 2 is pure client work — no octos-core protocol changes.

## Current state (grounded in code)
Already multi-session-ready: server multiplexes session-scoped events; app has
`session_keys`/`session_ids` maps (`app/src/backend/octos_ui.rs:62,65`) and routes incoming
events by `session_id` (`:241`); `CursorStore = HashMap<SessionKey,UiCursor>` + a `CursorPersist`
trait already exist (`crates/octos-app-transport/src/cursor/mod.rs`).

Single-session chokepoints (the whole job):
1. **One global cursor** — `SharedState.cursor: Option<UiCursor>` (`proto.rs:46`), overwritten by
   whichever session's notification arrives last (`proto.rs:355`). **Core bug.**
2. Reconnect bracket reads it (`proto.rs:90`); session-switch resets it (`proto.rs:98`).
3. "Pick the first/only session" shortcuts (`octos_ui.rs:629` — the "W08 will multiplex" TODO;
   `:262`).
4. Client holds a single `agent`/`session_id`/`current_prompt` (`main.rs:3235–3237`); no
   foreground-app concept.

## Work breakdown (bottom-up, each layer independently testable)

**Layer 1 — Transport: cursor → per-session** *(isolated, unit-testable; the first PR)*
- Replace `SharedState.cursor: Option<UiCursor>` with the existing `CursorStore`.
- Incoming durable notification (`proto.rs:355`): key the cursor update by the payload's
  `session_id` (`store.set(session, cursor)`), not a global.
- `OpenSession` bracket (`proto.rs:90`): `after` = that session's cursor (`store.get(session)`).
- Drop `OpenSessionFresh`'s global reset (`proto.rs:98`).
- Reconnect: re-open **each** session from its own cursor (the server rejects a cursor from a
  different session).
- (Optional) wire `CursorPersist`→SQLite (W04) for restart durability; `Noop` is fine for MVP.

**Layer 2 — App backend (`octos_ui.rs`): target the right session**
- Add `active_session`; make ops session-targeted (`cancel_prompt` derives session from
  `prompt_id`, not `.values().next()`; same for `:262`); per-session prompt/turn/stream maps;
  audit event arms route by `session_id`.

**Layer 3 — Client/UI (`main.rs`): N app records + switcher**
- Per-app record `{sessionId, renderBuffer, windowState, errorState}` + `foreground_app`;
  view stack + explicit switcher; each app = a `session/open` with a fresh `session_id` + `cwd`/`topic`;
  background sessions stay subscribed + buffer.

## Key decisions
- **Background sessions stay subscribed (no detach)** — protocol has no `detach`; keeps Phase 2
  client-only. Bound live-session count + trim background buffers.
- **Reconnect re-opens each session from its own cursor.**
- **Respect one-active-turn-per-session** — don't `turn/start` on a session with an in-flight turn.

## Risks
- Cursor-overwrite fix is load-bearing for reconnect-replay — do it first, with tests.
- Reconnect storm (N re-opens); memory of N buffering sessions; no-detach bandwidth cost.

## Testable first increment
Two sessions on one connection stream concurrently; each cursor advances independently (extend
`cursor/mod.rs` round-trip test to two keys); switch foreground via UI; reconnect replays both
correctly. Proves the foundation with zero AMA.

## Sequencing
Layer 1 (transport cursor) → Layer 2 (backend targeting) → Layer 3 (UI switcher). Layer 1 is a
self-contained reviewable PR that removes the core bug.

---

## Layer 3 — client N-app records + switcher (detailed scope)

**Status of 1/1b/2:** done + verified (transport host-tested 18/18; backend Android-compiled).
Layer 3 is the UI-heavy piece that finally opens >1 session and lights up 1/2 end-to-end.

**The crux:** `ChatData` (messages + streaming + `a2app_state` cards) is a **single global**
`CHAT_DATA`, read/written in ~dozens of places. "Multiple app windows" needs each app's
conversation. Two ways to get there:

- **Path A — per-app in-memory `ChatData`.** App holds `Vec<AppRecord{ session_id, title,
  ChatData }>`; every `CHAT_DATA` access is repointed to the foreground app's data. Enables *live*
  background rendering, but the blast radius is large (dozens of call sites) and risky to verify
  without device iteration.
- **Path B — hydrate-on-switch (RECOMMENDED for v1).** Keep ONE `CHAT_DATA`. Each app = a server
  session (which already holds its own durable history). Switching foreground calls the EXISTING
  `resume_session()` → `session/hydrate` to reload that session's history into `CHAT_DATA`.
  Background apps live on the server ledger; no per-app in-memory conversation. This is the
  "browser tabs + durable ledger" model codex endorsed, reuses machinery that already works, and
  keeps the blast radius small.

**Layer 3 sub-steps (Path B):**
1. **App-record model** on `App`: `Vec<AppRecord{ session_id: SessionId, title: String }>` +
   `foreground: usize`; retire the single `session_id`/`current_prompt` in favor of "the
   foreground record's" session/prompt.
2. **New-app flow:** `create_session()` (fresh `SessionId`) → push record → foreground it →
   clear `CHAT_DATA` for the new app.
3. **Switcher UI:** a launcher/app-switcher surface (makepad `live_design!`) listing open apps;
   tap → set foreground → `resume_session(that session)` (hydrate) → `CHAT_DATA` shows it.
   Long-press / swipe → close (tear down that session).
4. **Foreground guard (small but IMPORTANT):** with N sessions live, incoming stream events for a
   BACKGROUND session must NOT write the global `CHAT_DATA`. Apply events to `CHAT_DATA` only when
   `event.session == foreground`; optionally buffer a lightweight "has updates" badge for others.
   (Events are already `session_id`-keyed from Layer 2, so this is a routing guard, not a rewrite.)
5. **Send/cancel target the foreground session** (already session-parameterized after Layer 2).

**Honest note:** Layer 3 is a UI feature best built with device iteration (the switcher layout,
foreground swap, hydrate timing) — unlike the correctness-critical, host-testable Layers 1/1b/2.
Recommend it as its own focused effort. First increment: the app-record model + the foreground
guard (small, compilable, de-risks the state model) before the switcher widget.
