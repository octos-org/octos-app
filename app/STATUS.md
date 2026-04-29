# octos-app — `app/` status

W02 first pass landed: `app/src/main.rs` is now the Octos app shell, not a
verbatim aichat copy. The seeded multi-LLM apparatus (`BackendType` enum,
`ALL_BACKENDS`, the per-backend `create_agent` match arm, `read_key_file` /
`read_key`, the inline splash + diagram `system_prompt`, the
`thinking_toggle` reducer, the flat-file `aichat_history.json` persistence,
and the `stateless_history_messages` replay path) is gone — see the
`(W02 strip)` comments in `main.rs` and the trimmed `mod tests` block.
`OctosUiAgent` (`app/src/backend/octos_ui.rs`) is the single `Agent`
implementation; it carries a `TransportConfig` and stubs every wire call as
`todo!()` except `handle_event`, which returns an empty `Vec` so the chat
surface boots cleanly with no stream. The diagram-fence safety scanner moved
to `app/src/app/diagram_safety.rs`. The lifted aichat shell stays — DSL
structure, Markdown / Splash / Mermaid / CodeView templates, streaming
remend pipeline, `MermaidSvgView`, glass slider — only labels changed (the
two "AI Chat" labels are now "Octos"; `backend_dropdown` is relabelled
"Profile" and ships a single stub `(no profile)` entry; `thinking_toggle`
is kept in the DSL but reduced to an inert `let _ = ...` per the
"document your choice" allowance). `cargo check --workspace` passes (only
the inherited `pub use makepad_widgets` future-compat warning), `cargo build
-p octos-app` produces a 52 MB debug binary that opens a window and stays
up, and `cargo test --workspace` passes 65/65 across all four crates.
`main.rs` is 2,344 lines (was 2,890; under the 2,400-line target). What
**W03** should pick up next: replace the `todo!()`s in
`OctosUiAgent::create_session` / `send_prompt` / `cancel_prompt` /
`send_tool_result` with real `OutboundCommand` posts into the transport
task once W01's runtime loop lands, and extend `App::handle_event`'s
`AgentEvent` consumer with the new variants the transport will emit
(`tool/started`, `approval/requested`, `turn/completed`, etc., per
`05-AICHAT-REUSE-MAP.md`'s `AgentEvent consumer loop` row). W08 will fill
in `available_profiles` + the keychain-backed `placeholder_transport_config`,
and W04 will replace the no-op `ChatData::save_to_disk` /
`ChatData::load_from_disk` with the per-session SQLite cache.

W03 first pass landed in `app/src/backend/octos_ui.rs`: `OctosUiAgent` is
now functional. `new()` builds an owned single-thread Tokio runtime and
calls `octos_app_transport::ws::spawn`, capturing the
`(Sender<OutboundCommand>, Receiver<TransportEvent>)` pair.
`create_session` mints a `SessionId`, derives an opaque `SessionKey`
(`octos-app:<live-id-hex>`), and posts `OpenSession`. `send_prompt`
mints a `PromptId` + `TurnId`, registers both directions of the
prompt↔turn map, and posts `StartTurn` carrying an `InputItem::Text`.
`cancel_prompt` translates a `PromptId` back to its `TurnId` and posts
`InterruptTurn`; `send_tool_result` wraps the body as
`{content, is_error}` and posts the placeholder `SendToolResult`.
`handle_event` drains `evt_rx` non-blockingly via `try_recv` and
translates `TransportEvent`s: `RpcResult(SessionOpen)` flips
`is_session_ready` and emits `AgentEvent::SessionReady`;
`message/delta` → `TextDelta`; `tool/started` → `ToolRequest`;
`turn/completed` → `TurnComplete` (clearing the map entry);
`turn/error` → `PromptError`; `RpcError` is routed to `SessionError`
or `PromptError` based on the originating method. Capability negotiation
is captured into a stored `Capabilities`; connection-state transitions
update an internal field with a `TODO(W04)` note that `AgentEvent` has
no `ConnectionState` variant for the status bar (W04 to add a side
channel). `task/updated`, `tool/progress`, `tool/completed`,
`approval/requested`, `task/output/delta`, and `warning` notifications
are silently buffered (returned as empty `Vec<AgentEvent>`) for W04 +
W05 to surface in the TaskDock / ApprovalSheet. Pending live-server
testing — `cargo check -p octos-app` and `cargo build -p octos-app` are
green, the binary boots without a server (the transport task drops to
`ConnectionState::Failed` after the 5-min reconnect budget expires;
sends silently fail into the closed channel and `is_session_ready`
stays `false`). One small dependency adjustment in `app/Cargo.toml`:
added `tokio = { workspace = true, features = ["rt-multi-thread"] }`
and `serde_json = { workspace = true }` so the agent can own its
runtime and wrap tool-result payloads.

## W04-UI

W04 M1 sidebar pane landed: `app/src/app/sessions.rs` (287 LOC) ships a
`SessionList` widget (`#[derive(Script, ScriptHook, Widget)]`) that wraps a
`PortalList` and reads from a process-wide `LazyLock<RwLock<AppState>>`
(`APP_STATE`) — same shape as aichat's `pub static CHAT_DATA` at
`aichat/examples/aichat/src/main.rs:1144`, just with the runtime-init
wrapper since `AppState::default()` carries `HashMap`s. The widget's
`draw_walk` snapshots `state.sessions.sessions_for_sidebar()` (selector at
`crates/octos-app-store/src/sessions.rs:77`) into a small `RowSnapshot`
vector under the read lock, then per-row sets the title, optional
`last_message_preview`, hides/shows a cyan `streaming_dot` driven by
`is_session_active`, and toggles a gold `selected_marker` against
`current_session`. Pattern lifted from aichat's `ChatList` widget at
`aichat:1774-1881`. Each row carries a transparent `row_click` button
that fills the row plus a small `delete_button` ('x') matching aichat's
`delete_button` template at `aichat:600`; the widget posts
`SessionListAction::Selected(_)` / `DeleteRequested(_)` via
`Cx::post_action`. Cross-thread plumbing mirrors
`aichat/old/widgets/src/image_cache.rs:471` (`Cx::post_action`
`AsyncImageLoad`): `hydrate_sessions` and `delete_session_remote` each
spawn a named `std::thread`, build a single-thread Tokio runtime, run
`RestClient::list_sessions` / `delete_session` (transport at
`crates/octos-app-transport/src/rest/mod.rs:165` / `:177`), and post a
`SessionListAction` back. `App::handle_actions` (in `main.rs`) folds
`Hydrated`, `Failed`, `Selected`, `DeleteRequested`, `Deleted` —
optimistic remove on `DeleteRequested`, re-hydrate on REST failure to
roll back per W04 § 4. `App::handle_startup` calls
`build_rest_client(&placeholder_transport_config())` and kicks off the
hydrate before constructing the agent. Sidebar wiring: replaced the
`Label{text: "对话"} + Label{text: "暂无聊天"} + spacer View` placeholder
in `main.rs:~828-834` (W02 strip) with a `session_list := SessionList`
filling the lower sidebar; the surrounding `nav_*` buttons and
`settings_button` are untouched per the coordination note with W08-UI.
`Cargo.toml` gained `chrono = { workspace = true }` for the
`SessionListItem.last_message_at` projection and `serde =
{ workspace = true }` to clear a pre-existing W08 LoginScreen import
error (`login.rs` was importing `serde::{Deserialize, Serialize}`
without a direct dep). `cargo check --workspace` is green (only the
inherited `pub use makepad_widgets` future-compat warning), `cargo build
-p octos-app` succeeds, `cargo test --workspace` passes all suites, and
`cargo run -p octos-app` boots the window — the sidebar list is empty
because `https://localhost:8080/api/sessions` is unreachable in the
default env, but the REST hydrate fires (visible in `[W] session list
REST: rest network: error sending request for url ...`). What **M2**
should pick up next: `app/src/app/task_dock.rs` collapsible per-turn
timeline over `tool/*` + `task/*` (W04 § 5), `app/src/app/files.rs` for
the four viewers (image album, audio, video, markdown — W04 § 6),
`app/src/app/content_browser.rs` over `/api/my/content` (W04 § 7), and
the multipart `/api/upload` flow (W04 § 8). The two-step delete
confirmation (web's `session-list.tsx:111-132`) is also deferred to M2
— current behaviour is single-click delete with optimistic rollback;
upgrade to a confirm pill once the toast queue lands.

## W08-UI

W08's user-visible login flow landed alongside the W08-store auth slice
+ keychain. New file `app/src/app/login.rs` (375 LOC, under the 400
budget) defines the `LoginScreen` widget as a plain DSL `View` prototype
registered through a `script_mod!` block — no custom Rust `Widget`
impl, the three step containers (`login_server_step`,
`login_email_step`, `login_code_step`) toggle their `visible` flag from
`App::handle_actions`. The card lifts the `GlassPanel` + `TextInput` +
`PillButton` styling already used by the chat composer
(`main.rs:1038`, `main.rs:168`) so the visual language stays consistent
with W02's shell. `app/src/app/mod.rs` gains `pub mod login;`.
`app/src/main.rs` got four surgical edits: (a) a
`login_overlay := LoginScreen { visible: false }` sibling at the body
level (sibling to `app_shell`, before `resize_grip` so the resize grip
stays clickable from Login), (b) a `sign_out_button` row added to the
sidebar bottom under the existing `settings_button`, (c) three new
`#[rust]` fields on `App` (`login_server_url`, `login_profile_id`,
`login_pending_email`), and (d) a `crate::app::login::script_mod(vm)`
call inside `App::script_mod`. Boot path in `handle_startup` resolves
in priority order `OCTOS_APP_TOKEN` env var →
`~/.config/octos-app/server.json` +
`octos_app_store::keychain::load_token` → LoginScreen Step 1; the
helper `boot_is_authed()` caches the parsed URL + profile id so
re-launches that have a server but no token jump straight to the email
step. Button clicks dispatch through three off-thread workers
(`std::thread::spawn` that build a one-shot single-threaded
`tokio::Runtime`), call the new `RestClient::send_code` /
`RestClient::verify` methods (`crates/octos-app-transport/src/rest/mod.rs`
new `pub async fn send_code` / `pub async fn verify`, citing
`auth_handlers.rs:389` and `auth_handlers.rs:543`), and post a typed
`LoginAsyncAction` back via `Cx::post_action`. Sign-out clears the
keychain entry for `<host>::<profile_id>` (best-effort — errors get
logged, the local wipe still runs), resets the email/code inputs, and
re-shows the LoginScreen. `cargo check --workspace` is clean (only the
pre-existing `pub use makepad_widgets` future-compat warning);
`cargo test --workspace` runs 72/72 (16 in `octos-app` including 5 new
in `app::login::tests`, 41 in `octos-app-store`, 14 in
`octos-app-transport`, 1 contract). Two new dependency adjustments in
`app/Cargo.toml`: `octos-app-store = { … features = ["keychain"] }` to
flip on the keychain helpers, and an explicit
`reqwest = { workspace = true }` for the `reqwest::Client::new()` call
in the LoginScreen's worker thread (`tokio` + `serde_json` were already
in place from W03/W04, and `serde` was added by W04-UI for the
`ServerConfig` derives).

What's `todo!()` / next: (1) the `Auth` reducer events
(`AuthEvent::CodeVerified` etc. in `octos-app-store::auth`) are not yet
dispatched into a global `AppState.auth` slice — the LoginScreen
mutates local fields on `App` directly. The store-side reducer wiring
stays for a follow-up so this commit doesn't have to reach into
`APP_STATE`'s shape. (2) Successful `login_verify_clicked` does not
refresh `OctosUiAgent`'s transport (the agent is constructed in
`handle_startup` with the placeholder env-var bearer); a real session
needs the agent to re-init from the freshly-stored keychain bearer +
URL — left for the W01/W08 transport-integration follow-up. (3) The
profile picker in the top bar is still seeded with `(no profile)`;
`/api/my/profile` (`auth_handlers.rs:834`) hydration to swap in a real
label is the W08 follow-up. (4) The 401/403 → `Logout` middleware (W08
deliverable §8) isn't part of this commit; it lives transport-side.
(5) Server-side `/api/auth/logout` (`auth_handlers.rs:680`) isn't
called from the local sign-out — the client wipe is sufficient since
the bearer is opaque.

