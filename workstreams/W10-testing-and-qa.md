# W10 — Testing & QA strategy

## 1. Mission

Own the test pyramid: unit on `octos-app-store` (every commit); contract on
`octos-app-transport` against a mock JSON-RPC server with fault injection (every PR);
integration in `app/tests/` with `wiremock` + `tokio-tungstenite` (nightly); golden
screenshots on a macOS runner. M1 exit: unit + contract green. M2 exit: integration +
golden green, reconnect contract test the canonical wire-health signal.

## 2. Header

| Lane | Depends on | Lifts from | Milestones |
|---|---|---|---|
| Q | W01 transport API, W04 store API | `aichat:2658–2889` (reducer / safety / glass tests, lifted adapted) | M1+ continuous |

## 3. Scope

In: unit on `octos-app-store`; contract on `octos-app-transport` with fault injection;
integration in `app/tests/`; golden screenshots via Makepad screenshot + `insta`; CI;
fixtures; coverage via `cargo-llvm-cov`; fuzz entry points; bug-triage rubric.

Out: load testing of the Octos server (in its own CI); manual UAT; security pentesting
(`cargo audit` is in scope, adversarial review is not).

## 4. Test pyramid

| Layer | Library | Approx | Wallclock | Cadence |
|---|---|---|---|---|
| Unit | `cargo test` (sync) | 200–400 | < 5 s | every commit |
| Contract | `cargo test` + `tokio-tungstenite` mock + `tower::Layer` faults | 30–60 | < 60 s | every PR |
| Integration | `cargo test` + `wiremock` + mock WS | 10–20 | < 5 min | smoke per PR, full nightly |
| Golden | Makepad screenshot + `insta` binary snapshot | 10–20 | < 5 min macOS | nightly + on-demand |

Anything that can be a unit test is a unit test; the store's Makepad-free design enforces it.

## 5. Unit test layer (octos-app-store)

- **Reducer transitions per event variant.** One test per `Event` in
  `UI_PROTOCOL_NOTIFICATION_METHODS` (`turn/{started,completed,error}`, `message/delta`,
  `tool/{started,progress,completed}`, `task/updated`, `task/output/delta`,
  `approval/requested`, `progress/updated`); plus connection transitions, capability
  outcomes, ephemeral cleanup on `turn/completed`.
- **Selectors.** Each (`current_session_messages`, `streaming_text_for_turn`,
  `pending_approvals_for_session`, `task_dock_rows`) — happy-path + edge case.
- **Fixtures.** `fixtures::sample_app_state(name)` returns `empty`, `mid_stream`,
  `with_pending_approval`, `with_task_in_progress`, `post_reconnect_replay`, `cjk_streaming`
  under `crates/octos-app-store/tests/fixtures/`.
- **Net-new vs aichat:** cursor monotonicity (reject events with seq ≤ last applied),
  ephemeral cleanup, capability downgrade.

Dev loop: `cargo nextest run -p octos-app-store`.

## 6. Contract test layer (octos-app-transport)

Mock server: `tests/support/mock_ws_server.rs` uses `tokio-tungstenite::accept_async` with a
small DSL (`expect_session_open().reply_opened(…)`, `send_notification(method, payload)`).
~30 cases in M1, ~60 by M2: turn happy path (event order, durability, cursor); interrupt
(`turn/error { code: "interrupted" }`, ephemeral cleared); cursor rejection (stale `after`
→ `INVALID_PARAMS` → drop and REST hydrate via `wiremock`); replay correctness (drop after
seq 5, replay 6–9 strictly increasing); capability downgrade (no `pane_snapshots=1` on
upgrade if server omits it); `turn/interrupt` idempotency on completed turn.

Fault injection: a `FaultLayer` (`tower::Layer`-shaped) drops the socket at chosen byte
offsets, delays tokens ("slow tokens" verifies streaming UI does not deadlock), or mangles
a frame.

## 7. Integration test layer (`app/tests/`)

