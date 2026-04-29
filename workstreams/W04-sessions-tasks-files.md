# W04 — Sessions, tasks, files

## 1. Mission

Own everything that talks REST and renders non-chat surfaces: session sidebar,
task dock, file viewers, content browser. W03 owns the thread, W01 owns the
wire; this workstream binds the two via snapshot REST hydration on cold start
and a typed reducer for live `tool/*` and `task/*` notifications. We hide
session quirks (missing handles, deferred files, stale tasks) so W03 can treat
its session pointer as a single `SessionId`. Straddles M1 (sessions) and M2
(dock + viewers + gallery).

## 2. Header

| Field | Value |
|---|---|
| ID / Lane | W04 / B |
| Depends on | W01 (transport + protocol types), W02 (shell + sidebar slot) |
| Unblocks | W05 (approvals reuse task-output rendering), W07 (Studio reuses viewers + content browser) |
| Milestones | M1 (sessions); M2 (tasks + files + content browser) |
| Lifts from | `octos-web/src/components/{session-list,session-task-dock,content-browser}.tsx`; `octos-web/src/components/viewers/*`; `octos-web/src/api/{sessions,files,content,chat}.ts` |
| Wire | REST snapshot (sessions, files, my/content, upload) + `tool/*` / `task/*` from W01 |
| Output | `octos-app-store::{sessions,tasks,files}`; UI in `app/app/{sessions,task_dock,files}.rs` |

## 3. Scope

**In, M1.** `GET /api/sessions` hydrate; `DELETE /api/sessions/{id}` with
confirmation; `GET /api/sessions/{id}/messages` (paginated, `since_seq`-aware).
`SessionList` widget. `SessionMap` slice + selectors. REST client in
`octos-app-transport::rest` fans into the same `Event` stream `OctosUiAgent`
emits, so the reducer treats hydrate and live identically.

**In, M2.** Task dock: `tool/started/progress/completed` +
`task/updated/output/delta` in a collapsible per-turn timeline;
`task/output/read` drill-down. File browser + viewers (image album, audio,
video, markdown) over `/api/files/{handle}`. Multipart upload via
`POST /api/upload`; staged handles attach to the next `turn/start`. Content
browser over `/api/my/content` (filter/search/sort/pagination, single + bulk
delete).

**Out.** Chat thread (W03). Approvals / diff preview (W05). Drag-and-drop
upload (defer; modal picker). `/api/sessions/{id}/events/stream` SSE — WS
covers all interactive paths per `03-PROTOCOL-CONTRACT.md` § Shape; SSE is
legacy in `02-API-DRIFT.md`. Workspace contract / pane snapshots (W05).
Session search and slide present mode.

## 4. Sessions sub-surface

`PortalList` in the sidebar's lower PageFlip slot
(`04-IA-AND-NAVIGATION.md` § Sidebar): rows with status dot, title,
hover-revealed delete.

REST hydrate **Locked**: `GET /api/sessions` (`handlers.rs:476`) merges
standalone store + gateway proxy (`handlers.rs:497-510`). Re-fetch on login,
profile switch, refresh, or `cursor_invalid` (`01-ARCHITECTURE.md` § 7).

History **Locked**:
`GET /api/sessions/{id}/messages?limit&offset&source=full&since_seq&topic`
(`handlers.rs:612`). Cold: `since_seq=0`, limit 500 (matches
`octos-web/src/api/sessions.ts:42`). Reconnect reads the WS cursor; messages
ride `since_seq` — two parallel watermarks.

Delete **Locked**: `DELETE /api/sessions/{id}` (`handlers.rs:909`) clears
standalone + gateway. UI: first click → confirm pill (mirrors
`octos-web/src/components/session-list.tsx:111-132`); ✓ commits, ✗ / 5 s
reverts. Optimistic; failure re-hydrates + toast.

Streaming dot watches live events (`tool/started`, `tool/progress`,
`task/updated{runtime_state=running}`, `message/delta` → amber;
`tool/completed` + `turn/completed` clear) plus `pending_turns:
HashMap<SessionId, u32>` so background tasks
(`task/updated{lifecycle=spawned|running}` whose origin turn completed) keep
it lit — web's `useAllTasksBySession()` equivalent, surfaced as
`is_session_active(session)`. Green = idle, amber = in flight, grey =
reconnecting. Click → `OpenSession(SessionId)`; W03 re-mounts.

## 5. Task dock sub-surface

Collapsible under the composer (`04-IA-AND-NAVIGATION.md` ChatScreen).
Visible iff the current session has at least one active tool/task;
auto-collapses 30 s after terminal. Sources durable
(`03-PROTOCOL-CONTRACT.md` § Tool / task / progress events):
`tool/started → tool/progress* → tool/completed` correlate by `tool_call_id`;
`task/updated` and `task/output/delta` correlate by `task_id`. Types from
`octos-core/src/ui_protocol.rs`.

```text
tool/started      → tool_calls[id] = { name, status: Running, … }
tool/progress     → mutate progress / message in place
tool/completed    → status = Completed { success, output_preview, duration_ms }
task/updated      → tasks[id].{ lifecycle, runtime, summary }
task/output/delta → tasks[id].tail.push(bytes); next_cursor = cursor + bytes.len()
```