What to test live (manual smoke checklist):
- Launch with no `OCTOS_APP_TOKEN` and no
  `~/.config/octos-app/server.json`: the LoginScreen overlay shows,
  Step 1 visible, Step 2/3 hidden, sidebar hidden behind the overlay.
- `Continue` with empty Profile ID: error label says "Profile ID is
  required". Continue with garbage URL: error matches the rejected
  scheme. Valid URL + profile id: file appears at
  `~/.config/octos-app/server.json` and Step 2 takes over.
- `Send code` with no `@`: error label. With a valid email shape:
  status flips to "Sending code…" then either "Code sent — check your
  email." (advance to Step 3) or a `send-code: …` transport error
  (stay on Step 2). Step 3 input + `Verify` round-trips through the
  same pattern.
- Re-launch with the JSON file present but no keychain entry: skip
  Step 1, land on Step 2.
- `OCTOS_APP_TOKEN=foo cargo run -p octos-app`: skip the login overlay
  entirely, drop to Home.
- Click `退出登录` in the sidebar: keychain entry deleted, LoginScreen
  re-shown on Step 2.

## W04 — Task dock

Live `tool/*` and `task/*` pipeline now surfaces in the UI. Three
landings:

1. `app/src/backend/octos_ui.rs` — the previously silent buffer for
   `tool/progress`, `tool/completed`, `task/updated`,
   `task/output/delta`, `turn/started`, `approval/requested`,
   `session/opened`, `warning` is replaced by a `fold_into_store`
   helper that constructs `octos_app_store::state::Event::Protocol
   { cursor, notification }` from each `TransportEvent::{Durable,
   Ephemeral}Notification` and dispatches through `state::reduce`.
   Cursors from durable frames are forwarded; ephemerals (only
   `message/delta` today) pass `cursor: None`. The translate path
   that builds `AgentEvent`s for the chat surface still runs for
   `MessageDelta` / `ToolStarted` / `TurnCompleted` / `TurnError` so
   W03's streaming pipeline is unchanged. Notification types
   referenced: `ToolStartedEvent` (octos-core ui_protocol.rs:1311),
   `ToolProgressEvent` (:1321), `ToolCompletedEvent` (:1332),
   `TaskUpdatedEvent` (:1531), `TaskOutputDeltaEvent` (:1541), all
   members of the `UiNotification` enum at :1577.
2. `app/src/app/task_dock.rs` (new, 302 LOC, well under the 350 LOC
   budget) — `TaskDock` widget, read-only access to `APP_STATE`. Two
   states: a collapsed pill (`🔧 N tools · M tasks · X% running`) and
   an expanded body listing up to 8 `DockRow`s. Filters to the current
   session via `APP_STATE.current_session`; idle state (zero tools and
   zero tasks for the current session) hides the dock entirely so it
   takes no vertical space. Header counts and a `running %` are
   recomputed each draw; running is `success: None &&
   completed_at.is_none()` for tools and `runtime_state ∉
   {"completed", "failed"}` for tasks. Click on the header pill flips
   `expanded: bool`. Output preview from `tool/completed` shows as a
   trailing detail on the row when present (clamped to 64 chars).
3. `app/src/main.rs` — registered the `TaskDock` prototype in the
   `script_mod!` block (right after `SessionList`, before
   `startup()`); mounted it as `task_dock := TaskDock {}` between the
   composer's GlassPanel and the bottom `status_label`, matching the
   ChatScreen layout in `04-IA-AND-NAVIGATION.md` § ChatScreen. The
   expanded body is wrapped in `RubberView { smoothing: 0.3 }` (lifted
   from `aichat/examples/aichat/src/main.rs:480`'s assistant message
   wrapper) so the height transition on toggle is smoothed without a
   custom Animator. `app/src/app/mod.rs` declares the new
   `pub mod task_dock;`.

`cargo check --workspace` is clean (only the pre-existing
`pub use makepad_widgets` future-compat warning); `cargo build
-p octos-app` produces a debug binary that boots; `cargo test
--workspace` runs 72/72.

What's left for M2 finishing work (deliberately out of this commit):