Each test starts a `wiremock::MockServer` for REST (`/api/sessions`,
`/api/sessions/{id}/messages`, `/api/files/{handle}`, `/api/version`) plus the
`MockWsServer` from § 6 via `tests/support/`. The app boots backend-only behind
`app/Cargo.toml [features] test-headless = []`: `main` reroutes to
`App::run_backend_only`, wiring transport → store and exposing `AppState` via a public test
handle. Headless Makepad is *not* required (see § 16); this tier proves backend behaviour,
not draw correctness.

Cases: cold-start → list → open → hydrate → send turn → cancel; reconnect with valid cursor
(`Live → Reconnecting → Live`); stale cursor (one REST hydrate, cursor resets); approval
round-trip; file hydrate. Cap 5 min.

## 8. Golden screenshot tests

Makepad's screenshot path (`platform/src/cx.rs:158 ScreenshotRequest`,
`cx_shared.rs:149 take_studio_screenshot_request_ids`) is studio-driven today; non-studio
maturity is the risk (§ 16).

States: empty chat; hydrated history; mid-stream (fade-in paused); TaskDock with one
in-progress + one completed tool; approval card; sidebar collapsed vs expanded; glass
slider at min / default / max.

Pipeline: `cargo test -p octos-app --features test-golden` boots at 1440×900, drives a
deterministic event sequence, lets one redraw settle, feeds RGBA8 into
`insta::assert_binary_snapshot!` with tolerance (per-pixel L1 ≤ 4/255, fail if > 0.5 %).
macOS only (`macos-14`); Linux / Windows deferred. Fallback if pixel diffs flake:
`insta::assert_yaml_snapshot!` of the post-layout widget tree. Decide end of M1.

## 9. Adapted aichat tests (port `:2658–2889`)

**Keep verbatim** — safety scans + history injection (move to
`octos-app-store::safety::tests`, names unchanged): the eight `history_injection_*`,
`store_*`, and `outer_markdown_wrapper_*` tests at lines 2802–2888 (including the CJK
regression at 2833 and the unclosed-non-diagram-fence regression at 2843).

**Adapt** — UI invariants (under `app/src/app/shell.rs`, owned by W02; W10 runs them): the
three `aichat_glass_opacity_slider_contract`, `aichat_liquid_glass_shell_contract`, and
`aichat_drag_strip_preserves_resize_edges` (layer ordering `app < main < sidebar <
composer`, default in `0.82..0.87`).

**Drop** — `BackendType` replaced by profile listing (W08): the three `aichat_backend_type_*`
/ `aichat_create_claude_code_agent` / `aichat_defaults_to_moonshot_when_available` tests.
Replacement: `profile_listing_round_trips_active_profile` against a `wiremock` REST stub.
`non_splash_prompt_documents_*` move with the prompt itself to W03.

## 10. CI configuration

**Per-PR (mandatory; under 12 min on `ubuntu-latest`):** `cargo fmt --check`; `cargo clippy
--all-targets --workspace -- -D warnings`; `cargo test` on store, transport, and `app
--features test-headless --tests integration_smoke`; `cargo build -p octos-app`.

**Nightly:** `--tests integration_full`; golden (`--features test-golden` on `macos-14`);
`cargo audit`; `cargo deny check`; `cargo llvm-cov` → Codecov; `cargo +nightly fuzz run
notification_deser` and `fence_safety` for 600 s each.

**Release:** W09 owns the per-OS packager; W10's gate is full PR lane + golden green on the
release tag SHA (required check on `release-*`). Cache: one `actions/cache@v4` keyed on
`Cargo.lock`. Manual `workflow_dispatch` for macOS before merging shell / layout changes.

## 11. Test data & fixtures

Under `tests/fixtures/`, shared as a dev-dependency: `protocol_traces/{turn_happy,
turn_interrupt, reconnect_replay}.jsonl` (last with monotonic seq, drop-after-5);
`diffs/{single_file, multi_hunk}.patch` for W05; `mermaid/{flowchart, sequence}.md`;
`cjk/streaming_chunks.txt`; `math/inline_and_display.md`; `auth/profile_list.json` (replaces
BackendType); `approvals/typed_v1_payload.json`. The contract bootstrap deserializes each
`*.jsonl` line into its `UiNotification` variant from `octos-core` at suite startup;
mismatches fail with `fixture file:line` + the parse error.

