# 01 — Architecture

A thin client over a moving server. The architecture has to make that explicit: where the
authoritative state lives, where the cursor is, what the client owns vs. caches, and how each layer
fails when the network drops.

## 1. System diagram

```
┌─────────────────────── octos-app (this project) ───────────────────────┐
│                                                                        │
│   ┌────────────────── ui (Makepad widget tree) ─────────────────────┐  │
│   │  AppShell · Sidebar · ChatThread · Composer · TaskDock · …      │  │
│   │  (live-DSL, GPU-rendered, lifts heavily from aichat/main.rs)    │  │
│   └────────────────────────────┬───────────────────────────────────┘   │
│                                │ Actions / Events (Makepad)            │
│   ┌────────────────────────────┴───────────────────────────────────┐   │
│   │  app::state — single owner of in-memory UI state               │   │
│   │  Sessions, Turns, Tools, Approvals, Tasks, Files, Auth         │   │
│   │  Pure reducer; no I/O. Holds last-applied UiCursor.            │   │
│   └────────────────────────────┬───────────────────────────────────┘   │
│                                │ commands ↓ / events ↑                 │
│   ┌────────────────────────────┴───────────────────────────────────┐   │
│   │  app::backend — Agent trait (lifted from makepad_ai)            │   │
│   │  - OctosUiAgent (this project): WebSocket JSON-RPC v2           │   │
│   │  - StatelessBackendAdapter (kept for offline / inference dev)   │   │
│   └────────────────────────────┬───────────────────────────────────┘   │
│                                │ wire ↕                                │
│   ┌────────────────────────────┴───────────────────────────────────┐   │
│   │  octos-core (imported from ~/home/octos/crates/octos-core)      │   │
│   │  ui_protocol::{RpcRequest, RpcResponse, RpcNotification, …}     │   │
│   │  Identity types: TurnId, SessionId, ApprovalId, UiCursor        │   │
│   └────────────────────────────┬───────────────────────────────────┘   │
│                                │                                       │
│   ┌────────────────────────────┴───────────────────────────────────┐   │
│   │  app::transport — WebSocket + reconnect + heartbeat + REST      │   │
│   │  - ws://…/api/ui-protocol/ws  (interactive)                     │   │
│   │  - https://…/api/sessions, /api/files (snapshot hydrate)        │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
                              │ network
                              ▼
                       Octos server (~/home/octos)
```

## 2. Crate / module layout

This is the proposed in-tree layout for the future `octos-app` repo. We pick names that don't
collide with `octos-*` server crates.

```
octos-app/
├─ Cargo.toml                  # workspace
├─ app/                        # the binary
│   ├─ Cargo.toml              # makepad-example-aichat-style deps + octos-core path/git
│   ├─ src/
│   │   ├─ main.rs             # app_main!(App); script_mod!{...}; live-DSL UI
│   │   ├─ app/
│   │   │   ├─ mod.rs
│   │   │   ├─ state.rs        # AppState reducer (sessions/turns/approvals/…)
│   │   │   ├─ shell.rs        # window, sidebar, top bar, glass slider
│   │   │   ├─ chat.rs         # composer + thread + streaming pipeline
│   │   │   ├─ task_dock.rs    # tool/task progress widget
│   │   │   ├─ approvals.rs    # typed approval cards + diff preview
│   │   │   ├─ sessions.rs     # session list / switcher
│   │   │   ├─ files.rs        # file browser, viewers (image/audio/video/md)
│   │   │   ├─ coding.rs       # M3
│   │   │   ├─ studio.rs       # M3
│   │   │   ├─ slides.rs       # M3
│   │   │   └─ sites.rs        # M3
│   │   ├─ backend/
│   │   │   ├─ mod.rs          # re-export Agent trait, choose impl
│   │   │   ├─ octos_ui.rs     # OctosUiAgent — protocol-aware Agent
│   │   │   └─ stateless.rs    # offline LLM-only (kept from aichat)
│   │   └─ resources/          # fonts (LXGW Mono, NotoSans, NotoColorEmoji, Liberation Mono)
│   └─ tests/                  # integration tests (wiremock / fake server)
├─ crates/
│   ├─ octos-app-transport/    # WebSocket + reconnect + REST snapshot client
│   ├─ octos-app-store/        # AppState + reducers (no Makepad dep, easy to test)
│   └─ octos-app-render/       # streaming-markdown renderer wrappers (lifted from aichat)
└─ vendor/                     # path-deps to ~/home/octos and ~/home/aichat for dev
```

Three things to call out:

1. **`octos-app-store` is Makepad-free.** All reducers + types are unit-testable with plain
   `cargo test`. Makes the workstream parallelizable: W01 owns transport, W04 owns store, W02/W03
   own UI.
2. **`octos-app-transport` doesn't know about Makepad either.** It exposes a channel-based API
   that the Makepad `Agent` trait wraps. This means we can write protocol tests against a fake
   `axum` server without booting the UI.
3. **`octos-core` is a dependency, not a fork.** We import it via path (during dev) and via git
   tag (in CI). If the protocol moves, we bump the tag in one place.

## 3. Layered model

| Layer | Lives in | Knows about | Tests |
|---|---|---|---|
| Wire | `octos-app-transport` | bytes, sockets, JSON-RPC envelopes, reconnect | mock WS server |
| Protocol | `octos-core` (imported) | typed RPC, capability flags, cursors | upstream's tests |
| Store | `octos-app-store` | typed events → AppState; reducers; selectors | pure unit tests |
| Backend | `app/backend/octos_ui.rs` | bridges transport ↔ store ↔ Makepad `Agent` trait | integration |
| UI state | `app/app/state.rs` | UI-specific concerns (focus, scroll position, tab) | golden tests |
| UI render | `app/app/*.rs` + `script_mod!` | Makepad live-DSL, draw, animations | manual + screenshot |

