# 06 — Workstreams

Master index of the work. The tactical plan is parallel where possible, serial where the
protocol or the UI shell forces it. Each workstream has its own page in `workstreams/W0x.md`
with full scope, deliverables, exit criteria, and tests.

## Sequencing principle

Three concurrent swimlanes, each owned by one agent / contractor:

- **Lane A — Spine.** Protocol client, app shell, store, build. Without these, nothing works.
  Mostly serial inside the lane; gating items for everyone else.
- **Lane B — Chat experience.** Composer, streaming, message renderer, sessions, tasks, files.
  This is where most of the aichat reuse happens.
- **Lane C — Producers.** Coding, studio, slides, sites. Build on Lane A's spine and Lane B's
  patterns. Mostly independent of each other.

Cross-lane: **Lane Q — QA & build.** Tests, packaging, telemetry. Pulled forward (W10/W09 don't
wait for M3).

```
M0 (decisions)   M1 (chat-only)            M2 (tools+approvals)         M3 (producers)
──────────────   ─────────────────         ──────────────────           ─────────────
Lane A:  W01-init →  W01-WS — W02 ───────→ W04-snapshot                 W08-profile
                     W08-auth                                            W09-mobile (deferred)
Lane B:                  W03-chat-MVP ───→ W03-streaming-final → W04-tasks → W05-approvals
                                                                            W04-files
Lane C:                                                       (waiting)  → W06-coding
                                                                            W07-studio
                                                                            W07-slides
                                                                            W07-sites
Lane Q:  W10-test-skel ───────────────────────→ W10-CI ─────────→ W09-packaging
```

Mark M0/M1/M2/M3 as the gating milestones the team commits to externally. Internal sub-tickets
sit inside each W0x doc.

## Workstream catalog

| ID | Title | Lane | Depends on | Lifts from | Doc |
|---|---|---|---|---|---|
| W01 | Protocol client & transport | A | — | `octos-core`, `octos-tui`'s transport | `workstreams/W01-protocol-client.md` |
| W02 | App shell & navigation | A | W01 | `aichat:618–1142` | `workstreams/W02-app-shell.md` |
| W03 | Chat experience | B | W01, W02 | `aichat` ChatList + composer + streaming pipeline | `workstreams/W03-chat.md` |
| W04 | Sessions, tasks, files | B | W01, W02 | `octos-web` REST patterns | `workstreams/W04-sessions-tasks-files.md` |
| W05 | Approvals & diff preview | B | W01, W04 | none — net new | `workstreams/W05-approvals-diff.md` |
| W06 | Coding workspace | C | W05 | `aichat`'s code_view | `workstreams/W06-coding-workspace.md` |
| W07 | Studio / Slides / Sites | C | W04 | aichat composer + thread | `workstreams/W07-studio-slides-sites.md` |
| W08 | Auth & multi-tenancy | A | W01 | none — net new | `workstreams/W08-auth-tenancy.md` |
| W09 | Build, packaging, release | Q | W02 | aichat's `Cargo.toml` | `workstreams/W09-build-packaging.md` |
| W10 | Testing & QA strategy | Q | W01 | aichat's existing tests | `workstreams/W10-testing-and-qa.md` |

## Dependency DAG

```
                  ┌─────────┐
                  │   W01   │  protocol client
                  └─────────┘
                       │
            ┌──────────┼──────────┐
            ▼          ▼          ▼
        ┌─────┐    ┌─────┐    ┌─────┐
        │ W02 │    │ W08 │    │ W10 │
        │shell│    │ auth│    │ test│
        └─────┘    └─────┘    └─────┘
            │                     │
       ┌────┴────┬────────┐       │
       ▼         ▼        ▼       │
   ┌─────┐  ┌─────┐  ┌─────┐      │
   │ W03 │  │ W04 │  │ W09 │ ─────┘
   │ chat│  │ s/t/f  │ pkg │
   └─────┘  └─────┘  └─────┘
       │         │
       │         ▼
       │     ┌─────┐
       │     │ W05 │ approvals
       │     └─────┘
       │         │
       │     ┌───┴───┐
       │     ▼       ▼
       │  ┌─────┐  ┌─────┐
       │  │ W06 │  │ W07 │
       │  │coding  │studio│
       │  └─────┘  └─────┘
       │
   (chat alone is M1)
```

## Milestones

### M0 — Decisions & spine init (week 0)

Owner: architect (yc). Output:

