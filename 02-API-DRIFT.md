# 02 — API Drift Audit

Source of truth for endpoint contracts is `~/home/octos/crates/octos-cli/src/api/router.rs` and the
handler files it routes to. The published doc at `~/home/octos/api/OCTOS_WEB_REST_API.md` is *mostly*
in sync but its self-claimed "verified 2026-04-28" timestamp predates real handler edits on
2026-04-27. Treat the doc as a friendly summary, not a contract.

## Trust score: MEDIUM

Endpoints we plan to consume (chat send, sessions list, history hydrate, file fetch, auth) are in
sync with handler code; admin and UI-protocol surfaces have post-doc edits and need to be tested
against the live server before locking. There are a small number of cosmetic / parameter-name
drifts that don't change behaviour.

## What we will consume — locked or watch-listed

These are the endpoints `octos-app` depends on. "Locked" = match handler today, no recent churn;
"Watch" = match handler today, but recent churn; we'll capability-detect or version-check before
trusting in production.

| Surface | Endpoint | Purpose | Status | Handler |
|---|---|---|---|---|
| Auth | `POST /api/auth/send-code` | OTP request | Locked | `crates/octos-cli/src/api/auth_handlers.rs` |
| Auth | `POST /api/auth/verify` | Exchange OTP for bearer token | Watch | same — last edit 2026-04-24 |
| Auth | `GET /api/auth/status` | Token validity probe | Watch | same |
| Sessions | `GET /api/sessions` | List for sidebar hydrate | Locked | `crates/octos-cli/src/api/handlers.rs` |
| Sessions | `GET /api/sessions/{id}/messages` | History hydrate | Locked | same |
| Sessions | `DELETE /api/sessions/{id}` | Remove | Locked | same |
| Files | `GET /api/files/{handle}` | Inline image / pdf / md | Locked | `handlers.rs` |
| Upload | `POST /api/upload` | Multipart upload, returns handle | Locked | `handlers.rs` |
| Content | `GET /api/my/content` | Saved-content gallery (Studio outputs) | Locked | `handlers.rs` |
| Chat (legacy) | `POST /api/chat?stream=true` | SSE chat (kept as fallback only) | Locked | `handlers.rs` |
| UI Protocol | `GET /api/ui-protocol/ws` | The WebSocket we live on | **Watch — heavy churn** | `crates/octos-cli/src/api/ui_protocol.rs` |
| Status | `GET /api/version`, `GET /api/status` | Health / version probe | Locked | `router.rs` |

The UI Protocol endpoint is the one to monitor: its handler had material edits late on 2026-04-27,
post-doc-verification timestamp.

## Drift list — non-blocking

Items that exist but won't affect us:

- **Path parameter rename (cosmetic).** Doc shows `{session_id}`; router uses `{id}`. Functionally
  identical for axum routing. We use the path verbatim from the handler, so this doesn't matter.
- **WebSocket `/api/ws`** is registered (`router.rs:70`) and the doc mentions it as "legacy". We
  ignore it — we use `/api/ui-protocol/ws`.
- **`POST /api/admin/shell`** — feature-flagged with `allow_admin_shell=true`, intentionally not
  in the public doc. Out of scope (admin lives in web).
- **Profile scoping**: doc says it works via `X-Profile-Id` header from loopback, or via subdomain.
  Confirmed in `router.rs:537–566` (`extract_token` → `resolve_identity`). We send the header.

## Drift list — needs verification before we lock

Test before depending on:

- **`approval.typed.v1` capability flag.** Doc declares it (UPCR-2026-001). Handler at
  `ui_protocol.rs:~138` advertises it but the typed payload shape (`approval_kind`, `risk`,
  `typed_details`, `render_hints`) had recent edits. **W05 owns running a live capability probe
  before the approval card UI is committed.**
- **`pane.snapshots.v1` capability.** Same story (UPCR-2026-002). Diff preview / workspace
  snapshot endpoints are still settling. **W05 owns probing.**
- **`turn/interrupt` idempotency** on already-completed turns. Spec says idempotent; handler
  behaviour late-edited. **W01 owns a contract test.**
- **`UiCursor` rejection rules** (stale vs. future). `validate_session_scope` enforces profile
  match, but cursor rejection paths recently touched. **W01 owns a contract test.**

## Endpoints in the doc we will *not* consume

Listed for completeness — none of these are wired into M1–M3. Anything from `admin.rs` is a hard
"no" for octos-app (charter: admin stays in web).

- `POST /api/admin/profiles` and 50+ siblings (profile CRUD, gateway lifecycle, model catalog,
  channel admin, sub-accounts, metrics).
- `GET /api/register/setup-script/{id}/{auth_token}` — setup-script onboarding flow.
- Channel-specific webhook endpoints (Slack, WhatsApp, Telegram) — server consumes these from the
  outside, the client never calls them.

## What we ask the server team for

In priority order, these would simplify W01 and unblock M2:

1. **A pinnable revision of `octos-core`** (git tag like `octos-core-v0.9.x`) so we can lock the
   protocol types.
2. **Reconnect + cursor contract tests** in `octos-cli` we can run against a local server. (M9.6
   has the in-memory ledger; we want a black-box test to ride along.)
3. **Capability probe response published** so we can negotiate `approval.typed.v1` /
   `pane.snapshots.v1` without round-tripping each capability.
4. **A `/api/version` field** that pins the protocol semver, separate from the binary version.

These are tracked as open asks in `06-WORKSTREAMS.md` § Coordination.

## How we keep this doc honest

- Every endpoint we add to a workstream cites the handler file and line, not the doc.
- A CI check (added in W10) hits the live server's `/api/version` and a small set of probe
  endpoints; failure marks the build amber, not red.
- When we depend on a "Watch" endpoint, the workstream doc names a contract test that pins it.

If the server team revs the protocol semver, we bump `octos-core` and re-run the audit. The
doc itself (`OCTOS_WEB_REST_API.md`) is read once for orientation and then ignored.