The split between "Store" and "UI state" matters: durable state (sessions, turns, approvals)
roundtrips through the protocol; ephemeral state (which tab is active, scroll offset) does not.

## 4. Threading model

Makepad is single-threaded for UI; events run on the main loop. The transport must not block it.

- **Main thread**: Makepad event loop, reducer, all UI work.
- **Transport thread (tokio current-thread runtime)**: owns the WebSocket. Posts notifications to
  the main thread via `Cx::post_action` (same pattern `aichat` already uses for streaming).
- **REST fetches**: spawn one-shot tasks on the same tokio runtime; complete via
  `Cx::post_action` with a typed result message.

We do **not** spawn the agent's own tokio threads inside the UI process for full LLM inference
(unlike `aichat` which does have a `StatelessBackendAdapter` for OpenAI/Claude direct API). All
inference goes through the Octos server. The stateless adapter stays only as a dev tool.

## 5. State model (high level)

`AppState` (in `octos-app-store`):

```rust
pub struct AppState {
    pub auth: Auth,                              // token, profile_id
    pub connection: Connection,                  // WS state machine
    pub sessions: SessionMap,                    // SessionId → Session
    pub current: Option<SessionId>,
    pub cursor: Option<UiCursor>,                // last applied
    pub turns: HashMap<TurnId, Turn>,
    pub approvals: HashMap<ApprovalId, Approval>,
    pub tasks: HashMap<TaskId, Task>,
    pub files: HashMap<FileHandle, FileMeta>,
    pub ephemeral: Ephemeral,                    // streaming tokens, transient toasts
}
```

Reducers consume `Event` (typed wrapper around `RpcNotification` plus local UI events) and emit a
new `AppState`. The UI subscribes via selectors; only the slices that changed redraw.

`ephemeral` carries the in-flight `message/delta` tokens (the protocol explicitly says these are
not durable, see `03-PROTOCOL-CONTRACT.md`). On `turn/completed`, the durable history is hydrated
from REST or the next snapshot, and `ephemeral.streaming_text` is dropped.

## 6. Persistence

Three buckets, each with a clear policy:

| Bucket | Where | Survives restart? | Authoritative? |
|---|---|---|---|
| Auth token / profile id | OS keychain (`keyring` crate, macOS Keychain on darwin) | yes | client cache |
| Last applied cursor per session | local SQLite (`rusqlite`) | yes | client cache only |
| Session message history | server (REST hydrate on open) + 30-day local cache | yes (cache) | server |
| Streaming `ephemeral.*` | RAM only | no | non-authoritative |

This mirrors `aichat`'s `aichat_history.json`, but per-session and with a hard rule: on first
open of a session, REST history is the source of truth; the local cache is a startup-warmer, not
a fallback.

## 7. Failure model

| Event | Reaction |
|---|---|
| WebSocket drops | transport: exponential backoff (1s, 2s, 4s, 8s, 30s cap); store: mark `connection: Reconnecting`; UI: greys out composer, shows toast |
| Reconnect succeeds | transport sends `session/open { after: cursor }`; store applies replay; UI re-enables; toast clears |
| Server returns `cursor invalid` | store drops local cursor, fetches REST snapshot, then reconnects fresh |
| Turn errors mid-stream (`turn/error`) | store transitions turn to `Errored { code, message }`; UI shows error bubble; composer re-enables |
| Approval timeout (no user response) | server retries / cancels per its own policy; we just render whatever it tells us |
| REST 401 / 403 | clear keychain, route to login |
| REST 5xx on snapshot hydrate | retry with backoff up to 3x, then surface a "couldn't load history" banner; chat still works for new turns |

## 8. Build / packaging at a glance

(Detail in `workstreams/W09-build-packaging.md`.)

- `cargo build --release` produces a single binary; same toolchain `aichat` uses (Rust stable 1.95).
- `cargo-packager` builds `.app` (macOS), `.msi` (Windows), `.AppImage` / `.deb` (Linux).
- Fonts ship inside the binary via Makepad's `crate_resource("self:resources/...")`. Adds ~36 MB
  (`LXGWWenKaiMono` is 25 MB, `NotoColorEmoji` is 10 MB). Acceptable for desktop.
- Mobile / web targets are out of scope (charter).

## 9. Open architectural questions

1. **One window or many?** `aichat` is single-window. Octos has chat + studio + slides + sites +
   coding. Single-window with a sidebar is simpler and matches `octos-web`. Multi-window is more
   native-feeling. **Decision: single-window for M1; revisit at M3.**
2. **Splash inline UI from LLM responses?** `aichat` runs Splash blocks the model emits. Octos
   has its own sandbox crate (`octos-sandbox`). We need the Splash interpreter to honour Octos
   sandboxing or refuse to run untrusted blocks. **Decision: keep ```runsplash off by default in
   M1; turn on once sandboxing is reviewed.**
3. **Local LLM dev mode.** The `StatelessBackendAdapter` from `aichat` is useful for development
   without a running Octos server. **Decision: keep it behind a `--dev-llm-direct` flag.**
4. **Tenant / profile picker.** Octos puts profile in a header (`X-Profile-Id`); some
   self-hosters use subdomains. **Decision: a profile picker in the sidebar, header-based on the
   wire. Subdomain-only deployments configure via env var.**

These resolve before the workstream they touch starts. They are tracked in
`06-WORKSTREAMS.md` "open decisions" section.