Unknown variants drop with a debug log per the contract's forward-compat rule.
Tools render `tool_name · message · spinner`; tasks render
`display_name(tool_name) · phase · last_progress_event` (mirrors
`octos-web/src/components/session-task-dock.tsx:14-93`). Click → tail viewer
lazy-fetches `task/output/read { session_id, task_id, after: known_cursor,
limit_bytes: 64KiB }` over the same WS, keeping a 256 KiB rolling ring;
further drill downloads `task.output_files[]` via `/api/files/{handle}`.
Events carry `session_id`; non-current drop — dock stays stable across
switches.

## 6. File viewers

All consume pre-signed handles from `response_path_for_profile_file`
(`handlers.rs:68-74`) — either a profile-scoped path
(`encode_profile_file_handle`) or a tmp-upload handle
(`encode_tmp_upload_handle`). Both decode safely on `GET /api/files/{handle}`
(`handlers.rs:1217`) and `GET /api/files?path=…` (`handlers.rs:1199`). Opaque
strings, never path-manipulated client-side. URL =
`format!("{base}/api/files/{}", url_encode(handle))`; inline media appends
`?token=…` (matches `octos-web/src/api/files.ts:8` + auth-middleware fallback
at `router.rs:546`). All three endpoints are **Locked**.

| Viewer | Lift from | Notes |
|---|---|---|
| `ImageAlbumViewer` | `viewers/image-album-viewer.tsx` | Prev/next, zoom, ESC. |
| `AudioPlayer` | `viewers/audio-player.tsx` | Play/pause, scrubber, sticky drawer. |
| `VideoPlayer` | `viewers/video-player.tsx` | No native H.264; M2 ships poster + `robius_open`. See § 14. |
| `MarkdownViewer` | `viewers/markdown-viewer.tsx` | Reuses W03's streaming-markdown renderer. |

Each takes a `FileMeta` from `AppState.files`, not a bare handle.

## 7. Content browser

The "Content" sidebar item, backed by `GET /api/my/content`
(`auth_handlers.rs:1122`), **Locked**. Each entry's `path` is already a signed
handle (`auth_handlers.rs:1160-1163`). Server-side filters `category`,
`search`, `from`/`to`, `sort` (`newest|oldest|name|size`), `limit`, `offset`
match `octos-web/src/api/content.ts:38-54`.

Grid: image/slides/video thumbnails, audio rows, markdown rows. Thumbnails
via `/api/my/content/{id}/thumbnail` (`auth_handlers.rs:1178`); inline body
via `/api/my/content/{id}/body`; full download via `/api/files/{handle}`.
**Reuses `PortalList` from aichat**
(`aichat/examples/aichat/src/main.rs:343-614`) for virtualised scroll.

Single delete `DELETE /api/my/content/{id}` (`auth_handlers.rs:1263`); bulk
`POST /api/my/content/bulk-delete { ids }` (`auth_handlers.rs:1303`). Both
Locked. Optimistic remove + toast on failure. Also W07's surface for past
Studio outputs — same widget, filtered by `tool_name`.

## 8. Upload flow

Composer `+` triggers a native picker (`rfd` crate; Makepad has no native
dialog). Files post to `POST /api/upload` multipart (`handlers.rs:944`),
**Locked**. Limits: 50 MB / file, 100 MB total (`handlers.rs:960-1000`).
Response: `Vec<String>` of pre-signed handles.

Handles stage in `AppState.composer.pending_handles`; the next `turn/start`
consumes them as `InputItem::Attachment { handle }`. Round-trip server-side —
never resolved to bytes. Failures toast and clear staging. For Studio
direct-into-workspace uploads, `POST /api/site-files/upload`
(`handlers.rs:1034`) is the alternative; W07 owns the call site, we expose the
helper.

## 9. AppState slices owned

```rust
SessionMap(BTreeMap<SessionId, SessionMeta>);
SessionMeta { id, title, message_count, updated_at,
              last_seen_cursor: Option<UiCursor>,
              last_message_seq: Option<u64> /* since_seq watermark */ }
HashMap<ToolCallId, ToolCall { id, name, status, … }>
HashMap<TaskId, Task { id, lifecycle, runtime, tail: TailRing,
                       next_cursor: OutputCursor,
                       output_files: Vec<FileHandle> }>
HashMap<FileHandle, FileMeta { handle, mime, size, original_filename,
                               session, kind }>
pending_uploads: Vec<FileHandle>
```

Selectors: `sessions_for_sidebar`, `is_session_active`, `dock_rows`,
`viewer_for`. Pure data — reduced from W01's typed events plus local actions
(`SessionDeleteConfirm`, `UploadStaged`, `ContentFilterChanged`).

## 10. Deliverables

**M1.**

1. `octos-app-transport::rest::sessions` — `list`, `messages(id, since_seq)`, `delete`; mock-tested.
2. `octos-app-store::sessions::reducer` over `Hydrated`, `Removed`, `MessageBatch`, `TurnStarted`, `TurnCompleted`.
3. `app/app/sessions.rs::SessionListWidget` — new-chat pill, dot, two-step delete; mounted in W02's sidebar slot.
4. Streaming-indicator selector + cold-open flow (no cache → fetch + populate W03; cached → `since_seq` top-up).

