# Running octos-app — smoke-test runbook

App binary: `/Users/yuechen/home/octos-app/target/debug/octos-app`
(`cargo build -p octos-app`).

## 1. Quick start (dev token bypass — recommended)

`OCTOS_APP_TOKEN` skips `LoginScreen` entirely
(`app/src/main.rs:2126`, `crates/octos-app-store/src/keychain.rs:14`).
A live e2e server already runs on `:55928` with token
`<OCTOS_APP_TOKEN>`.

```bash
mkdir -p ~/.config/octos-app
cat > ~/.config/octos-app/server.json <<'JSON'
{ "server_url": "http://127.0.0.1:55928", "profile_id": "coding" }
JSON

OCTOS_APP_TOKEN=<OCTOS_APP_TOKEN> \
    /Users/yuechen/home/octos-app/target/debug/octos-app
```

App boots straight to home — server URL + profile from `server.json`,
bearer from env (no keychain prompt).

## 2. Local dev — start your own Octos server

Recipe lifted from `octos/scripts/compare-tui-coding-ux-tmux.sh`
(the canonical `octos serve` invocation):

```bash
PORT=58080; TOKEN=octos-app-dev-$RANDOM
DATA=/tmp/octos-app-data; WORK=/tmp/octos-app-cwd
mkdir -p "$DATA" "$WORK"
export DEEPSEEK_API_KEY=...     # or ANTHROPIC_API_KEY / OPENAI_API_KEY

/Users/yuechen/home/octos/target/release/octos serve \
    --host 127.0.0.1 --port "$PORT" --cwd "$WORK" --data-dir "$DATA" \
    --provider deepseek --model deepseek-v4-pro --auth-token "$TOKEN" \
    2>&1 | tee /tmp/octos-serve.log
```

Notes: binds 127.0.0.1 (`octos/CLAUDE.md:147`); installer-service default is
:8080 but on this machine `ominix-ap` holds :8080 — use 58080+.

## 3. First-run dialog (LoginScreen Step 1)

Without the env-var bypass, the three-step LoginScreen runs
(`app/src/app/login.rs`). Step 1:

- **Server URL** — e.g. `http://127.0.0.1:58080`.
- **Profile ID** — e.g. `coding` (must match a real profile).

`Continue` writes `~/.config/octos-app/server.json` and shows the email step.
Steps 2/3 only complete on a server with SMTP + a registered user. The
currently-running e2e/dev servers have `email_login_enabled: false`
(`/api/auth/status`) — OTP cannot finish there. Use §1 for those.

## 4. Smoke checklist (after launch)

1. Window opens to home page (LoginScreen hidden) when `OCTOS_APP_TOKEN` is set.
2. Sessions panel hydrates without error (may render empty).
3. Pick or create a session; composer focuses.
4. Type a prompt, send; assistant reply *streams* (tokens appear live).
5. Tool/diff blocks render inline if the model invokes a tool.
6. Switch to another session and back — UI swaps cleanly.
7. `Logout` clears keychain and returns to LoginScreen Step 2.
8. Quit + relaunch — skips Login, lands on home.

## 5. Known issues blocking live testing

- **`/api/version` shape drift.** Server returns
  `{service, version: "0.1.1+…", build_date, tunnel_domain}`
  (`octos/crates/octos-cli/src/api/handlers.rs:2323`); app expects
  `{version: UiProtocolVersion, capabilities: UiProtocolCapabilities}`
  (`octos-app/crates/octos-app-transport/src/rest/mod.rs:94`). Pre-WS probe
  will deserialize-fail. Tracked in `02-API-DRIFT.md:34`.
- **OTP path unusable on dev/e2e servers.** No SMTP, no users → `send-code`
  always returns `ok:true` (anti-enumeration) but `verify` rejects.
- **`/api/sessions` per-profile** wants a real `X-Profile-Id`; bare list is
  `[]`, unknown profile is `503 Sessions not available`.
- **Cloud `cloud@<CLOUD_HOST>` not reachable on :80/:443/:8080.**
  Behind frps + Caddy on a tenant hostname; TODO ask user for
  `<tenant>.octos-cloud.org` URL and per-tenant token, then reuse §1.

## 6. Probe cheatsheet

```bash
BASE=http://127.0.0.1:55928
TOKEN=<OCTOS_APP_TOKEN>
curl -s "$BASE/api/version"                                # shape check
curl -s "$BASE/api/auth/status"                            # admin_token_login_enabled?
curl -s -H "Authorization: Bearer $TOKEN" "$BASE/api/sessions"
curl -sI "$BASE/admin/" | head -1                          # SPA loads
```

Three green probes ⇒ §1 quick start should land on home; a red probe
pinpoints which step to fix first.

## Smoke 2026-04-28

End-to-end protocol smoke against the e2e server at `http://127.0.0.1:56831`,
profile `admin`, driven through `octos-app-transport` only (no UI layer).
Test: `crates/octos-app-transport/tests/live_smoke.rs` (`#[ignore]`d).
Driver: `scripts/smoke-live.sh`.

Invocation:

```bash
OCTOS_LIVE_TOKEN='…' \
  cargo test -p octos-app-transport --test live_smoke -- \
    --ignored --nocapture --test-threads=1
```

Result: **PASS** (one round, no fixes needed).

Timings & wire trace:

| Phase | Observed |
|---|---|
| Idle → Dialing → Handshaking → Live | ~3.5 ms |
| `session/open` reply (with caps) | arrived before `Live` transition |
| Negotiated capabilities | `typed_approvals=false`, `pane_snapshots=false` |
| `turn/start` reply (`accepted=true`) | first frame after the `turn/started` notification |
| Stream tokens to `turn/completed` | 2.479 s end-to-end |
| `MessageDelta` count | 2 |
| Concatenated streamed text | `pong` (4 bytes) |

Notable contract observations confirmed by the trace:

- WS upgrade lands cleanly with `Authorization: Bearer …` + `X-Profile-Id: admin`
  headers our transport sets in `crates/octos-app-transport/src/ws/mod.rs` (the
  `build_request` helper).
- Capability advertisement on this server is empty for both v1 features —
  expected on a stripped e2e build; clients should treat unknown/missing flags
  as "off" and degrade gracefully (`03-PROTOCOL-CONTRACT.md` § Capability
  negotiation).
- A `session/open` durable notification arrives *after* the `RpcResult`,
  presumably the replay baseline payload — our transport routes it correctly
  via `DurableNotification` and does not double-fire `Live`.
- `turn/started` lands as a durable notification before the first
  `message/delta` ephemeral, matching the spec ordering. `turn/completed`
  closes the stream.
- No `RpcError` / `turn/error` observed; `Disconnect` cleanly tore down the
  socket.
