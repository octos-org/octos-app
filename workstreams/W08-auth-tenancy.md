# W08 — Auth & Multi-Tenancy

## Mission

Make `octos-app` a signed-in client that talks to any Octos server and any profile the
user can reach, without leaking credentials and without going through a browser. Email
+ OTP login, token in the OS keychain, `X-Profile-Id` on every authed request, top-bar
profile picker, auto-logout on 401/403 with redirect preservation. The only place in
the app that knows about credentials.

## Header

| Field | Value |
|---|---|
| ID | W08 |
| Title | Auth & multi-tenancy |
| Lane | A — Spine |
| Depends on | W01 (transport — needs the request-header hook) |
| Lifts from | small port of `octos-web/src/api/client.ts` patterns |
| Milestone | M1 |
| Owner | TBD — Lane A agent #2, after W01 |

## Scope

**In.** LoginScreen; OS-keychain token; first-run "Server URL + Profile ID" dialog;
profile-picker dropdown; `X-Profile-Id` on every authed REST call and the WS handshake;
auto-logout on 401/403; `AppState.auth` slice.

**Out — admin (sidebar "Settings ⤴" deep-links to `https://<server>/admin/...` via
`robius_open`).** Profile CRUD, sub-account create/manage, password reset (no password —
OTP is the credential), allowlist/invite admin, `email_login_enabled` /
`allow_self_registration` toggles.

## LoginScreen flow

Four states: `Idle` (email + "Send code") → `SendingCode` (spinner) → `AwaitingCode`
(code field, "Verify", "Resend" with 60 s countdown, email read-only) → `Verifying`.
Wire (in `crates/octos-cli/src/api/auth_handlers.rs`):

- `POST /api/auth/send-code` (`auth_handlers.rs:389`). Server returns `ok: true` even on
  rate-limit / unknown-email to prevent enumeration; always advance to `AwaitingCode`.
- `POST /api/auth/verify` (`auth_handlers.rs:543`). On `ok && token`, persist and transition.
- `GET /api/auth/status` (`auth_handlers.rs:508`) once on boot — banner if
  `email_login_enabled == false`; detects `bootstrap_mode`.

**Errors.** `/send-code` network: stay on `Idle`. `/verify` network: stay on
`AwaitingCode`. `ok: false`: show server `message`. HTTP 503: banner.

**Redirect preservation.** Before transitioning to Login, navigator stashes the current
`CurrentScreen` into `AppState.auth.redirect_after_login`. After successful verify the
reducer dispatches `NavigateTo(redirect)` if set, else `NavigateTo(Home)`.

## Token storage

[`keyring`](https://crates.io/crates/keyring) wraps native backends (Apple Keychain via
`security-framework` on darwin; Secret Service on Linux; Credential Manager on Windows).
Service `io.ominix.octos-app`; account key `<server_host>::<profile_id>`
(multi-account-ready, §9); value `{ token, issued_at, server, profile_id }` JSON.

**Keychain unlock prompt (macOS).** First read may surface the OS modal. All keychain
I/O off-thread (`tokio::task::spawn_blocking`), staged via `Cx::post_action`. If denied,
route to Login with a "Keychain denied — tokens won't be remembered this session" banner.

**Headless / dev fallback.** `OCTOS_APP_TOKEN=<T>` (or `--token <T>`) skips keychain;
in-memory only. Never write plaintext tokens to disk.

## Profile resolution (three modes)

Wire: every authed request carries `X-Profile-Id: <id>`. Server resolution is in
`crates/octos-cli/src/api/router.rs:537–566` (`extract_token`, `resolve_identity`,
`AuthIdentity`). The loopback-only X-Profile-Id proxy-auth path (~`router.rs:619–693`)
serves self-hosters fronting through Caddy with per-subdomain routing — `octos-app`
doesn't rely on it; we always send a real bearer.

Client picks `profile_id` in priority order:

1. **Env override (dev).** `OCTOS_APP_PROFILE_ID=<id>` overrides everything; banner.
2. **Subdomain hint (octos-web only).** `octos-web` parses `*.octos.ominix.io` etc.
   (`octos-web/src/api/client.ts:4–17`). **`octos-app` has no host to parse** — this
   branch doesn't exist for us; documented so future readers don't add it.
3. **Header-based (the actual mode).** Top-bar dropdown selection, the single available
   profile, or the value typed into the first-run dialog. Stored in
   `AppState.auth.profile_id`; rides on every outbound request.

## Profile picker UI

Top-bar dropdown styled like `aichat`'s `backend_dropdown` — glass popover, profile name
plus a role / sub-account chip.

- **Ideal:** public profile-listing endpoint scoped to the user's token. **None exists
  as of 2026-04-28** (see `02-API-DRIFT.md` § "What we ask the server team for").
- **Fallback (M1):** after `/api/auth/verify`, call `GET /api/my/profile`
  (`auth_handlers.rs:834`); dropdown shows the single resolved profile, read-only.
- **Manual fallback:** "Switch profile…" modal takes a free-text Profile ID stored
  alongside the token. Unblocks multi-sub-account users until listing ships.

When listing lands, swap `octos-app-store::auth::list_profiles` — UI doesn't change.

## Auto-logout / token "refresh"

No refresh token. The bearer is opaque; the server validates on every request.

- REST **401 or 403**: clear the keychain entry for `<server>::<profile_id>`, drop
  `AppState.auth.token`, set `redirect_after_login = AppState.current`,
  `NavigateTo(Login)`. Toast "Session expired."