## 12. Coverage tracking

`cargo-llvm-cov` (chosen over `tarpaulin` for accurate stable-toolchain coverage). Floors:
store ≥ 70 % line; transport ≥ 80 %; `app/` UI no floor. Per-PR advisory Codecov comment;
per-nightly enforced; two consecutive breaks files a P2.

## 13. Bug triage workflow

S0 (data loss / silent corruption / common-path crash) → block release; immediate hotfix.
S1 (M1/M2 happy-path regression with workaround) → block. S2 (edge case) → next release.
S3 (cosmetic) → follow-up. Tagged within 1 working day. W10 owner is gatekeeper;
disagreement escalates to architect. Flaky tests carry `#[ignore = "flaky-CI-only"]`;
font-drift golden re-baseline allowed once per quarter.

## 14. Deliverables

**M1:** (1) test directory skeleton across three crates; (2)
`tests/support/mock_ws_server.rs` harness; (3) eight aichat safety tests ported verbatim;
(4) three glass / drag tests adapted; (5) reducer unit tests; (6) selector tests + fixture
helper; (7) contract tests (happy turn, interrupt, stale cursor, capability downgrade); (8)
CI per-PR lane; (9) `cargo-llvm-cov` + Codecov; (10) fixtures schema-checked.

**M2:** (11) `--features test-headless` in `app/`; (12) integration tests via `wiremock` +
mock WS; (13) reconnect contract promoted into per-PR; (14) golden screenshot harness; (15)
fuzz `notification_deser` (bytes → `UiNotification::from_rpc_notification`) and
`fence_safety` (text → `assistant_message_is_safe_to_store`); (16) `TRIAGE.md`; (17)
coverage floors enforced.

## 15. Exit criteria

**M1:** store tests green (≥ 200, ≥ 70 % coverage); transport green (≥ 30 contract cases
covering happy turn, interrupt, cursor rejection, capability downgrade); eight ported aichat
safety tests verbatim; per-PR CI gating, mean wall < 12 min; coverage on every PR.

**M2:** integration green in nightly; reconnect contract green and gating PRs; golden green
on macOS (≥ 10 frames, < 0.5 % drift); fuzz ≥ 600 s nightly with no crashes for two
consecutive weeks; coverage floors enforced; `TRIAGE.md` committed.

## 16. Risks

- **Makepad headless rendering maturity.** The screenshot path (`platform/src/cx.rs:158`,
  `cx_shared.rs:149–164`) is studio-driven, not exercised from `cargo test` on GPU-less or
  virtualised displays. Risk: black frames, GPU timeouts on cloud macOS. Mitigations: small
  budget, retry-once, YAML widget-tree fallback. Decide end of M1.
- **CI macOS minute costs.** `macos-14` is 10× linux. Per-PR macOS not budgeted; nightly +
  manual `workflow_dispatch` only. W10 triggers macOS before merging shell / layout.
- **`octos-core` protocol churn.** Pin to a git tag in CI; fixture schema-check is the
  drift signal.
- **Mock WS divergence.** Weekly manual "live smoke" via W01's `cargo run --example smoke`
  against staging.
- **Flaky dual-mock integration.** Port-allocation races. Mitigation: ephemeral ports,
  readiness poll; quarantine twice-flaky tests to nightly with `flaky-CI-only`.

## 17. Open questions

1. Pixel-tolerance threshold (0.5 %) is an opening bid; macOS point updates may force
   widening. Set during M2.
2. Legacy REST chat path (W01's `--legacy-rest-chat`) in CI or manual-only? Default manual.
3. YAML widget-tree fallback as *primary* alongside pixel diffs? Robust to font drift,
   blind to colour bugs. Decide after M1.
4. "Delta firehose" test if Octos pushes client-side back-pressure on `message/delta`?
   Revisit at M2 start.
5. `proptest` for cursor-monotonicity beyond W01's 200 random drops? Defer until M2.
