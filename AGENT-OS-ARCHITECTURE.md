# Agent OS — Architecture Design (v3)

Status: **v3 — refined after building & validating the AMA + 3-domain-agent MVP on-device
(2026-07-14).** v2 (2026-07-13) was a code-grounded design review; v3 folds in what the
running system taught us. The v2 verdict still holds — *feasible as "multiple octos sessions
rendered as app windows"; the near-term work is a **control plane + a truly multi-session
client**, not LLM orchestration* — and v3 sharpens five load-bearing points with evidence.
Read **§0 (as-built)** and **§16 (v3 refinements)** first; §1–§15 are the v2 design of record.
Written high level → low level.

Goal: evolve octos-app from a one-shot card generator into a multi-agent system where each
user-facing *app* (weather, travel, shopping…) is an autonomous, long-lived **octos session**
that streams to its own window, coordinated by a **thin, advisory** Activity Management Agent
(AMA), with the **client owning all window/lifecycle decisions**.

---

## 0. As-built status (2026-07-14) — what actually runs today

A working slice of this architecture ships and was validated on real phones (OnePlus 6/6T).
It is **Phase 0 + most of Phase 1**, with a **thin AMA already wired end-to-end** — proving the
*shape* (peer sessions, advisory routing, direct-to-window generation) ahead of the Phase-2
multi-session transport that will make it robust.

**What exists** (`app/src/main.rs`, unless noted):
- **N concurrent peer sessions, one AMA.** `clear_chat()` opens 3 domain app-agent sessions
  (`AppRecord{ domain: "weather" | "stock" | "news" }`) plus an AMA session — all live at once
  on one embedded `liboctos.so serve --stdio` kernel.
- **Decision → activation, working.** `submit_prompt` sends the intent **only** to the AMA
  (persona inlined per-turn via `AMA_SYSTEM_PROMPT`, held in `pending_intent`); on the AMA's
  `TurnComplete` the leading token is parsed to an `app_id`; `route_to_app(app_id)` sets
  `foreground` and hands the domain agent a domain-locked prompt (`app_splash_router_for`). A
  background app's streamed events are badged by `app_of_prompt(prompt_id)`, never written to the
  visible surface.
- **Direct-to-window generation.** The routed domain agent emits the `runsplash` card **itself**
  as its streamed answer — the sub-agent relay (and its card-truncation bug) is gone.
- **Per-appid memory injection.** `a2app/apps/<domain>/{app.md, exemplars/*.splash}` + shared
  `framework.md`/`widgets/` are deployed as an `app-cards/` tree under the profile memory dir;
  the octos kernel **assembles them itself at inject time** (`octos-memory` →
  `assemble_app_cards`) — no build step, no generated `MEMORY.md` artifact (cap
  `config.memory.max_inject_tokens = 16000`).
- **Live-data binding.** Cards call `sys.weather/airquality/stock/stockbar/news` helpers that
  fetch real values at render time (open-meteo, Yahoo Finance, Hacker News) — the LLM writes
  `sys.stock("AAPL","price")`, never a number. Re-eval on data arrival via `DATA_FETCH_EPOCH`.
- **iOS-grade cards.** Weather, stock (iOS-Stocks: gridlined intraday area chart with y-axis +
  range selector + frosted stat grid), and news (iOS-News list) cards render full-screen.

**What is NOT built yet** (still the real work — see §7, §10, §11): the multi-session *transport*
(today one shared cursor; §10a), any `session/open` prompt/memory/model seed (persona + memory are
injected client-side as prompt text, not sealed contracts; §10b/§7), the AMA↔client↔app **control
plane** (`focus/open/close`, `needs_focus/idle`; §10f), and lifecycle/cache-loss reconciliation
(§2.5). The MVP proves intent, not durability.

**The honest gap:** the AMA and app agents are peer sessions coordinated by **client-side
convention over ordinary turns/events**, not by a protocol. That was the fastest way to validate
the shape; §7 is what makes it an OS.

---

## 1. Vision & mental model

An agent version of a tabbed OS. The user works inside an *app context*; each app is a
persistent agent that refines its UI and holds state over time. A supervisor (AMA) *proposes*
which app should be in focus and when to open/close apps; the client decides and renders.