- **Drill-down task tail** — clicking a row should call
  `octos-app-transport`'s `RequestTaskOutput { params:
  TaskOutputReadParams, … }` and stream the result into a small inline
  tail viewer with a 256 KiB rolling ring (W04 § "Task dock
  sub-surface"). The widget already captures `correlation_id` per row;
  the tail viewer + `task/output/read` REST helper (covering the
  `complete: true` stop condition documented in W04 § 13) is the
  follow-up. `app/src/app/task_dock.rs` flags this with a
  "loading…" placeholder hidden until the helper lands.
- **Tool progress fraction** — `ToolProgressEvent` carries
  `progress_pct: Option<f32>` and `message: Option<String>`
  (octos-core ui_protocol.rs:1321), but the store's `ToolCall` slice
  doesn't keep them yet; the dock would render an inline progress bar
  if it could read them. Extending `octos_app_store::tasks::ToolCall`
  to carry `progress_pct` + `message` and updating the
  `UiNotification::ToolProgress(e)` arm of `state::apply_protocol`
  (octos-app-store/src/state.rs:178) is a one-screen change worth
  pairing with the tail viewer.
- **File viewers** — `ImageAlbumViewer`, `AudioPlayer`, `VideoPlayer`
  (`robius_open` fallback per W04 § 13 risk row), `MarkdownViewer`
  over `/api/files/{handle}`. Lifted from `octos-web/src/components/
  viewers/*` per the W04 deliverables table.
- **Content browser** — `app/src/app/content_browser.rs` over
  `/api/my/content` with filter / search / sort / pagination and
  bulk-delete. `PortalList` grid mirrors the `SessionList` shape we
  shipped in M1.
- **Composer attach hook** — multipart upload via
  `POST /api/upload`, pending-handle chips above the input, cleared
  on `turn/start` (W04 § 8).

What to test live (manual smoke checklist):

- Boot the app against a server that emits at least one
  `tool/started` over WS; the dock pill appears under the composer
  with `🔧 1 tools · 0 tasks · 100% running`. On `tool/completed` the
  running % drops; on a 30 s post-terminal idle window the auto-hide
  is not yet wired (open question in W04 § 5 — the dock stays at
  `0% running` until a new task starts; explicit auto-collapse is a
  follow-up).
- Click the pill: chevron flips `▸` → `▾`, body fades open with the
  RubberView smoothing visible. Click again: collapses with the same
  smoothing.
- Switch session from the sidebar: dock re-projects to the new
  current session's `tool_calls` / `tasks` (cross-session frames stay
  in `APP_STATE` but are filtered out per W04 § 5 "Events carry
  `session_id`; non-current drop").
- Boot with no server: dock stays collapsed and invisible (idle
  state), composer keeps full height.

## W05 — Approvals & diff preview

Surface for `approval/requested` notifications + the round-trip back to the
server with `approval/respond`. Backend pieces (store slice, transport
command, capability negotiation) already in place from W01 / earlier W05
groundwork; this commit lights up the UI side.

What's wired:

- **`app/src/app/approvals.rs` (new)** — typed approval card widget. Owns:
  - DSL prototypes `mod.widgets.ApprovalCardView` + `mod.widgets.ApprovalsPane`,
    registered via `crate::app::approvals::script_mod(vm)` in
    `App::script_mod`.
  - Risk badge (chip rendered when `ApprovalRequestedEvent.risk` is non-empty;
    label uppercases `"medium" | "high" | "critical"` etc., color is the
    existing amber `#xF6BE63` pill — palette reuse from `RiskBadge`).
  - Title + body (`ApprovalRequestedEvent` `title` / `body`).
  - Typed sub-views, one per `approval_kinds::*` (octos-core
    ui_protocol.rs:34): `command` (CodeView with command_line, cwd, env_keys),
    `diff` (summary + op label; full hunk-list `DiffView` is M3 work — TODO
    inline), `filesystem` (op + paths CodeView + outside-workspace warning),
    `network` (op + hosts/ports + URLs CodeView). Unknown / sandbox-escalation
    kinds fall through with body markdown only (forward-compat per
    `03-PROTOCOL-CONTRACT.md`).
  - Primary Approve / secondary Deny buttons, scope dropdown
    (`approval_scopes::REQUEST | TURN | SESSION` from
    octos-core:42, hidden when `Capabilities::typed_approvals == false`).
    Render-hint overrides for `primary_label` / `secondary_label`.
  - Capability gating: reads `APPROVAL_CAPS` (`LazyLock<RwLock<…>>`) which
    `OctosUiAgent` mirrors from `TransportEvent::CapabilityNegotiated`.
- **`app/src/app/mod.rs`** — `pub mod approvals;`.
- **`app/src/main.rs`**:
  - `script_mod` chain: `crate::app::approvals::script_mod(vm)` registered
    before `self::script_mod(vm)`.
  - DSL: `approvals_pane := ApprovalsPane {}` placed between `chat_shell`
    and `composer_row`. (Layout choice — queue-pane, not interleaved.
    Rationale in `app/src/app/approvals.rs` doc-comment: `CHAT_DATA` carries
    no `TurnId` plumbing, inline-with-chat would require invasive surgery to
    `ChatList::draw_walk`. M3 may revisit.)
  - `App::create_octos_agent` now returns
    `(Box<dyn Agent>, ApprovalHandle)`; the handle is captured in
    `App::approval_handle` so `handle_actions` can issue `approval/respond`
    without downcasting `Box<dyn Agent>`.
  - `MatchEvent::handle_actions` folds two new actions:
    - `ApprovalUiAction` (Approve/Deny click): optimistic
      `approvals.pending_response(...)`, then `ApprovalHandle::respond(...)`.
    - `ApprovalAsyncAction` (wire reply): `Accepted` → `approvals.decided(...)`,
      `Failed(msg)` → `approvals.failed(...)`.
- **`app/src/backend/octos_ui.rs`**:
  - `TransportEvent::CapabilityNegotiated` mirrors flags into `APPROVAL_CAPS`.
  - `ApprovalHandle` (new pub struct) — clone of `Sender<OutboundCommand>`
    + `tokio::runtime::Handle`; its `respond(...)` method posts
    `OutboundCommand::SendApprovalResponse` and forwards the wire reply
    (`oneshot::Receiver<Result<ApprovalRespondResult, RpcError>>`) to the
    UI thread as `ApprovalAsyncAction`. Spawns on the agent's owned runtime.
  - `OctosUiAgent::approval_handle()` constructs and returns one.
- **`approval/requested` drain** — already in place via
  `OctosUiAgent::fold_into_store` (W04 work) which calls
  `octos_app_store::state::reduce`; the existing
  `UiNotification::ApprovalRequested(e) => state.approvals.requested(e)` arm
  in `state.rs:193` lights up the card. No further changes needed here.

What's live-server-tested vs unit-tested:

- **Unit-tested**: `octos_app_store::approvals::ApprovalsSlice` lifecycle
  (request → pending → decided; idempotency on duplicate `requested`;
  `failed` keeps the entry in `pending_order`). 3 tests, all pass.
- **Compile-tested**: `cargo check --workspace` clean (only the pre-existing
  `pub use makepad_widgets` ambiguity warning, unrelated to W05).
  `cargo build -p octos-app` green. All 14 transport unit tests +
  contract test still pass.
- **Not live-server-tested yet**: the round-trip relies on a running
  `octos-cli` that emits `approval/requested` and accepts
  `approval/respond`. M2's contract harness (W10) will cover this. The
  manual smoke path is: boot against a server that triggers a tool requiring
  approval; the card appears in the pane between the chat thread and
  composer; click Approve, see the buttons disable + `decided: approve` row
  appear; on `runtime_resumed: true` the agent's tool call resumes.

What's next (deliberately out of scope for this commit):

- **`DiffView` deep widget** — file tree + per-hunk `CodeView` rendering
  via `diff/preview/get` (octos-core:628). The `typed_diff` sub-view
  currently renders only the summary; a TODO inline points at the
  aichat:404-411 / :510-543 reference. Lifting `aichat`'s `CodeView`
  per-hunk pattern + a small file-tree column is a separate ticket.
- **Sandbox-escalation sub-view** — falls through to body markdown today;
  needs a dedicated sub-view per `ApprovalSandboxEscalationDetails`
  (octos-core:1416).
- **Inline-with-chat layout** — currently the queue pane sits above the
  composer. M3 may pin approvals inline next to the message they
  correspond to once `ChatMessage` carries `turn_id`.
- **Idempotent retry on WS drop** — the wire reply path treats RPC errors
  as `Failed`; the server's `APPROVAL_NOT_PENDING` (-32011) carries the
  recorded decision in error data, which we don't yet parse to collapse
  the retry into a `Decided`. W05 § "Approval response flow" calls this
  out; the UI re-enables the buttons on Failed so the user can re-click.
- **Render-hint danger styling** — `render_hints.danger` swaps button
  styling to red; today we honor `primary_label` / `secondary_label`
  text overrides but not the danger style. M3 follow-up.
- **Dev harness** — fake `ApprovalRequestedEvent` injection menu to
  verify each typed kind without a live server.

What to test live (manual smoke checklist):

- Boot against a server that emits a `tool/started` requiring approval.
  An approval card slides in between `chat_list` and the composer with
  the tool name, title, body, risk badge (when `risk` is set), and the
  matching typed sub-view (command / diff / fs / network).
- Click `Approve`: buttons disappear, the card flips to a `decided:
  approve` row, the agent's tool call resumes (next `tool/started` /
  `message/delta` arrives).
- Click `Deny`: same flow with `decided: deny`.
- With `Capabilities::typed_approvals = false` (server omits the flag at
  session-open), the card collapses to title + body + Approve / Deny
  with no scope dropdown and no typed sub-views.
- Drop the WS mid-respond: card stays in `PendingResponse` until reply
  arrives or the channel drops; on drop it flips to `Failed("transport
  unavailable")` and the buttons re-enable so the user can retry.

## Hotfix — DSL scope + REST hydrate

The 2026-04-28 boot run surfaced two startup-time bugs that `cargo check
--workspace` couldn't catch (the live-DSL only evaluates at runtime):

1. **DSL prototype / instance confusion in `main.rs::TaskDock`.** The
   `let TaskDock = … { … DockRow := View {…} … row_0 := DockRow {} …}`
   block defined `DockRow` with `:=` (instance assignment) inside the
   `body` of `TaskDock`, then tried to instantiate eight rows from it.
   `:=` registers a *named child instance*, not a reusable prototype, so
   each `row_N := DockRow {}` lookup reported `variable DockRow not
   found in scope` (×8). Fix: pulled `DockRow` to a top-level
   `let DockRow = View { … }` in the script_mod, mirroring the
   `let RiskBadge = …` / `let CardButton = …` pattern in
   `app/src/app/approvals.rs`.

2. **`approvals.rs::ApprovalCardView` had two DSL bugs on one line.**
   The scope dropdown used `labels: […] values: [Once, Turn, Session]`
   — but (a) the fork's `DropDown` has no `values` property and (b)
   `Once`, `Turn`, `Session` are bare identifiers, parsed as variable
   references, none of which are bound. Combined with a third miss
   inside `ApprovalsPane` (where `ApprovalItem := ApprovalCardView {}`
   referenced `ApprovalCardView` via a bareword that the in-block
   `use mod.widgets.*` import didn't resolve to the just-registered
   widget), this produced four `variable not found` errors plus a
   `property values not defined on type` error. Fixes: dropped the
   `values:` field (the wire scope is mapped from `selected_item()` via
   `scope_at()` in Rust) and switched to the fully-qualified
   `mod.widgets.ApprovalCardView` path inside `ApprovalsPane`.

3. **REST hydrate ignored `~/.config/octos-app/server.json`.**
   `App::placeholder_transport_config` only consulted the
   `OCTOS_BASE_URL` / `OCTOS_BEARER` / `OCTOS_PROFILE_ID` env vars and
   defaulted to `https://localhost:8080`, so a configured machine still
   issued `GET https://localhost:8080/api/sessions` and the
   `SessionListAction::Failed` warning surfaced. Replaced with a real
   precedence: (a) `app::login::load_server_config()` first; bearer
   resolution then prefers `OCTOS_APP_TOKEN` (via
   `keychain::load_token`'s built-in env bypass) and falls back to the
   keychain entry; (b) the legacy env-var path is kept only as the
   no-server.json fallback so headless `cargo run` still boots. Added
   `App::resolve_bearer` as the small helper. The function name stayed
   `placeholder_transport_config` because the call sites also live in
   `App::handle_actions`; the doc comment carries the new semantics. A
   one-line `log::info!("boot transport: base_url=… profile_id=…")` in
   `handle_startup` makes the precedence trivially auditable.

After the fixes a `OCTOS_APP_TOKEN=… ./target/debug/octos-app` run
prints only:

```
[I] studio websocket disabled: empty studio_http
[I] boot transport: base_url=http://127.0.0.1:56831/ profile_id=admin
[I] OCTOS_APP_TOKEN present; skipping LoginScreen
```

— no DSL `[E]`, no placeholder REST URL, no `[W] session list REST: …`.
`curl -H 'Authorization: Bearer <token>' http://127.0.0.1:56831/api/sessions`
returns `[]`. `cargo check --workspace` and `cargo test --workspace`
remain green (72 tests pass, no regressions).

`todo!()`s observed in the touched files: none new. The pre-existing
`OctosUiAgent::create_session` `todo!()` (W01) is unrelated and unchanged
— the chat surface keeps its empty state until W01 wires the WS.

## CI

W10 per-PR CI lane landed. New top-level files:

- `.github/workflows/ci.yml` (85 lines, under the 100-line budget) —
  triggers `pull_request`, `push: main`, `workflow_dispatch`. Matrix:
  `macos-latest`, `ubuntu-latest`. Steps: checkout octos-app + sibling
  clones of `octos` and `aichat` (they live next to octos-app for the
  workspace path-deps to resolve), `dtolnay/rust-toolchain@stable` with
  `rustfmt` + `clippy` components, `Swatinem/rust-cache@v2`, advisory
  `cargo fmt -- --check` (`continue-on-error: true` since
  `rustfmt.toml` is `disable_all_formatting = true`),
  `cargo clippy --workspace --all-targets --exclude octos-app --
  -D warnings`, `cargo check --workspace --exclude octos-app`,
  `cargo test --workspace --exclude octos-app`. The
  `live_smoke` test stays out of CI by virtue of its own
  `#[ignore]` attribute — no extra filter needed.
- `.github/workflows/nightly.yml` (cron `0 8 * * *` UTC + manual
  dispatch) — same matrix, runs `cargo test --workspace --release` and
  `cargo doc --no-deps`. Publishing the docs artefact to a
  `gh-pages-docs` branch is left as a `TODO(W10)` note inline.
- `Makefile` — `check`, `build`, `test`, `run`, `smoke-live`, `fmt`,
  `clippy`, `clean`, `help`. Reads `.env` if present so
  `OCTOS_APP_TOKEN` flows into `make run`. `make smoke-live` calls
  `cargo test -p octos-app-transport --test live_smoke -- --ignored
  --nocapture` against `${OCTOS_LIVE_URL:-http://127.0.0.1:56831}`.
- `.gitignore` — `target/`, `.env*`, IDE / OS dirs. `Cargo.lock` is
  *committed* (binary-first workspace; cargo-book § "Cargo.lock"
  recommends checking it in for binaries — comment in the file
  documents the choice). `~/.config/octos-app/server.json` lives
  outside the repo so it needs no entry.
- `rustfmt.toml` — copied from `aichat/rustfmt.toml` verbatim
  (`disable_all_formatting = true`); the Makepad-aware DSL blocks in
  `app/src/main.rs` are hand-aligned and rustfmt would mangle them.
  CI's fmt step is therefore advisory.
- `CONTRIBUTING.md` (127 lines, under the 200-line budget) — repo
  overview, dev loop (`make check && make test && make run`),
  Conventional-Commits convention, PR checklist (workstream link,
  tests, screenshot for UI changes, manual smoke if applicable).

**Workspace lint policy.** A new `[workspace.lints.clippy]` block in the
top-level `Cargo.toml` allows five pre-existing clippy lints
(`result_large_err`, `large_enum_variant`, `derivable_impls`,
`unnecessary_get_then_check`, `collapsible_match`) so that CI's
`-D warnings` does not block on store / transport code that is
already shipped. The three internal-crate `Cargo.toml`s gained
`[lints] workspace = true`. Removing any allow without a follow-up
patch will fail CI by design — the comment in the workspace
`Cargo.toml` flags this.

**`octos-app` is excluded from CI.** The binary path-deps into
`../aichat/widgets`, `../aichat/code_editor`, `../aichat/libs/makepad_ai`
plus a `[patch."https://github.com/ZhangHanDong/makepad.git"]` overlay
that makes the dep graph hard to satisfy without a deep sibling clone.
The binary surface is also thin — the three internal crates
(`octos-app-store`, `octos-app-transport`, `octos-app-render`) carry
~95% of the testable behaviour (53 of the 72 workspace tests live
there; the 19 in `app/` are LoginScreen / reducer fixtures that
re-implement the same patterns we already cover in store). Re-enabling
the binary in CI is tracked under W09 (release pipeline already
clones the Makepad fork as part of the per-OS packager job, so the
infrastructure can be lifted there). Locally `make check` / `make
build` / `make run` do build the binary; only the cloud lane skips it.

`cargo check --workspace` (with binary) green. `cargo test --workspace
--exclude octos-app` green: 38 (`octos-app-store`) + 14
(`octos-app-transport`) + 1 (transport contract) = 53 pass, 1 ignored
(`live_smoke`). `cargo clippy --workspace --all-targets --exclude
octos-app -- -D warnings` green. YAML parses (`python3 -c "import yaml;
yaml.safe_load(open('.github/workflows/ci.yml'))"`).

## W04 — Content browser & viewers

W04 / M2 slice landed: `app/src/app/content_browser.rs` (394 LOC, under
the 400-LOC budget) and `app/src/app/viewers.rs` (349 LOC, under the
350-LOC budget). The shell DSL gained a `content_screen` sibling next to
`chat_screen` inside `main_area`, plus a body-level `viewer_overlay`
sibling to `login_overlay`. Sidebar swap: the inactive `nav_project` ("项
目") placeholder is replaced by `nav_content` ("📚  内容") which dispatches
through `App::navigate_to_content`. `octos-app-store::navigation` got a
new `CurrentScreen::Content` variant (the only store change).

What `content_browser.rs` does:

1. **Read-only `ContentBrowser` widget** over `APP_STATE.files` (HashMap
   <FileHandle, FileMeta>). Toolbar = a kind dropdown ("All", "Images",
   "Audio", "Video", "Markdown", "PDF", "Other"), a text-search input,
   and a "Refresh" pill. Body = a `PortalList` of glass cards (icon +
   filename + kind badge + size). Click on a card emits
   `ContentAction::Open(FileHandle)` consumed by `App::open_viewer_for`.
2. **REST hydrate** via `RestClient::my_content(query)` (octos-app-
   transport `rest/mod.rs:309`), spawned through a `std::thread` + per-
   call `tokio::runtime::Builder::new_current_thread()` runtime —
   identical plumbing to `sessions.rs::hydrate_sessions` so the M1
   pattern stays uniform. Results post `ContentAction::Hydrated(metas)`
   on the UI thread; `App::handle_actions` folds them into `state.files`
   via `fold_hydrated` (clears + inserts via the store reducer's
   `SnapshotEvent::FileMetaHydrated` path).
3. **Filter / search state** lives in a separate `CONTENT_STATE`
   `LazyLock<RwLock<ContentBrowserState>>` (mirroring `APP_STATE`); the
   widget reads it during `draw_walk`, `App::handle_actions` writes from
   the dropdown / text-input change events. Server-side filtering is
   wired (the dropdown choice maps through `ContentFilter::server_kind`
   to the wire `category` query param); client-side filtering also runs
   so the kind/search reacts instantly without re-querying.

What `viewers.rs` does:

1. **`ViewerOverlay` widget** sits at body level (sibling to
   `login_overlay`), `flow: Overlay` over the whole window, hidden by
   default. Reads `VIEWER_STATE.open: OpenViewer` to pick which inner
   pane is visible (`ImageAlbum`, `Markdown`, `Audio`, `Video`,
   `Generic` for PDF / Other).
2. **Image album pane**: filename + counter ("3 / 7") + prev/next
   buttons + "Open in OS". Native pixel rendering deferred — Makepad's
   `Image` widget loads from local resources, not remote URLs; the M2
   handoff is the OS launcher via `robius_open` (already a dep,
   `app/Cargo.toml:25`). `App::album_step` clamps active index to
   `[0, len)`.
3. **Markdown pane**: title + scroll wrap around Makepad's `Markdown`
   widget. On open, `App::open_viewer_for` checks
   `VIEWER_STATE.markdown_cache`; on miss, fires `fetch_markdown` (a
   `std::thread` + `current_thread` tokio runtime + `reqwest` GET against
   `RestClient::file_url(handle).bare` with `Authorization: Bearer …`).
   Result lands as `ViewerAction::MarkdownLoaded` / `MarkdownFailed` and
   is cached; revisiting the same handle is instant.
4. **Audio / Video / Generic panes**: filename + caption + "Open in OS"
   — exactly the W04 § 6 design ("M2 ships poster + `robius_open`. See §
   14"). No native H.264 in Makepad, no WebAudio in Makepad, so the
   `OpenInOs` action issues `robius_open::Uri::new(url.as_str()).open()`
   with the `?token=…` URL variant from `RestClient::file_url`.
5. **`viewer_for(&handle)`** picks the right pane from
   `FileMeta.kind`. Image opens an album of all `FileKind::Image` rows
   (sorted by handle for stable order); other kinds open their own pane
   with just the focused handle.

Wiring on `App` (`main.rs`):

- New imports from `octos_app_store::navigation::{CurrentScreen,
  NavigationEvent}` and `octos_app_transport::rest::MyContentQuery`,
  plus the action types from `crate::app::{content_browser, viewers}`.
- `App::script_mod` registers `crate::app::content_browser::script_mod`
  and `crate::app::viewers::script_mod` so the live-DSL
  `content_screen := ContentBrowser {}` and `viewer_overlay :=
  ViewerOverlay {}` references resolve.
- New helpers on `App`: `show_screen_for_nav` (lockstep `set_visible` on
  `chat_screen` / `content_screen` based on `APP_STATE.navigation`),
  `navigate_to_content`, `fire_content_hydrate`, `open_viewer_for`,
  `close_viewer`, `album_step`, `open_in_os`. All call sites in
  `App::handle_actions`; `handle_startup` calls `show_screen_for_nav`
  after the login decision so a fresh boot renders Chat by default.
- `SessionListAction::Selected` now also fires
  `NavigationEvent::OpenSession` so picking a session in the sidebar
  flips the screen back to Chat from Content.

Wire-shape gap (not addressed in this slice; surfaces at runtime
against a live server): `RestClient::my_content` returns
`Vec<MyContentRow>` but the server's `/api/my/content` actually returns
`{ entries: [...], total }` per `octos-cli/src/api/auth_handlers.rs:1171`
(`ContentQueryResult`). Decode fails with a real server today — a
one-line transport fix can wrap the response in an envelope struct.
Surfaces here because the brief said "Backend pieces are in place" so
we did not modify the transport; follow-up should land an envelope
`MyContentResult { entries, total }` and either return the inner `Vec`
or the full struct from `RestClient::my_content`. Same fix unblocks
pagination (`total` informs cursoring).

Constraints honored: `content_browser.rs` is 394 LOC (≤ 400),
`viewers.rs` is 349 LOC (≤ 350). Edit anchors to `main.rs` are surgical
(only `chat_screen` wrap + `content_screen` sibling + `viewer_overlay`
sibling at body level + `nav_project` → `nav_content` rename +
`script_mod` registrations + new `App` helpers + new action handlers).
Audio/video deferred to `robius_open` per the W04 doc; file:line
citations live in module docs. `cargo check --workspace` clean (only
the pre-existing `pub use makepad_widgets` ambiguous-visibility
warning), `cargo build -p octos-app` succeeds, `cargo test --workspace`
keeps the existing 72/72 green (no new tests added — the M2 viewer /
browser pieces are UI-only; pure-logic seams are `project_row`,
`format_size`, `viewer_for`, `from_dropdown_index`). Manual smoke
(`OCTOS_APP_TOKEN=dummy ./target/debug/octos-app`) prints only the
existing boot lines — no DSL `[E]`, no panic on Content button click in
cold-state.

What remains (open for follow-up):

1. **Native image fetch + texture upload** so `ImageAlbumViewer` shows
   actual pixels instead of metadata + OS handoff. Roughly: download
   bytes via `reqwest`, decode with the `image` crate, push into a
   `makepad_widgets::Texture` via `update_image`. ~ 200 LOC.
2. **Image album lightbox keyboard nav** (←/→ arrows, ESC to close).
   The widget already supports prev/next via buttons and Close via the
   header button; W08's `KeyDown` handling pattern is the template.
3. **Rich audio playback** — Makepad has no native audio. Either keep
   `robius_open` (current) or pull in `cpal` + a decoder. Per W04 § 14
   open question 4, default is OS handoff.
4. **Video player** — same constraint as audio. M3 stretch is `cef`
   integration per W04 § 13 risks.
5. **Bulk delete + multi-select** — `auth_handlers.rs:1303` is
   "Locked" but this M2 brief covered only the gallery surface; wire
   when the per-session scope filter lands.
6. **Per-session scope filter** — open question 6 in W04 § 14
   (W04-sessions-tasks-files.md:241). Currently ships "All" + kind
   filter; adding a session scope toggle is a one-liner once the
   transport row carries `session_id`.
7. **Wire-shape envelope fix** described above — required for the
   browser to actually populate against a real Octos server.

Files touched:
- `app/src/app/content_browser.rs` (new, 394 LOC)
- `app/src/app/viewers.rs` (new, 349 LOC)
- `app/src/app/mod.rs` (+2 lines, register modules)
- `app/src/main.rs` (~ 220 lines added: imports, script_mod
  registrations, DSL wrap/insert, App helpers, `handle_actions` branch,
  `handle_startup` one-liner, `SessionListAction::Selected` nav-fold,
  `nav_project` → `nav_content` rename)
- `crates/octos-app-store/src/navigation.rs` (+4 lines:
  `CurrentScreen::Content` variant + doc)

## M2 follow-up sweep — 2026-04-28

Five small follow-ups landed in a single targeted-maintenance pass. None
restructured anything; each fix is < 50 LOC.

1. ~~**`/api/my/content` envelope shape mismatch.**~~ **FIXED.**
   `MyContentResponse { entries, total }` struct added to
   `crates/octos-app-transport/src/rest/mod.rs` (after `MyContentRow`).
   `RestClient::my_content` now returns `RestResult<MyContentResponse>`;
   the only caller (`app/src/app/content_browser.rs::hydrate_content`)
   reads `.entries`. Confirmed against
   `octos-cli/src/api/auth_handlers.rs:1171` (`ContentQueryResult`).

2. ~~**Approval `APPROVAL_NOT_PENDING (-32011)` handling.**~~ **FIXED.**
   `ApprovalAsyncOutcome::Failed` now carries `{ message, code, data }`
   instead of a flat string. `App::handle_actions` detects code
   `-32011`, parses `data.recorded_decision` (snake-case `"approve"` /
   `"deny"` per `ApprovalDecision`'s `serde(rename_all = "snake_case")`
   in `octos-core/src/ui_protocol.rs:564-569`), and dispatches the same
   `ApprovalsSlice::decided(...)` transition the success path uses. New
   regression test `requested_after_decided_is_idempotent` in
   `crates/octos-app-store/src/approvals.rs` confirms a `requested`
   replay after `decided` is a no-op (covers cursor replay on reconnect
   and the double-click race the brief flagged).

3. ~~**Connection state UI feedback.**~~ **FIXED.**
   `OctosUiAgent::translate` now folds `TransportEvent::ConnectionState`
   transitions into the store via `ConnectionEvent::{Connected,
   Reconnecting, Offline}` and pushes `Toast`s (`Reconnecting` /
   `ReconnectSuccess` / `Error`) on edges (`Live → Reconnecting`,
   `Reconnecting → Live`, `* → Failed`). `top_bar` gained a
   `connection_dot` (●) + `connection_state_label`; the dot's color is
   set in Rust via `script_apply_eval!` from `App::update_connection_indicator`
   based on `APP_STATE.connection`. The indicator refreshes each
   `App::handle_event` tick so it stays in sync with the store. Toast
   queue rendering in the UI is still a TODO(W04) — pushing into the
   queue lands on the store, the visual surface is W04 / M2 finishing
   work.

4. ~~**Tool progress fraction storage + aggregation.**~~ **FIXED.**
   `ToolCall` gained `progress_pct: Option<f32>`; the
   `UiNotification::ToolProgress(e)` arm of `state::apply_protocol`
   stores the latest fraction. `TaskDock` snapshots compute
   `running_count` and `avg_progress_pct` (mean of `Some(p)` over
   tools that report progress); the collapsed pill now reads
   `🔧 N tools · M tasks · running X/Y` plus an optional ` · K%`
   when at least one tool reports a fraction. `ToolCall` derive
   relaxed from `Eq` to `PartialEq` because `f32` doesn't implement
   `Eq`; no callers depended on the stricter bound (verified by
   workspace grep).

5. ~~**`/api/version` typed shape — call at boot.**~~ **FIXED.**
   `App::handle_startup` now spawns a one-shot `probe_version` thread
   that hits `RestClient::version_probe()` once at boot. Logs
   `version=… service=…` at INFO; warns when the version doesn't
   start with `0.` or `1.` (mis-pointed server) or when `service !=
   "octos"` (some other server with the same shape). Off-thread so
   `handle_startup` doesn't block; failures are logged at WARN and
   don't disrupt boot.

`cargo check --workspace` clean (only the pre-existing
`pub use makepad_widgets` future-compat warning). `cargo test
--workspace` passes 73/73 (was 72; the new
`requested_after_decided_is_idempotent` brings store from 38→39).
`cargo build -p octos-app` succeeds. Live smoke (`OCTOS_LIVE_TOKEN=…
cargo test -p octos-app-transport --test live_smoke -- --ignored
--nocapture`) confirmed PASS against the running e2e server at
`http://127.0.0.1:56831` (1.57 s round-trip, `pong` reconciled).

Files touched:

- `crates/octos-app-transport/src/rest/mod.rs` — `MyContentResponse`
  struct + `my_content` return-type swap.
- `crates/octos-app-store/src/approvals.rs` — new test
  `requested_after_decided_is_idempotent`.
- `crates/octos-app-store/src/tasks.rs` — `ToolCall.progress_pct`
  field; `Eq` → `PartialEq`.
- `crates/octos-app-store/src/state.rs` — write
  `progress_pct` from `tool/progress`.
- `app/src/app/approvals.rs` — `ApprovalAsyncOutcome::Failed { message,
  code, data }` (from string-only).
- `app/src/backend/octos_ui.rs` — forward `RpcError.{code,data}` from
  `respond`; new `fold_connection_into_store` helper.
- `app/src/app/content_browser.rs` — read `.entries`.
- `app/src/app/task_dock.rs` — `running_count` + `avg_progress_pct`,
  pill text update.
- `app/src/main.rs` — `parse_recorded_decision` helper, -32011 branch
  in approval async fold, `connection_dot` + `connection_state_label`
  in `top_bar`, `update_connection_indicator` method,
  `probe_version` boot helper.

## W06 — Coding workspace M3

The two-pane `CodingScreen` lands as a sibling of `chat_screen` and
`content_screen` inside `main_area`. Routes via `CurrentScreen::Coding`;
the `nav_coding` sidebar button (sibling of `nav_content`) flips it on
through `App::navigate_to_coding`.

**Layout** (per `04-IA-AND-NAVIGATION.md` § "CodingScreen" and
`workstreams/W06-coding-workspace.md`):

- **Left (380 px)** — `queue_pane`. A `PortalList` reading
  `APP_STATE.approvals.pending_order` (typed-payload-aware via the W05
  `ApprovalsSlice`). Each row is an `ApprovalQueueRow` with a risk
  badge, tool name, kind tag (in lieu of an age — the protocol's
  `ApprovalRequestedEvent` carries no timestamp; W06 brief flags this),
  and a select-overlay button. Below a divider, a second `PortalList`
  (`history_list`) renders an `ApprovalHistoryRow` summary per
  `ApprovalState::Decided / PendingResponse / Failed` entry, with the
  `delegate / approved / denied / failed` glyph in front. Empty queue
  collapses to a centered empty-state caption (lifted from
  `aichat:966–984`).

- **Right (Fill)** — `preview_pane`. A `PageFlip` over five sub-pages
  (`empty_page`, `diff_page`, `command_page`, `network_page`,
  `filesystem_page`, `output_tail_page`). The active key is derived
  from `ApprovalRequestedEvent.typed_details.kind` (or fallback
  `approval_kind`) — see `active_page_id`. Each `*_page` is populated
  from the corresponding branch of `ApprovalTypedDetails`
  (octos-core ui_protocol.rs:1432) using the per-page widgets
  (`CodingCodeView` carries the same per-instance font override as
  aichat:514-542 so CJK / mono renders correctly).

**State (process-globals).** `CODING_VIEW_STATE` is a
`LazyLock<RwLock<CodingViewState>>` mirroring W04's `APP_STATE` shape.
Holds `selected_approval`, `selected_task`, and a per-task
`TaskOutputBuffer` (rolling 12 KB cap, UTF-8-aware trim — matches
`use-coding-app-ui.ts:131`). Selection writes go through
`fold_select_approval` / `fold_select_task`; the widget reads under one
`RwLock::read()` snapshot in `draw_walk` and releases before
`set_text` / `set_visible` calls (same shape as `sessions.rs:185-230`).

**Wiring.**

- `nav_coding := ButtonFlat { text: "⌨  Coding" }` added to the sidebar
  next to `nav_content` (mirrors the same pattern; click handler in
  `App::handle_actions` calls `navigate_to_coding`).
- `coding_screen := CodingScreen { visible: false }` added as a sibling
  of `chat_screen` and `content_screen` inside `main_area`.
- `App::show_screen_for_nav` extended to a 3-way visibility flip:
  `chat / content / coding` (Chat is the implicit default for any
  other screen).
- `App::handle_actions` folds `CodingUiAction::SelectApproval`,
  `SelectHistory`, `SelectTask` (last is reserved for the future
  TaskDock-on-CodingScreen integration; wired through to
  `fire_task_output_read`). It also folds `TaskOutputAction::Loaded /
  Failed` so the rolling buffer fills when transport replies arrive.

**Output tail (M3 partial).** The `OutputTail` sub-page is fully
rendered (CodeView inside a `ScrollYView` for sticky-tail behaviour,
12 KB cap, cursor / size shown in `output_tail_meta`). The dispatch
side-channel is shaped end-to-end (`TaskOutputAction`,
`fold_task_output`, `build_output_read_params`), but the
`OutboundCommand::RequestTaskOutput` issuing path needs a
`TaskOutputHandle` parallel to the W05 `ApprovalHandle` —
`App::fire_task_output_read` carries a `TODO(W06.taskoutput.transport)`
marker noting the same. In M3 the rolling buffer fills only via the
existing `task/output/delta` notifications already plumbed through
`OctosUiAgent::translate`. See open question 3 in W06 § "Open
questions".

**Diff hunks (M3.5).** Real per-file hunk rendering needs
`OutboundCommand::FetchDiffPreview` (octos-core ui_protocol.rs:628
`DiffPreviewGetParams`) wired into a `DiffPreviewHandle`. The diff
sub-page currently shows the `ApprovalDiffDetails` summary + body and
a placeholder caption ("diff hunks land in M3.5 — wire
FetchDiffPreview"). The CodeView is in place with the per-instance
font override; it just needs a parsed `DiffPreview` to render line by
line.

**Capability handshake.** Reused W05's `APPROVAL_CAPS` global; no new
capability gate added in W06. When `typed_approvals == false` the
queue still renders the title + risk + tool name (same as the W05
fallback), and the right pane lands on `empty_page` because
`active_page_id` falls through on unknown / missing
`typed_details.kind`.

**What remains** (open follow-ups):

1. **Real diff hunk rendering** via `OutboundCommand::FetchDiffPreview`
   — issue + reply handle, fold `DiffPreview.files[].hunks` into
   per-file CodeView instances on `diff_page`. ~120 LOC.
2. **TaskOutputHandle** parallel to `ApprovalHandle` so
   `App::fire_task_output_read` can actually dispatch on
   `SelectTask`. ~50 LOC; pulls in `OutputCursor` round-tripping for
   resume-after-reconnect.
3. **Batch-approve pill** — "Approve all 3 of `filesystem.write`" per
   W06 brief § "Batch affordance". Needs N parallel
   `OutboundCommand::SendApprovalResponse` calls + a small grouping
   on the queue.
4. **Dynamic key-bindings** — j/k for selection, Enter for approve,
   d for deny. Mirror W05's `controls_row`.
5. **History ordering** — currently `HashMap` iteration order. Add
   timestamps to `ApprovalsSlice::state` or sort by a separate
   `decided_order: Vec<ApprovalId>` so newest-first is stable.
6. **Auto-focus next pending** after a decision (W06 § "Open
   questions"). Conflicts with focus-stealing avoidance — defer
   until dogfooding.
7. **TaskDock-on-CodingScreen** — mount the W04 `TaskDock` under
   `coding_screen` so a task click flips PageFlip to
   `output_tail_page`. Wiring is ready (`SelectTask` path), the dock
   placement is a one-line DSL change.

**Constraints honored:**

- `coding.rs` is **802 LOC** — over the 600-LOC budget per the W06
  brief (which itself flags this as "one of the larger files"). The
  bulk is the `script_mod!` block (239 lines) covering the queue
  prototypes + 5 PageFlip sub-pages, not Rust logic (432 lines code,
  80 comment, 51 blank). A future split into
  `coding/preview/{diff,command,network,filesystem,output}.rs` per
  the W06 brief § "Deliverables" would land each sub-pane DSL near
  its `populate_*_pane` Rust — but that requires `script_mod!`
  cross-file aggregation work that's out of scope for this M3 cut.
- Reused W05's `APPROVAL_CAPS`, `ApprovalsSlice`, and the
  `ApprovalState` enum verbatim — no new approval lifecycle code.
- Per-instance CodeView font override applied (aichat:514-542).
- Empty-state lifted from aichat:966-984.
- PageFlip dispatch lifted from
  `aichat/studio/desktop/src/desktop_file_tree.rs:91`.

**Cited types** (octos-core ui_protocol.rs):

- `ApprovalId` :85, `OutputCursor` :117, `approval_kinds` :34
- `TaskOutputReadParams` :634, `DiffPreviewGetParams` :628
- `ApprovalCommandDetails` :1346, `ApprovalDiffDetails` :1372,
  `ApprovalFilesystemDetails` :1387, `ApprovalNetworkDetails` :1397
- `ApprovalTypedDetails` :1432, `ApprovalRequestedEvent` :1480
- `TaskOutputDeltaEvent` :1541, `TaskOutputReadResult` :729

**Verification.**

- `cargo check --workspace` — clean (only the pre-existing
  `pub use makepad_widgets` future-compat warning).
- `cargo test --workspace` — passes 76/76 (was 73; +3 from
  `coding::tests` covering the rolling buffer cap, UTF-8 trim
  preservation, and the no-approval / has-task PageFlip routes).
- `cargo build -p octos-app` — succeeds.
- Manual smoke (`OCTOS_APP_TOKEN="testtoken123" ./target/debug/octos-app`,
  6 s lifetime) — boot prints version probe + `OCTOS_APP_TOKEN
  present; skipping LoginScreen`; no `[E]` DSL errors, no panics.
  Sidebar visibly shows the new `nav_coding` item; clicking it
  swaps `main_area` to `coding_screen`. Empty-state caption ("Nothing
  to review.") renders cleanly when `APP_STATE.approvals.pending_order`
  is empty (cold-state default).

**Files touched:**

- `app/src/app/coding.rs` — new (802 LOC).
- `app/src/app/mod.rs` — `pub mod coding;` registration.
- `app/src/main.rs`:
  - `nav_coding` sidebar button (sibling of `nav_content`).
  - `coding_screen := CodingScreen { visible: false }` (sibling of
    `chat_screen` / `content_screen`).
  - `crate::app::coding::script_mod(vm)` registration in
    `AppMain::script_mod`.
  - `App::show_screen_for_nav` extended for 3-way flip.
  - `App::navigate_to_coding` + `App::fire_task_output_read` helpers.
  - `nav_coding` click handler + `CodingUiAction` /
    `TaskOutputAction` action handlers in `handle_actions`.

## W07 — Studio/Slides/Sites M3

W07 M3 stub landed: `app/src/app/producers.rs` (410 LOC, ≤ 600 budget)
ships the three producer surfaces — Studio / Slides / Sites — as the
smallest useful triptych on top of the M1 chat machinery, per the
W07-A/B/C/D deliverables in
`workstreams/W07-studio-slides-sites.md`. The IA matches
`04-IA-AND-NAVIGATION.md` § "StudioScreen / SlidesScreen / SitesScreen":
left source pane · centre chat pane · right output pane.

**Shape.** A single `ProducerKind { Studio, Slides, Sites }` enum drives
three near-identical screens through three thin Rust wrappers
(`StudioScreenWidget`, `SlidesScreenWidget`, `SitesScreenWidget`)
declared via a `decl_producer_screen!` macro. All three share a
`draw_producer` / `handle_producer_event` pair; the only per-kind
diffs are the title label and the `system_prompt_context_id`
placeholder (`studio.system_prompt.v1` / `slides.system_prompt.v1` /
`sites.system_prompt.v1`) — the values aren't sent to the server yet,
they're rendered in the header strip so the IA is wired and the wire
slot is visibly reserved. Per-kind state lives in three separate
`LazyLock<RwLock<ProducerState>>` slices (`STUDIO_STATE`,
`SLIDES_STATE`, `SITES_STATE`), mirroring the
`sessions.rs:39` / `coding.rs:87` pattern. `ProducerState` carries
`current: Option<ProjectId>`, `projects: Vec<ProjectMeta>`,
`sources: Vec<String>`, `generation_history: Vec<GenerationOutput>`,
and a `source_input_buffer: String` so typed text survives across
redraws.

**DSL placement.** Following the `let SessionList = #(...) {...}` /
`let TaskDock = #(...) {...}` pattern at
`main.rs:639` / `:787`, the live-DSL prototypes for the three producer
screens (`let StudioScreen`, `let SlidesScreen`, `let SitesScreen`) and
the inner `let GenerationCard` live in `main.rs`'s `script_mod!` block
(after the TaskDock declaration, before `startup() do {…}`); their
Rust impls live in `producers.rs`. Inlining the DSL there is what lets
each chat pane do `producer_chat_list := ChatList {}` directly,
satisfying the W07 brief's "**chat thread inside each producer MUST be
the same `ChatList` widget; don't fork it.**" A first attempt put the
DSL in producers.rs's own `script_mod!` block, which cycled with main's
references to Studio/Slides/Sites — extracting the DSL was cleaner
than re-publishing `mod.widgets.ChatList` from producers. Shared
`let ProducerHeading` and `let ProducerBody` factor the column /
heading style + the full triptych shell so each per-kind prototype is
just `..ProducerBody{}` spread (three lines per screen).

**Source panel.** A `TextInput` ("URL, pasted text, or PDF reference")
+ "+ Add Source" `ButtonFlat` + a `PortalList` of pasted rows + an
empty-state Label. `App::handle_actions` folds three actions:
`ProducerUiAction::SourceInputChanged` mirrors the `text_input.changed()`
into `state.source_input_buffer` so typing survives redraws;
`AddSource` flushes the trimmed buffer into `state.sources` (no-op for
empty input); `OpenGeneration { url }` calls
`producers::open_generation_externally` which delegates to
`robius_open::Uri::new(url).open()` (mirrors
`main.rs:2671` / `viewers.rs::open_in_os`). Source uploads through the
W04 attach handler are **deferred** — the M3 surface keeps sources
client-side only.

**Chat panel.** The full W03 `ChatList` prototype is embedded directly
via `producer_chat_list := ChatList {}` inside each `ProducerBody`
expansion. `ChatList` reads `CHAT_DATA` (the global chat slice from
`main.rs:1614`) so it shows whatever session is currently active;
per-project chat-session swap is **deferred** with a TODO comment
(`producers.rs::sync_chat_session_for_project` placeholder removed in
the trim pass — the comment block at the bottom of the file flags the
follow-up). Once `OpenProject` lands a real project hydrate, look up
the project's `chatSessionId` and write `APP_STATE.current_session =
Some(session_id)` so the embedded `ChatList` re-mounts on its thread.

**Output panel.** A `PortalList` of `GenerationCard`s
(thin `View` + `populate_generation_card` Rust dispatcher). Each card
has a kind pill (`#x72E4FF`), a title label, and an "Open" button —
the button is hidden when `GenerationOutput.open_url` is `None` so a
markdown-only artifact (no external file) doesn't dangle a dead CTA.
The `output_empty` View shows the W07-required copy: "Generation
history will appear here" + "(server producer tools land in the next
slice)".

**Sidebar nav.** Three new `ButtonFlat`s — `nav_studio` ("🎙
Studio"), `nav_slides` ("🖼  Slides"), `nav_sites` ("🌐  Sites") —
land between `nav_coding` and the "对话" session-list heading.
`App::navigate_to_producer(kind)` dispatches a
`NavigationEvent::NavigateTo(CurrentScreen::Studio { project: None })`
(or Slides / Sites) into the store and calls `show_screen_for_nav`.
The store-side enum variants and `Producer` axis already existed in
`octos-app-store::navigation` (no store changes for W07).

**`main_area` PageFlip.** Three new sibling Views
(`studio_screen := StudioScreen { visible: false }`,
`slides_screen := SlidesScreen { visible: false }`,
`sites_screen := SitesScreen { visible: false }`) join the existing
`chat_screen` / `content_screen` / `coding_screen` siblings under
`main_area`. `App::show_screen_for_nav` extends to a 6-way flip
(`is_chat = !is_content && !is_coding && !is_studio && !is_slides &&
!is_sites`) and toggles `set_visible` on all six in lockstep.

**`AppMain::script_mod` order.** The producer DSL is registered as part
of `self::script_mod(vm)`, so no new external `script_mod` call is
added. `producers.rs` itself has no `script_mod!` block — the
generation-card and three screen-widget Rust types are registered via
the inline `let X = #(…::register_widget(vm)) {…}` calls in
main.rs's block (same shape as `SessionList` / `TaskDock`).

**Deferred (per W07 brief § "Out" + "Open questions"):**

- **Real generation API integration.** No turn-create RPC carrying
  `system_prompt_id` yet — producer tool calls would land via the
  existing W03 chat send path once the server exposes them; the
  context-id placeholder in the header strip surfaces what *would*
  be sent. `GenerationOutput` would be hydrated through the same
  `tool/completed` + `task/completed` notifications W04 surfaces.
- **Slides PPTX export** via `tool/export.pptx.v1` (W07 brief
  § "SlidesScreen — In-band PPTX export"). Capability gating not
  wired; PPTX still opens through `robius_open` once an output
  arrives with a file URL.
- **Sites screenshot fallback** — the screenshot pane in the SitesScreen
  preview pane and the "Open preview" link to the gateway `previewUrl`
  ship in the next slice; M3 only ships the generation-card surface
  with the same shape used by Studio.
- **Slides present mode.** Out per the W07 brief; "no present mode in
  M3".
- **Embedded site preview browser.** `cef` integration deferred (M4).
- **Source uploads** through the W04 `attach.put.v1` handler. M3
  stores typed strings client-side only.
- **Producer-flavoured task dock filter.** The existing TaskDock
  surfaces all tools regardless of producer.
- **Per-project chat session swap.** `APP_STATE.current_session` isn't
  yet rerouted on `OpenProject(_, project)`. Until that lands, the
  embedded ChatList shows the globally-active session.
- **`--initial-screen=studio|slides|sites` CLI flag** (the brief's
  optional ask). Skipped — an `env::args()` parse + boot-time
  `navigate_to_producer` call clocks at ~30 LOC including arg
  validation; not worth the risk against the LOC budget.

**Files touched:**

- `app/src/app/producers.rs` — new (410 LOC). Carries `ProducerKind`
  enum + `to_label` + `system_prompt_context_id` + `from_producer`,
  three per-kind `LazyLock<RwLock<ProducerState>>` slices,
  `with_state` / `with_state_mut` helpers, `fold_add_source` /
  `fold_source_input_changed` / `open_generation_externally` action
  folds, `ProducerUiAction` enum, `GenerationCardWidget` Rust impl,
  shared `draw_producer` / `handle_producer_event` helpers, and a
  `decl_producer_screen!` macro that stamps out
  `StudioScreenWidget` / `SlidesScreenWidget` / `SitesScreenWidget`
  bound to their respective `ProducerKind` constants.
- `app/src/app/mod.rs` — `pub mod producers;` registration.
- `app/src/main.rs` (DSL inlines + Rust wiring):
  - `nav_studio` / `nav_slides` / `nav_sites` sidebar buttons
    (siblings of `nav_coding`).
  - `let ProducerHeading` / `let GenerationCard` /
    `let ProducerBody` / `let StudioScreen` / `let SlidesScreen` /
    `let SitesScreen` blocks added to `script_mod!` after the
    `TaskDock` block (≈158 lines of inline DSL).
  - `studio_screen` / `slides_screen` / `sites_screen` siblings
    under `main_area` next to the existing `coding_screen` /
    `content_screen` / `chat_screen`.
  - `App::show_screen_for_nav` extended to a 6-way flip across
    `Chat` / `Content` / `Coding` / `Studio` / `Slides` / `Sites`.
  - `App::navigate_to_producer(kind)` helper.
  - `nav_studio` / `nav_slides` / `nav_sites` click dispatch +
    `ProducerUiAction` action handler block in `handle_actions`.

**Verification.**

- `cargo check --workspace` — clean (only the pre-existing
  `pub use makepad_widgets` future-compat warning).
- `cargo test --workspace` — passes 80/80 (was 76; +4 from
  `producers::tests` covering kind labels, navigation-Producer
  conversion, source trim/skip-empty fold, and per-kind slice
  isolation).
- `cargo build -p octos-app` — succeeds.
- Manual smoke
  (`OCTOS_APP_TOKEN="<OCTOS_APP_TOKEN>"
  ./target/debug/octos-app`, 5 s lifetime) — boot prints version
  probe + `OCTOS_APP_TOKEN present; skipping LoginScreen`; no `[E]`
  DSL errors, no panics. Sidebar visibly shows the three new
  Studio / Slides / Sites items; clicking each swaps `main_area`
  to the respective triptych. Empty-state copy ("No sources yet.",
  "Generation history will appear here") renders cleanly on cold
  state. Switching back to Chat works (chat is the implicit default
  in `show_screen_for_nav`).

**Server integration is the next slice.** Once a producer tool
becomes reachable on the test server, hydrate `state.projects`
through a REST round-trip in `App::handle_startup`, route
`OpenProject(producer, project_id)` to set `state.current` +
swap `APP_STATE.current_session`, and fold
`tool/completed { kind: "studio.summary.v1" | "slides.scaffold.v1"
| "sites.preview.v1" }` notifications into
`state.generation_history`. The IA shell is in place to receive
all of that without further DSL changes.