**M2.**

5. `octos-app-store::tasks::reducer` for all six `tool/*` / `task/*` kinds; correlation by `ToolCallId` / `TaskId`.
6. `app/app/task_dock.rs::TaskDockWidget` — collapsible rows; expand-to-tail with `task/output/read` pagination via a `call_task_output_read` helper on W01's transport.
7. `octos-app-transport::rest::{files, upload, content}` — download/query, multipart upload, content `fetch`/`delete`/`bulk_delete` and thumbnail/body URLs.
8. `app/app/files.rs` — image / audio / video (`robius_open` fallback) / markdown viewers.
9. `app/app/content_browser.rs` — `PortalList` grid, filter bar, multi-select, delete.
10. Composer attach hook: pending-handle chips cleared on `turn/start`.

## 11. Tests & verification

- **Pure-store** in `octos-app-store/tests/`: each reducer with golden notification streams.
- **REST mocks via `wiremock`** in `crates/octos-app-transport/tests/`. Cover the `handlers.rs:497-510` merge in `/api/sessions`; `since_seq` forwarding in messages; `DELETE` 204; `/api/files/{handle}` image + markdown bytes; `/api/upload` multipart; `/api/my/content` gallery shape.
- **Task correlation**: synthetic streams interleaving two `task_id`s — only the right `tail` grows. Out-of-order `tool/progress` after `tool/completed` warns, no panic. `task/output/delta` for unknown `task_id` creates a skeleton, awaits `task/updated`.
- **Viewer smoke**: snapshot `ImageAlbumViewer`, `MarkdownViewer` against `tests/fixtures/files/`. Audio/video: "loads URL, mounts, ESC closes".
- **Content browser**: filter combos; bulk-delete optimistic state.
- **Drift CI** (per `02-API-DRIFT.md`): probe `/api/version` + `/api/sessions`, `/api/files/list`, `/api/my/content?limit=1`; shape mismatch → amber build.

## 12. Exit criteria

**M1.** User logs in (W08), opens Chat; sidebar populates within 500 ms p95
of `/api/sessions`. Click swaps the thread; cold history via `since_seq=0`;
reopen fetches the diff via cached `last_message_seq`. Two-step delete works;
failure restores with a toast. Streaming dot lights within 100 ms of the
first `tool/started`/`message/delta`, clears on `turn/completed`, greys
during reconnect. Reducer + transport mocks green; one manual smoke against a
local server.

**M2.** Task dock shows a row per active tool/task within 100 ms; updates on
every `tool/progress` + `task/updated`; expand-to-tail paginates
`task/output/read` and stops at `complete: true`. All four viewers render
fixtures plus a real artifact. Multipart upload attaches a handle; the next
turn includes it in `turn/start`; file fetches back via `/api/files/{handle}`.
Content browser lists, filters, sorts, paginates, single + bulk delete. 30 s
offline drop preserves dock state and resumes via cursor replay (shared with
W01).

## 13. Risks

- **File-handle encoding.** Tmp upload handles had a past decoding bug. Treat opaque; URL-encode once at request boundary; fixture pair (signed handle ↔ URL) in tests.
- **`/api/sessions` merge.** Standalone wins on duplicate id; gateway copy may carry a higher `message_count`. Accept the merged number; re-fetch on session open if stale.
- **Orphaned tasks on reconnect.** Terminal `task/updated` may miss the ledger pre-drop. If running-ish > 5 min on the current session, fall back to `task/output/read` and transition locally on `complete: true`.
- **Bulk-delete race.** Stale `total` after cross-page delete. Re-issue query post-delete (200 ms debounce).
- **Video viewer.** No native H.264 in Makepad. Ship a poster + `robius_open` in M2; revisit with `cef` in M3.
- **`task/output/read` shape.** 13 fields incl. `complete`, `truncated`, `live_tail_supported` (`octos-core/src/ui_protocol.rs:728-749`). M9.4 signed off; flag amber if a probe shows extras.

## 14. Open questions

1. **Session titles.** `/api/sessions` returns no title; web infers from the first user message. Eager 1–2-message hydrate or a server-side title field? Ask via `06-WORKSTREAMS.md` Coordination.
2. **Topic vs. session.** Handler exposes `?topic=`; bus stores topics in session keys (`octos-bus/session.rs:565`). M1 ignores; confirm for M2 or add a topic switcher.
3. **Dot on reconnect.** Lit from cursor reply, or only after the first replayed `task/updated`? Default: lit if any task with `lifecycle != completed` survives replay.
4. **Video viewer.** `robius_open` vs. native decoder. Likely `robius_open` for M2.
5. **Audio recording.** Web has `audio_upload_mode` (`octos-web/src/api/chat.ts:30`); record-from-mic not in M1/M2 scope.
6. **Content browser default scope.** "All" with a "limit to current session" checkbox, or scope-by-default? Quick UX call.
