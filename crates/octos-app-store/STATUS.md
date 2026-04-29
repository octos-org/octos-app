# octos-app-store — status

The W08 auth slice has landed. `crates/octos-app-store/src/auth.rs` defines the
redacting `SecretToken` newtype (its `Debug` and `Display` are both `<redacted>` and the only way out is `expose(&self) -> &str`), the `ProfileId` and `ServerHost` newtypes, the `AuthSlice` struct, the `AuthEvent` enum covering `LoginRequested` / `CodeSent` / `CodeVerified` / `AuthError` / `Logout`, and a pure deterministic `reduce` function. Module-level docs cite the relevant server endpoints by line number against `~/home/octos/crates/octos-cli/src/api/auth_handlers.rs` (`send_code` :389, `auth_status` :508, `verify` :543, `logout` :680). The `keychain` module (gated behind a new `keychain` Cargo feature, opt-in) wraps the workspace `keyring` crate with `store_token` / `load_token` / `delete_token` keyed under the service name `octos-app::<host>::<profile_id>`, and short-circuits via `OCTOS_APP_TOKEN` for headless / dev runs. `cargo check -p octos-app-store` and `cargo check -p octos-app-store --features keychain` both compile clean; `cargo test -p octos-app-store --features keychain` runs 10 unit tests covering the reducer state transitions, redaction guarantees on both `SecretToken` and the containing `AuthSlice`, the `service_name` format, the env-var bypass path, and that `KeychainError` `Display` does not embed any token-shaped substrings. Total new-code LOC: 347 (under the 350 budget). What's next: the LoginScreen UI in `app/src/app/login.rs` (different agent, later W08 task) — it dispatches `AuthEvent::LoginRequested` / `CodeSent` / `CodeVerified` into this slice, drives the four-state machine described in `workstreams/W08-auth-tenancy.md`, and wires the keychain helpers behind `tokio::task::spawn_blocking`; transport-side, the `auth_headers()` selector and 401/403→`Logout` middleware still need adding (probably alongside W01's REST client work).

---

## 2026-04-28 — W04 store core + reducer landed

The W04 reducer skeleton is in place. Eight new modules were filled in under `crates/octos-app-store/src/`:

- `sessions.rs` — `Session`, `SessionMap` (HashMap + ordered `Vec<SessionKey>`), with `insert` / `remove` / `touch` (move-to-front-on-update), and selectors `sessions_for_sidebar` / `is_session_active`. Keyed by `octos_core::SessionKey` (re-exported from `~/home/octos/crates/octos-core/src/types.rs:160`).
- `turns.rs` — `Turn` struct + `TurnStatus { Pending, Streaming, Completed, Errored, Interrupted }`. The streaming text buffer lives in `state::Ephemeral`, NOT here, per `03-PROTOCOL-CONTRACT.md` § Live streaming output. `turn/error` with `code: "interrupted"` produces the `Interrupted` variant; everything else produces `Errored`.
- `tasks.rs` — `Task`, `ToolCall`, and a local `ToolCallId(String)` newtype (the protocol wire uses a free-form string; the newtype keeps the `HashMap` key honest). `Task` carries `last_cursor: Option<OutputCursor>` from `task/output/delta`. Lifecycle/runtime states are stored as `String` so unknown server values pass through (forward-compat per the contract's capability rules).
- `files.rs` — `FileMeta`, `FileKind` viewer hint enum, and a local `FileHandle(String)` newtype (declared here rather than re-exported from `octos-app-transport` to avoid a `store → transport → store` dep cycle, exactly as the brief specified). `FileKind::from_mime` falls back to filename for `.md` since some servers return `text/plain` for markdown.
- `navigation.rs` — `CurrentScreen { Login, Home, Chat { session }, Coding, Studio { project }, Slides { project }, Sites { project } }`, `Producer { Studio, Slides, Sites }`, `ProjectId(String)`, and `NavigationEvent { NavigateTo, OpenSession, OpenProject, Logout }` with a small `reduce(screen, current_session, event)` that keeps `current_session` and `CurrentScreen::Chat.session` in lockstep.
- `toasts.rs` — `ToastQueue { items: VecDeque<Toast>, capacity }` defaulting to capacity 3; `push` evicts the front when full. `ToastKind { Error, Reconnecting, ReconnectSuccess, Info }`.
- `approvals.rs` — `ApprovalsSlice { by_id, state, pending_order }` with `requested` / `pending_response` / `decided` / `failed`. `requested` is idempotent (cursor-replay-safe). Detailed payload types come straight from `octos-core`'s `ApprovalRequestedEvent` / `ApprovalDecision` / `ApprovalTypedDetails` — we don't re-encode them.
- `state.rs` — top-level `AppState` (auth, navigation, sessions, current_session, cursor map, turns, tasks, tool_calls, files, approvals, ephemeral, toasts, connection), `Ephemeral { streaming_text, thinking_text }`, `ConnectionState { Connected, Reconnecting, Offline }` (defined locally — not re-exported from `octos-app-transport` — to break the dep cycle, also as specified), the top-level `Event` enum (`Protocol { cursor, notification }` wrapping `octos_core::ui_protocol::UiNotification`, plus `Auth` / `Navigation` / `Snapshot` / `Connection` / `Toast` / `DismissOldestToast`), and the `pub fn reduce(state: &mut AppState, event: Event)` dispatcher. `lib.rs` re-exports the top-level surface (`AppState`, `Event`, `reduce`, `ConnectionState`, `Ephemeral`, `SnapshotEvent`, `ConnectionEvent`, `UiCursorMap`).

Source-of-truth comments cite octos-core line numbers for each imported type (e.g. `// see octos-core ui_protocol.rs:62 (UiCursor), :69 (TurnId), :1577 (UiNotification)`).

### Reducer behaviour (worth knowing for downstream agents)

- `Event::Protocol { cursor: Some(c), notification }` advances `state.cursor[session_id] = c`. The caller (transport) is expected to pass `cursor: None` for `MessageDelta` (ephemeral, never advances).
- `MessageDelta` appends to `ephemeral.streaming_text[turn_id]` and flips the matching turn from `Pending` to `Streaming`.
- `TurnCompleted` / `TurnError` drop both `streaming_text` and `thinking_text` for that turn — durable history is expected to rehydrate via REST. `TurnCompleted.cursor` (the event payload's optional cursor) is also written into `state.cursor`, so the resume point is canonical.
- `ToolStarted` / `ToolCompleted` and `TaskUpdated` together drive the per-session `has_active_task` flag via a small `recompute_active` helper that scans for non-terminal tool/task rows. `ToolProgress` upserts a stub if `tool/started` was missed (defensive — protocol shouldn't reorder, but cheap to handle).
- `NavigationEvent::Logout` clears `current_session` and `screen`, and the parent `state::reduce` then also wipes `ephemeral` and `toasts` (treats Login as a fresh slate).
- `SnapshotEvent::SessionRemoved` clears the session pointer, drops the cursor entry, and routes back to `CurrentScreen::Home` if the removed session was the current one.

### Tests

`cargo test -p octos-app-store` runs **38 unit tests, all passing** (41 with `--all-features`, including 3 keychain tests). The required tests from the brief are all in:

- `sessions::tests::session_map_move_to_front_on_touch`
- `turns::tests::turn_lifecycle_streaming_to_completed`
- `tasks::tests::tool_call_correlation_by_id`
- `toasts::tests::toast_queue_evicts_oldest_at_capacity`
- `navigation::tests::navigation_logout_clears_session_pointer`
- `state::tests::reduce_durable_notification_updates_cursor`

Plus extra coverage for: `MessageDelta` not advancing the cursor; `TurnCompleted` dropping ephemeral buffers; tool lifecycle flipping `has_active_task` on/off; `TaskUpdated` Running→Completed clearing the dot; `ApprovalRequested → pending_response → decided` lifecycle including idempotent re-requests; `Logout` wiping ephemeral + toasts; `SessionRemoved` clearing the session pointer + cursor + routing Home; `ConnectionEvent` round-trip; and the per-module unit tests (sidebar order, MIME detection, etc.).

### LOC

Total production LOC across the eight new modules: **783** (under the 800 budget). `state.rs` is the largest at 262 lines; the rest are 51–92 lines each. No `todo!()` in production code.

### Compile target

- `cargo check -p octos-app-store` — clean (no warnings).
- `cargo check -p octos-app-store --all-features` — clean.
- `cargo test -p octos-app-store` — 38 passing.
- `cargo test -p octos-app-store --all-features` — 41 passing (10 pre-existing auth + 3 keychain + 25 new modules).
- Workspace `cargo check` — clean for this crate; the only warning surfaced is a pre-existing `makepad_widgets` glob ambiguity in `app/src/main.rs` and is unrelated.

### What's next

1. **LoginScreen UI** in `app/src/` (a different agent, per the brief): consume `AuthSlice`, dispatch `AuthEvent::*` and `NavigationEvent::NavigateTo(CurrentScreen::Home)` after a successful verify. Block on this slice via the `pub use` re-export from `lib.rs`.
2. **Transport bridge** (W01): translate WebSocket-decoded `RpcNotification<Value>` → `UiNotification` → `Event::Protocol { cursor, notification }`, and REST hydrate fans → `Event::Snapshot(SessionsHydrated)` / `FileMetaHydrated`. Reconnect heartbeat → `Event::Connection(Reconnecting | Connected | Offline)`.
3. **Cursor persistence** (binary crate): the `state.cursor: HashMap<SessionKey, UiCursor>` slice is the in-memory side; a small SQLite `cursor_cache` table needs writing (see `01-ARCHITECTURE.md` § 6).
4. **Approvals UI** (W05): the `ApprovalsSlice` is plumbed but the typed-details rendering (command / diff / filesystem / network / sandbox-escalation) is still UI-side.
5. **Diff preview + task output read** RPC results (W04 M2): not in this drop — they go through `octos-app-transport` once the RPC client lands; the store will gain a `Task::tail` ring buffer and a `DiffPreview` cache map.
6. **Multipart upload staging**: `pending_uploads: Vec<FileHandle>` from W04 § 9 is not yet in `AppState`; add when the composer attach path lands (W03/W04 boundary).