- WS close with auth-shaped close code (server to pin via `octos-core`
  `CloseReason`): same. Plain network drops keep reconnecting via W01.
- **No proactive `/api/auth/status` polling.** One implicit failed request is cheaper.

User back weeks later hits 401 on the first `GET /api/sessions`; redirect-aware logout
fires; they sign back in and land on the same session.

## Multi-account support

**M1: one account at a time.** Single keychain entry, single `AppState.auth`.

**M2+ (deferred).** The `<server>::<profile_id>` key already supports multiple entries;
missing pieces are an "Accounts" submenu, a per-account state slice (or
`active_account` discriminator), and per-account caches. Don't start in M1.

## AppState slice

```rust
pub struct Auth {
    pub token: Option<SecretToken>,           // newtype: redacted Debug/Display
    pub profile_id: Option<String>,           // sent as X-Profile-Id
    pub profile_meta: Option<ProfileMeta>,    // id, name, parent_id, is_sub_account
    pub redirect_after_login: Option<CurrentScreen>,
    pub server_auth_status: Option<AuthStatusResponse>,
    pub server_url: Url,
}
```

Selectors: `is_authed()`, `effective_profile_id()` (env override → state → None),
`auth_headers()` (`Authorization` + `X-Profile-Id`).

## Deliverables

1. `AppState.auth` slice + serde tests in `octos-app-store/src/auth.rs`.
2. Reducer — `Login`, `Logout`, `SendCode{Requested,Succeeded,Failed}`,
   `Verify{Requested,Succeeded,Failed}`, `ProfileSelected`, `RedirectCaptured`.
3. `octos-app-store::auth::keychain` — `keyring` wrapper, off-thread helpers, fallbacks,
   `<host>::<profile>` key.
4. Transport integration — `auth_headers()` on every REST call and the WS connect
   request (header *and* `?profile_id=` query — belt-and-braces for proxies dropping
   unknown headers).
5. LoginScreen (`app/app/login.rs`) — ports `aichat`'s `TextInput` + `PillButton`;
   four-state machine.
6. **First-run "Server URL + Profile ID" dialog** — populates `server_url` and
   (optionally) `profile_id`. Skipped with `--server` / `--profile`. Substitute for
   octos-web's subdomain inference (§14).
7. Profile-picker widget (`app/app/profile_picker.rs`) — keyboard-navigable, swappable
   source.
8. Auto-logout middleware — single point turning 401/403 into `Logout(Expired)`.
9. `Logout` wiring — clears keychain, drops `AppState.auth`, navigates to Login.

## Tests & verification

- **Mock auth server contract test** (`app/tests/auth_mock.rs`). `wiremock` serving
  canned `/api/auth/{send-code,verify,status}` and `/api/my/profile`; drive LoginScreen
  via store actions; assert `Auth.token` is populated and `X-Profile-Id` rides on a
  probe request.
- **Keychain integration smoke test** (`app/tests/keychain.rs`) gated on
  `target_os = "macos"` plus a `gnome-keyring-daemon` Linux variant. Write, read,
  delete. Skipped in CI without a session keychain (W10 owns runner config).
- **401/403 redirect contract** (store level). Dispatch `RestError(401)`; assert
  `CurrentScreen::Login` and `redirect_after_login == previous_screen`.
- **Env-var override.** `OCTOS_APP_PROFILE_ID=foo` ⇒ `effective_profile_id() ==
  Some("foo")`.
- **Log-redaction grep.** Trace logs on, grep for bearer prefix; fail if seen.

## Exit criteria

- New user, clean machine: launch → Server URL → email → OTP → code → `HomeScreen`
  with sessions hydrating.
- Quit / relaunch: arrives on Home directly.
- 401 mid-session: returns to Login; same session pre-selected for redirect.
- Logout: keychain clean; relaunch shows LoginScreen.
- Token never appears in any log, panic, debug dump, or error surface.

## Risks

| Risk | Mitigation |
|---|---|
| **No public profile-listing endpoint** (`02-API-DRIFT.md` § "What we ask the server team for") | `/api/my/profile` for single; manual "type a profile ID" for multi; swap on ship |
| **Subdomain detection on a packaged native app — no browser, no subdomain; we use server URL + profile_id explicitly** | First-run dialog asks for both. Copy: "If you sign in at *acme*.octos.ominix.io in a browser, your Profile ID is `acme`." `octos-web` skips this dialog since the host already encodes it |
| Keychain unlock prompt blocks startup | Off-thread I/O; UI in `Loading`; falls to LoginScreen on denial |
| Token leaks via `Debug` / panics | `SecretToken` redacting newtype; transport scrubs `Authorization` from trace logs |
| `/api/auth/verify` on Watch list | Contract test pins response shape |
| Proxies stripping WS headers | Header *and* `?profile_id=` query on the WS URL |
| Headless dev without keychain | `OCTOS_APP_TOKEN`; in-memory only |

## Open questions

1. **WS close-code for auth failure.** Protocol doesn't pin a code. Until confirmed,
   treat any close immediately followed by `GET /api/sessions` 401 as the signal.
   Tracked in `06-WORKSTREAMS.md` Coordination.
2. **Multiple servers in M1?** No multi-server UI; defer to M2. Swap via `--server`.
3. **`bootstrap_mode == true`.** No admin token configured. M1 refuses to log in and
   points at the `octos` CLI.
4. **Picker label, single-profile case.** Bare label or inert chevron? UX call at
   end of M0.
5. **Logout-reason copy.** "Session expired" vs "Server rejected token" — decide
   during LoginScreen review.