**Mental model (corrected): browser tabs / chat sessions with subscriptions + durable
ledgers — NOT Android processes.** octos sessions are cached runtimes + JSONL history, not OS
processes with lifecycle callbacks, permissions, saved-instance-state, or process-death
contracts. Server "eviction" = **cache loss**, not "activity stopped." Consequence that shapes
everything below: **the client owns durable window/app state and reconciles it against session
streams.** The Android analogy is a UX metaphor only; the implementation is tabs + ledgers.

## 2. Goals / Non-goals

**Goals**
- Apps are **autonomous, stateful, long-lived** octos sessions (refine + interact, not one-shot).
- **Isolation**: one app's context/failure does not pollute another.
- **Direct-to-window** output: each app session streams to its own view — no intermediary re-emits content (this is what kills the sub-agent relay/truncation problem).
- **Bounded cost**: per-app context stays small; idle apps drop from cache and rehydrate.
- **Cheap, non-blocking routing**: choosing which app handles input adds ~no latency in the common case.
- **Extensible**: adding an app = an app package (prompt + memory + tools/permissions + version), no core change.

**Non-goals (now)**
- Separate OS processes per app (too heavy on-device; one embedded server hosts many sessions).
- True process-death/saved-instance-state semantics (we get cache-loss + ledger rehydrate instead).
- Autonomous inter-app collaboration (apps independent; only advisory housekeeping via AMA).

## 3. Core principles (load-bearing, post-review)

1. **Peer sessions, not sub-agents.** *(Review: strongest part of the design.)* Each app is a
   peer octos session with `session_id`-scoped events + persisted history, streaming directly to
   its window. Sub-agents remain fine for an app's *internal* work, but are a poor abstraction
   for independent app windows (they force the content relay we're escaping).
2. **The AMA is advisory; the client decides.** AMA emits *proposals* (focus / open / close /
   confidence). The **client** owns window/lifecycle decisions and durable state. If the AMA
   "owns" windows, it contradicts client-as-window-manager. Never let AMA output be authoritative.
3. **Routing ≠ generation.** AMA output is a small typed decision, never content — so it can't
   "lose content in translation." Make it a classifier only.
4. **Fan-out, not pass-through — but cancel ≠ rollback.** Broadcasting to foreground+AMA in
   parallel avoids added latency, BUT interrupt only stops *future* work; already-streamed deltas
   and tool side-effects are NOT undone. Speculative routing must assume the wrong app may have
   already shown output. (See §9.)
5. **Per-appid memory = the app package.** `a2app/apps/<appid>/` (spec + exemplars + widget docs)
   is the durable app definition and survives every phase. But it must become a *contract*
   passed at session creation (§7), not just prompt text.
6. **The control plane is a first-class artifact (§7), and it's the real build work.** *(Review:
   the AMA↔client↔app protocol does not exist in octos today.)*

## 4. System overview