- Resolved open architectural questions (`01-ARCHITECTURE.md` §9):
  - one window vs many → settled (one)
  - Splash inline UI policy → settled (off in M1, sandboxed in M2)
  - dev LLM-direct flag → settled (kept behind `--dev-llm-direct`)
  - profile picker UX → settled (sidebar dropdown, header on wire)
- `octos-app/` repo created (separate from this planning tree); workspace skeleton committed.
- `octos-core` pinned to a known-good revision in `Cargo.toml`.

### M1 — Chat-only walking skeleton (weeks 1–4)

Output:

- W01 (transport, no approvals/tasks yet)
- W02 (app shell, sidebar, top bar, status label, profile picker stub)
- W08 (login + token storage + simple profile selection)
- W03 (chat thread, composer, streaming pipeline; lifts from aichat)
- W04 partial (session list + history hydrate; no tasks/files yet)
- W10 partial (unit tests on store + transport mocks)

Exit criteria: a user logs in, picks a profile, opens a session, has a streaming chat with
markdown / code / math / mermaid / diagram-kit rendering, cancels mid-turn cleanly, reconnects
after a forced WebSocket drop.

### M2 — Tools, approvals, tasks (weeks 5–7)

Output:

- W04 complete (task dock with live tool/task progress, file viewers)
- W05 (approval cards, typed payloads, diff preview)
- W09 (cargo-packager pipeline producing `.app` / `.msi` / `.AppImage`)
- W10 (CI green; integration tests against a fake server)

Exit criteria: a user runs an agent that wants to edit files; sees the diff; approves; sees
output stream into the task dock; the app survives a 30-second offline period without losing
state.

### M3 — Producer surfaces (weeks 8–11)

Output: W06 (coding), W07 (studio, slides, sites stubs).

Exit criteria: each producer has its own screen, can create a project, send a generation
request, see streamed output, and access produced artifacts. Slide present mode and full Sites
preview deferred.

## Agent swarm orchestration

The user explicitly asked to use parallel agents. Here is how each phase fans out:

### Drafting phase (now → M0)

- 4 recon agents in parallel ✅ (done — see this directory's foundation docs for synthesis)
- 10 workstream-doc drafters in parallel (kicked off after this index lands; see
  `workstreams/`)

### Implementation phase (M1+)

- Lane A (W01/W02/W08): one agent per workstream, **serial** inside the lane (W01 → W02 → W08)
- Lane B (W03/W04/W05): parallelizable once W02 lands. W03 and W04 in parallel; W05 starts
  after W04's task model is in.
- Lane C (W06/W07): parallel after W05 (typed approval payloads must exist before coding).
  W07's three sub-surfaces (studio/slides/sites) can fan to three more agents.
- Lane Q (W09/W10): always-on, one agent per stream, started in M0.

Total peak parallelism: ~6 agents during M1, ~9 during M2, ~12 during M3.

## Coordination & open asks

Items the architect tracks across workstreams. Each maps to a server-team conversation.

| Item | Owner | Status | Notes |
|---|---|---|---|
| Pinnable `octos-core` revision | server team | ask | needed before W01 enters M1 |
| Capability probe published | server team | ask | unblocks W05 typed approvals |
| Reconnect / cursor contract test fixtures | server team | ask | unblocks W01 / W10 |
| `/api/version` field with protocol semver | server team | ask | feeds W01 / W10 |
| One-shot REST endpoint for profile listing (no admin token) | server team | ask | unblocks W08 profile picker |
| Splash sandbox review | sandbox team | ask | unblocks Splash inline UI in M2 |

## Open decisions still owned by the architect

These don't block M0 → M1 but should land before M2.

- **Mobile / web target.** Out of scope per charter; revisit post-M3 only if user demand exists.
- **Light theme.** Out of M1; build palette switch infra in W02 so M2 can flip a single live-DSL
  block.
- **Multi-window.** One window only in M1; revisit if Coding+Chat+Studio juggling proves painful.
- **Local LLM dev mode hardness.** Keep `StatelessBackendAdapter` from aichat; gate behind a
  flag, document as dev-only. No QA cycles spent on it in M1.

## Reading order for a new contributor

1. `00-CHARTER.md` — what we're doing and why (10 min)
2. `06-WORKSTREAMS.md` — this doc (5 min)
3. Their assigned `workstreams/W0x.md` (15–20 min each)
4. `03-PROTOCOL-CONTRACT.md` if they touch the wire (15 min)
5. `01-ARCHITECTURE.md` for crate / threading model (15 min)
6. `05-AICHAT-REUSE-MAP.md` if their workstream is on Lane B (10 min)

The other docs (`02-API-DRIFT.md`, `04-IA-AND-NAVIGATION.md`) are reference, read on demand.