```
┌──────────────────────── Phone: octos-app (Makepad client = WINDOW MANAGER + owner of truth) ─┐
│  • per-app records: {sessionId, cursor, render buffer, window state, error state}             │
│  • view stack (foreground app + background app records) + launcher/switcher (explicit UI)     │
│  • DECIDES focus/open/close (AMA only proposes); reconciles state vs. streams on cache loss    │
│  • Splash runtime (runsplash renderer, WeatherIcon, sys.* helpers)                            │
└──────────┬──────────────────────────── stdio, session-multiplexed ui-protocol ──────┬─────────┘
           │ per-session turns/events + a CONTROL side-channel (new, §7)               │
┌──────────▼──────────────────────── Embedded octos server (liboctos.so) ─────────────▼─────────┐
│  AMA session (advisory)      Weather session          Shopping session        …               │
│   intent→{focus/open/close   full agent: own          full agent: own                          │
│   proposal + confidence}     history/memory/tools     history/memory/tools                     │
│                              ─ streams to its window ─                                          │
│  App packages (shared memory): a2app/apps/<appid>/{app.md, exemplars/}, widgets/, framework.md │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

## 5. Components

- **Client / Window Manager (octos-app).** Owner of truth. Holds a per-app record
  `{sessionId, cursor, renderBuffer, windowState, errorState}`; view stack + explicit switcher;
  input dispatch; **decides** focus/open/close (executing or rejecting AMA proposals); reconciles
  app state when the server drops a cached runtime. Renders each session's stream via runsplash.
- **AMA (advisory session).** Thin classifier. Input → typed *proposal*
  `{stay | switch(appId) | open(appId, seed) | close(appId), confidence, requestId}`. Cheap
  model tier. Never renders, never authoritative, rate-limited.
- **App session (one octos session per app).** Full agent: own history/memory/workspace/tool
  policy; seeded from its app package; handles build + refinement statefully; streams direct to
  its window; may *request* (not force) escalation to AMA.
- **App package (per-appid memory + manifest).** `a2app/apps/<appid>/…` today; must grow a
  manifest: prompt, tools, workspace root, memory scope, model lane, permissions, renderer type,
  version. The unit you "install."
- **Splash runtime (aichat).** The UI toolkit (runsplash + widgets); agent-agnostic; unchanged.
- **Embedded octos server.** One `serve --stdio` process hosting many sessions.

## 6. Key flows

- **Launch** — AMA proposes `open(weather)` → **client** creates a session seeded with the weather
  package, foregrounds it → app builds the card → streams to the window.
- **Refine in-app** — foreground = weather → input goes to the weather session (remembers current
  card/state) → emits updated card. Stateful; no re-derivation.
- **Implicit switch** — input fans out to foreground app (starts) + AMA (classifies). AMA proposes
  `switch(travel)` → **client** interrupts weather, foregrounds travel, replays input. *(Caveat:
  weather may have shown a few deltas first — §9.)*
- **Explicit switch** — user taps in the switcher → client foregrounds; no AMA/NLU.
- **Close / cache-loss** — client tears down (user intent) or server evicts a cached runtime; on
  eviction the client keeps its app record and rehydrates from the session ledger on next use.

## 7. Control plane & contracts (the real work — new in v2)

None of this exists in octos today (review: side-channel = **NOT SUPPORTED**). It must be
designed and added to `octos-core` ui-protocol + the transport.

- **App-session creation contract.** Extend `session/open` (or a wrapper) to accept an
  **app-package reference** that seeds the session's system prompt + memory scope + tool
  policy + model lane — first-class, client-passed, not profile-baked. *(Today `session/open`
  carries `session_id/topic/profile_id/cwd/sandbox/after` only; prompt/memory come from profile
  config — insufficient.)*
- **AMA control messages** (client↔AMA): typed proposals
  `{decision, appId, seed?, confidence, rationale, requestId}` + a client `ack/apply/reject`.
  Advisory only.
- **App housekeeping** (app→client/AMA): `needs_focus`, `idle`, `status`, `handoff(payload)` —
  with **permissions + rate limits** (a compromised app must not spam focus/suppress switches).
- **Focus/lifecycle vocabulary** the protocol currently lacks: `focus`, `detach`, `open_app`,
  `close_app`. *(Today: only `session/open/turn/start/turn/interrupt/hydrate/list/delete`.)*
- **Cancellation policy.** Define what "cancel" means after streamed output/side-effects (it is
  NOT rollback). Likely: mark the interrupted turn's UI as superseded client-side; treat any tool
  side-effects as committed; never assume erasure.

## 8. (folded into §5–7 above)

## 9. Routing design & risks (rewritten, honest)

Objective: know which app owns each input without blocking latency. Mechanism: fan-out +
speculative execution + explicit-UI-first + self-escalation. **Real risks (from review):**

- **Premature output.** The foreground app streams *visible* deltas before the AMA verdict/
  interrupt arrives → the user briefly sees the wrong app's answer. Mitigate: a short "hold
  first paint" window on low-confidence inputs; or render foreground output optimistically but
  be ready to visually supersede it.
- **Cancel ≠ rollback.** Interrupt stops future tokens only; tool calls/approvals/artifacts and
  shown deltas persist. Mitigate: don't let speculative turns run side-effecting tools before a
  focus decision; design turns so the risky work happens after first paint.
- **One active turn per session.** A re-route target that's already busy can't accept the turn
  cleanly → need explicit queue/reject semantics per app session.
- **Self-escalation authority.** An app judging "not mine" has already spent tokens/UI/tools, and
  a buggy/injected app could spam or suppress switches → the housekeeping channel needs
  permissions + rate limits (§7).
- **Cost.** Every fan-out runs ≥2 agents. Needs per-app/AMA/turn budgets + attribution
  (turn-completion carries token counts today, but no budgets).

Bias the system toward **explicit UI switching** (launcher/switcher/gestures) so implicit NLU
routing is the minority path — smaller blast radius for every risk above.

## 10. octos capability reality (verified against code, replaces "assumptions")

| Need | Status | Gap to close |
|---|---|---|
| (a) Many sessions over one stdio pipe | **PARTIAL** | Server multiplexes session-scoped turns/events (`octos-cli/.../ui_protocol.rs:4591,10536`), but **no `focus`/`detach` protocol**, and the client backend assumes ≤1 session (`app/src/backend/octos_ui.rs:625`). Add multi-session client + focus/detach verbs. |
| (b) On-demand session seeded w/ prompt+memory | **PARTIAL** | `session/open` seeds `cwd/sandbox` only (`octos-core/.../ui_protocol.rs:1731`); prompt/memory are profile-baked (`runtime/session.rs:488`, `runtime/profile.rs:245`). Add the app-package seed param (§7). |
| (c) Cancel in-flight turn | **SUPPORTED (no rollback)** | `turn/interrupt` works (`ui_protocol.rs:11137,19668`) but deltas/side-effects already emitted persist (`:19694`). Define cancellation policy (§7). |
| (d) Suspend / rehydrate for eviction | **PARTIAL** | TTL/LRU runtime cache + eviction (`runtime/cache.rs:254,457`) + chat-state hydrate (`ui_protocol.rs:2541`); **no full app-state suspend/resume**, no `session/suspend|resume`. Model as cache-loss + client-owned state. |
| (e) Cheap model tier for AMA | **PARTIAL** | Achievable via profile/topic/lane or low reasoning effort (`runtime/profile.rs:326`), but **no per-session model field**. Make AMA a cheap-lane/profile. |
| (f) AMA↔client/app control channel | **NOT SUPPORTED** | No focus/switch/open/close commands, no `needs_focus/idle/status` events (`ui_protocol.rs:943`; transport `lib.rs:128,174`). **Build it (§7) — the central new work.** |

## 11. Phased plan (re-sequenced after review)

- **Phase 0 — Reliable single-agent generation.** Client injects the focused app's memory into
  one main-agent call; drop the sub-agent/relay. *Honest caveat:* the protocol has no client
  system-prompt field, so this is prompt **injection** (works today; `app/src/main.rs:28`), not
  clean app seeding. Kills truncation now.
- **Phase 1 — Apps as client-owned, refinable objects.** App-instance records + launcher/switcher
  + stateful refine, still one session. Big UX win; does **not** validate the multi-agent arch.
- **Phase 2 — Multi-session CLIENT (the real foundation; PROMOTED).** Make the transport/backend
  truly multi-session: per-session cursors/streams/window-state (today: single shared cursor,
  reset on open, persistence noop — `transport/proto.rs:44,87`, `transport/cursor/mod.rs:1`), N
  live sessions, one app = one session streaming direct-to-window. Relay is gone for good.
- **Phase 2.5 — Lifecycle semantics (decided EARLY, not late).** Define cache-loss/rehydrate +
  client state reconciliation before autonomy/routing depend on it. *(Review: Phase-4 hardening
  was mis-scoped — lifecycle is core.)*
- **Phase 3 — Control plane + thin AMA.** Add the §7 protocol (app-package seed, AMA proposals,
  housekeeping, focus/open/close) to octos-core + transport; then the AMA classifier + fan-out.
  This is **protocol work first**, orchestration second.
- **Phase 4 — OS-grade.** App manifests/packages, per-app auth/permissions, cost budgets +
  attribution, crash/restart recovery of the stdio child, cross-app handoff, test matrix.

Property: per-appid memory + Splash renderer ride unchanged through all phases. 0→1 is client UX;
**2 is the foundation**; 2.5 fixes lifecycle early; 3 is the protocol/control-plane build.

## 12. Missing components (must design before/with the build)

App manifest/package contract · per-app auth + tool permissions (can't share one profile-level
surface) · AMA protocol (typed decisions, confidence, requestIds, cancellation policy, rate
limits) · multi-session client state (per-session cursor/buffer/window/error) · crash/restart
recovery of the embedded stdio child · cost accounting + attribution · test strategy (interrupt
races, late AMA decisions, cursor replay, app crash recovery, routing false pos/neg, tool
side-effect cancellation, mobile cold-start).

## 13. Top blockers to resolve BEFORE building (review's list)

1. **App-session creation contract** — how an app package seeds prompt/memory/tools/model/permissions/version.
2. **Control messages** — the AMA↔client↔app protocol (§7), advisory + permissioned + rate-limited.
3. **Cancellation semantics** — what "cancel" means after streamed output/side-effects.
4. **Lifecycle model** — replace Android suspend with octos cache/hydrate + client-owned state.
5. **Multi-session client** — per-session cursors/streams/window-state in the transport + app.

## 14. What carries forward / what's dropped

**Carries forward:** per-appid memory (`a2app/`), runsplash renderer + widgets, generation
prompts (→ app packages), on-device provisioning know-how, and the *validated* peer-agent core.
**Dropped (scaffolding for the wrong shape):** the `spawn` sub-agent + relay, the message-router,
`OCTOS_SKILLS_PATH` read-zone, the `RUST_LOG` probe.

## 15. Open decisions

- **D1** Ship Phase 0–1 (reliable, refinable single-agent apps) as a product milestone before the 2→3 protocol build? → **leaning yes:** §0 shows Phase 0–1 already delivers real product value (iOS-grade apps) and de-risks the shape. (See R1.)
- **D2** AMA router: rules+embeddings vs. small LLM vs. hybrid. → **resolved direction: hybrid, two-tier** (R4).
- **D3** How much of the control plane lives in octos-core (protocol) vs. a thin client-side convention over existing turns/events? (Review leans: it needs real protocol support, not a convention.) → still open; R7 raises the cost of *not* having it (persona/memory re-injected per turn).
- **D4** Optimistic-render-then-supersede vs. hold-first-paint for low-confidence routing. → **mostly mooted** (R1): we do not speculatively render, so there is no premature wrong-app paint to supersede.

---

## 16. v3 refinements (what the running MVP taught us)

Seven load-bearing corrections/sharpenings, each grounded in the as-built system (§0) or in
building the first iOS-grade app cards. R1 corrects the routing *shape*; R2–R4 attack the latency
that shape costs; R5–R7 harden memory, extension, and persona.

**R1 — Routing is AMA-first *sequential*, not fan-out. Own it in the design.**
v2 §4/§9 (and stale code comments at `main.rs:52-54,3374`) assume *"broadcast to foreground+AMA in
parallel + speculative execution."* The code does **not** do this and **should not**: `submit_prompt`
prompts **only** the AMA; the domain agent runs **after** `route_to_app`. Reason: domain agents are
**domain-locked** — the weather agent has no city for `"TSLA"`, the stock agent has nothing for
`"top news"`. Speculatively running the foreground app on an out-of-domain intent yields *garbage*,
not a useful first paint. So the §9 *"premature output"* risk largely **evaporates because we don't
speculate** (D4). The price is latency: AMA (~7 s) **then** generation (~30 s), serial. v3 keeps
AMA-first and buys the latency back with R2–R4 instead of speculation. Action: delete the
"broadcast/fan-out" language from the code comments and §4/§9; the honest model is *classify →
then activate one agent*.

**R2 — Parse the decision on the first line, not on `TurnComplete`.**
The AMA is contracted to reply *"exactly one short line: `<app id> — <reason>`."* Today the client
waits for the whole turn to complete (`main.rs:6062`) before routing. Route as soon as the **first
newline** of `ama_text` lands (the app id is the leading token already parsed at `main.rs:6070`).
Frees the tail of the AMA turn (reason + stop tokens) from the critical path — a latency win with
zero protocol change.

**R3 — In-app refinement must skip the AMA.**
When an app is foreground and the input *refines the current card* ("make it dark", "change to
Tokyo", "add volume"), route **directly** to the foreground agent; consult the AMA **only** when the
input plausibly names another domain. This is §3's *explicit-UI-first* principle made concrete: the
refine loop (the most common in-app action) must not pay the AMA round-trip. Cheap heuristic: if
`foreground` is set and the input carries no other-domain trigger term, skip routing; the AMA still
arbitrates genuinely new/ambiguous intents. Pairs with the app being a **stateful** session that
remembers its current card (§6 "refine in-app").

**R4 — Two-tier AMA: fast local classifier → LLM fallback (resolves D2).**
Most intents (`"AAPL"`, `"weather in Paris"`, `"top news"`, `"英伟达股价"`) are classifiable by cheap
**rules + embeddings** at ~0 latency and ~0 cost. Make the AMA two-tier: a **local** pass routes the
confident majority with no model call; the **LLM AMA session** is the fallback for genuinely
ambiguous input only. This removes the ~7 s AMA leg from the common path while preserving principle
3 (*cheap, non-blocking routing*). The LLM AMA stays as the escape hatch, not the default.

**R5 — Memory must be *per-domain*, not one global blob (the token-budget bug, formalized).**
As-built, octos's `assemble_app_cards` (in `octos-memory`) concatenates **every** app package into
**one** injected memory block for **every** agent's every turn. Two real failures: **(a)** the stock
agent carries the weather+news specs it never needs — breaks *isolation* and *"per-app context stays
small"*; **(b)** the block grows O(apps) and, past `config.memory.max_inject_tokens` (16000), the
kernel silently drops the **tail** app — a data-loss cliff that gets worse with every app added.
Refinement: **scope memory by domain** — each domain agent gets only `apps/<domain>/` + shared
`framework.md`/`widgets/`; the **AMA** gets only a tiny **app registry** (`domain → one-line
description`), never the card specs (it classifies, it doesn't generate). Now that assembly lives in
octos, this is a natural filter *inside* `assemble_app_cards` (select the active app subdir per
session) — no build step to change. Long-term: the §7 **app-package seed at `session/open`** carries
the memory scope. This is the concrete form of **principle 5**.

**R6 — Two extension axes: app *packages* (content, no build) vs. data *capabilities* (shared native surface).**
v2's *"adding an app = an app package, no core change"* is only **half true**, and this session proved
it: the iOS-Stocks card needed a **new** `sys.stockbar` helper, a `maxh` parameter, and consistent
2-decimal money formatting — all **framework (native) changes in `widgets/src/splash.rs`, rebuilt
into the APK**. Name the two axes:
  - **App package** (`a2app/apps/<domain>/`: prompt + spec + exemplars) — pure content. Adding or
    refining a card that **reuses existing** `sys.*` helpers is **no build**.
  - **Data capability** (a `sys.*` helper) — a shared, **versioned, native** primitive reused across
    apps. A genuinely new live-data source is a **framework change**, not an app change.
  The app package should **declare the capabilities it needs** (a manifest field), so the build/deploy
  **fails fast** on a missing helper instead of the card silently rendering `"—"`. Treat the `sys.*`
  surface as a **shared stdlib** (keep `body_binds_live_data()` and the helper list in sync when it
  grows), not per-app glue. This refines both principle "extensible" (§2) and the §5 app-package
  contract to include a capability dependency.

**R7 — Persona hygiene: seal the role once; stop re-injecting it.**
As-built, domain agents have an **empty** session system prompt
(`OCTOS_PLACEHOLDER_SYSTEM_PROMPT = ""`, `main.rs:32`) and receive **all** instructions from the
per-message `app_splash_router_for` wrapper; the **AMA** persona is **duplicated** (session prompt
*and* re-appended in every routing message, `main.rs:4147`). Both are fragile: a dispatch that omits
the wrapper yields an agent with **no instructions**, and re-injection burns tokens every turn. This
is precisely why §7's **app-package seed at `session/open`** is load-bearing — persona + memory scope
+ tool policy should be **sealed into the session once**, not re-sent per turn. Until the protocol
lands: give domain agents a real (non-empty) session persona and **stop double-injecting** the AMA
prompt.

**Net:** R1 makes the design honest about the shape; R2–R4 recover the latency that honest shape
costs (first-line parse, skip-AMA-on-refine, two-tier classifier); R5–R7 are the correctness/scale
fixes (scoped memory, the app-vs-capability split, sealed personas) that turn the MVP into something
that survives adding the 4th, 10th, 40th app. None require the full §7 control plane to *start* — but
all three point at it as the durable home.
