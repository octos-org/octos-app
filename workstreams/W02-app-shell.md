# W02 — App shell & navigation

## Mission

Own everything the user sees that *isn't* a feature surface: transparent macOS
window, drag strip, `app_shell` / `sidebar` / `main_area` GlassPanel layout, top bar
(profile picker + glass slider + connection indicator), status bar, the navigation
state machine driving a `PageFlip` over Chat / Coding / Studio / Slides / Sites, and
a small toast queue. By M1 the shell renders, the user can move/resize, switch
pages, see connection state, and watch toasts. Inner slots are stubs filled by W03+.

## Header

| | |
|---|---|
| Lane | A — Spine |
| Depends on | W01 (needs `Connection`, latency, cursor seq) |
| Lifts from | `aichat:618–1142` (live-DSL shell) and `aichat:2413–2655` (`MatchEvent` / `AppMain`) |
| Output | `octos-app/app/src/app/shell.rs` + a `live_design!` block in `octos-app/app/src/main.rs` |
| Milestone | M1 |

## Scope

**In:** borderless transparent macOS window, drag strip, edge-resize, custom resize
grip (`aichat:618–633, 1127–1136, 1179–1189`); three GlassPanels (`:635–865`); sidebar
nav (Home / Chat / Coding / Studio / Slides / Sites / Content / Settings); top bar
(page title, profile picker, glass slider, connection dot); status label;
`CurrentScreen` reducer + `PageFlip` over the five feature pages; toast queue.

**Out:** chat / composer / streaming (W03); session list contents (W04); approvals +
diff (W05); login + profile data (W08, W02 ships a disabled stub); agent dispatch
(W01 + W03); light theme.

## Live-DSL structure plan

`05-AICHAT-REUSE-MAP.md` has the full table; W02's deltas:

**Verbatim into `live_design!`:** window + transparent pass + body Overlay
(`aichat:618–634`); `app_shell` (`:635–656`); `sidebar` + `sidebar_header`
(`:658–712`); `nav_*` ButtonFlat styles (`:714–836`, actions repointed); hairline
`SolidView` (`:839–843`); `main_area` (`:845–865`); `top_bar` outer View, page title,
glass-slider ToolbarGlass (`:867–880, 940–958`); `status_label` (`:1116–1123`);
`resize_grip` (`:1127–1136`).

**Adapted:** `nav_*` actions dispatch `NavigateTo`. `backend_dropdown` (`:881–938`)
becomes `profile_dropdown` — same DropDown / PopupMenuFlat; labels runtime-loaded by
W08, W02 ships a disabled stub. `chat_shell` Overlay (`:961–987`) is replaced by
`main_pageflip` over stub pages `home_page`, `chat_page`, `coding_page`,
`studio_page`, `slides_page`, `sites_page`. The empty-state Labels (`:966–984`) lift
as the body of `home_page` with text per `04-IA-AND-NAVIGATION.md`.

**Dropped:** `composer` + composer actions (`:989–1114`, W03 owns); `thinking_toggle`
(`:1067–1074`, server-side).

**New:** `connection_dot` Vector in `top_bar`, tinted by `Connection`; `toast_layer`
Overlay bottom-right of `main_area`, stacked `RubberView`s;
`sidebar_context_pageflip` below the nav (session list in Chat, project list in
Studio/Slides/Sites). W02 owns the *slot*; W03/W04/W07 own contents.

**Verbatim Rust into `shell.rs`:** `should_start_window_drag` (`:1179–1189`);
`glass_opacity_values` (`:1156–1177`); the Cmd/Ctrl-link
`MarkdownAction::LinkNavigated` block (`:2434–2469`).

## Navigation state machine

`CurrentScreen` is exactly the enum from `04-IA-AND-NAVIGATION.md`: `Login`, `Home`,
`Chat { session }`, `Coding`, `Studio { project }`, `Slides { project }`, `Sites
{ project }`.

W02 owns the reducer (`octos-app-store::navigation`, Makepad-free, unit-testable) and
the discriminant→page-id selector. Page indices fixed: `Login=0, Home=1, Chat=2,
Coding=3, Studio=4, Slides=5, Sites=6`. `App::redraw_navigation` writes `active_page`
on `main_pageflip`.

Events: `NavigateTo(CurrentScreen)` from sidebar clicks (dispatch shape mirrors
`aichat:2471–2499`); `OpenSession(SessionId)` from W04 → `Chat { session: Some(id) }`;
`OpenProject(Producer, ProjectId)` from W07; `Logout` from W08 → `Login`.

`PageFlip` keys off `current.discriminant()`; variant payloads (session id, project
id) are read by the page from `AppState.current`. Per-screen ephemeral state (scroll,
focus) lives in `AppState.ephemeral.per_screen[discriminant]`; W02 owns the slot,
features own the contents.

## Top bar contents

Left-to-right inside `top_bar`:

1. **Page title `Label`** — mirrors `CurrentScreen`. Style as `aichat:873–877`.
2. **Profile picker `DropDown`** — visual clone of `backend_dropdown`
   (`aichat:881–938`). W02 ships a disabled stub (`labels=["(no profile)"]`); W08
   calls `set_labels(profiles)` + `set_selected_item(idx)`; user selection fires
   `SelectProfile(ProfileId)`, dispatch shape mirrors `aichat:2491–2499`.
3. **Glass-opacity slider** — verbatim from `aichat:940–958`. Value runs through
   `glass_opacity_values` (`:1167–1177`); `App::apply_glass_opacity` writes per-layer
   alphas to `app_shell` / `sidebar` / `main_area` (and `composer` once W03 lands).
   Initial `DEFAULT_GLASS_OPACITY = 0.90` (`:1152`); the "90%" label updates on every
   `slided` action.
4. **Connection indicator** — 12×12 circle Vector, tinted by `AppState.connection`:
   `Connected{latency_ms}` → green `#x4FCC85`; `Reconnecting{attempt}` → amber
   `#xEABF55` pulsing alpha 0.5–1.0; `Disconnected{error}` → red `#xE5604F`;
   `Initializing` → cream-dim `#xCDBF9F`. Mapping in `shell.rs::connection_color`.
5. **Status label format** (`update_status` mirrors `aichat:2525, 2569–2574`):
   `"Connected · {latency_ms}ms · cursor {cursor_seq}"`, `"Reconnecting (attempt
   {n}, next in {s}s)"`, `"Disconnected: {error}"`, `"Initializing…"`.

## Toast queue design

Transient banners at bottom-right of `main_area`. DSL: `toast_layer` Overlay. Rust:
`AppState.ephemeral.toasts: VecDeque<Toast>`.

- **Queue depth:** 3 visible; older ones fall off the top silently; new ones append.
- **Animation:** each toast inside `RubberView { smoothing: 0.3 }` (lifted from
  `aichat:480–483`); height interpolation gives a smooth slide.
- **Auto-dismiss:** error 8s; info 4s; reconnect-warning sticky during
  `Reconnecting`; reconnect-success 3s.
- **Toast types:** `error` (red) for `AgentEvent::PromptError` / `SessionError` /
  REST 5xx; `reconnect-warning` (amber, sticky) during `Reconnecting`;
  `reconnect-success` (green, 3s) on Reconnecting → Connected; `info` (cream) for
  misc state changes.
- **Dispatch:** modules emit `Toast::push(kind, message)` via `cx.action`; the store
  reducer updates `ephemeral.toasts`; the shell renders the bottom 3.

## Deliverables

1. `octos-app/app/src/main.rs`: `app_main!(App)`, the `live_design!{...}` block
   above, the `script_mod!` block (verbatim from `aichat:2538–2543` minus
   `diagram_kit`; W03 adds it back).
2. `octos-app/app/src/app/shell.rs`: `App` struct (`#[deref] ui: WidgetRef`,
   `AppState`, agent slot); `handle_actions` covers nav clicks, profile dropdown,
   glass slider, link routing.
3. `octos-app-store::navigation`: `CurrentScreen`, `NavEvent`, `apply`, transition
   unit tests.
4. `octos-app-store::toasts`: `Toast`, `ToastKind`, `push`, `tick(now)`,
   queue-depth + sticky-behaviour unit tests.
5. `shell.rs::should_start_window_drag` — verbatim `aichat:1179–1189`.
6. `shell.rs::glass_opacity_values` + `apply_glass_opacity` — verbatim `:1156–1177`.
7. `shell.rs::connection_color` + `update_status` — new, mirroring `aichat:2525,
   2569–2598`.
8. `App::handle_event` covers `WindowDragQuery` exactly as `aichat:2552–2559`,
   forwards to `match_event`, then to the agent loop slot.
9. Sidebar "Settings" → `robius_open::Uri::new(&web_dashboard_url).open()`, call
   shape from `aichat:2464`.

## Tests & verification

Smoke checklist (shell-visible subset of `05-AICHAT-REUSE-MAP.md`'s post-lift list):

- [ ] Drag strip moves the window; edges still resize.
- [ ] Glass slider re-tints panels in documented order; "90%" updates live.
- [ ] Cmd/Ctrl-click on a markdown link opens the OS browser (stub Markdown in
      `home_page`).
- [ ] Sidebar nav swaps `main_area`; `PageFlip.active_page == discriminant()`.
- [ ] Connection dot tints reflect the four `Connection` states (dev menu stubs).
- [ ] Toast queue caps at 3; the 4th drops the oldest.
- [ ] Reconnect-warning sticky during `Reconnecting`; replaced by reconnect-success
      on transition; auto-dismisses 3s later.
- [ ] Status label format matches the four documented strings.

Screenshot fixtures (W10 harness): `shell-empty.png`, `shell-connected.png`,
`shell-reconnecting.png`, `shell-chat-stub.png`.

Unit tests in `octos-app-store::navigation::tests` and `::toasts::tests` — both
Makepad-free, plain `cargo test`.

## Exit criteria

1. `cargo run` opens a transparent borderless window matching aichat's four-panel
   style.
2. Drag, resize, drag-strip, glass slider, sidebar nav all work.
3. Each feature page renders its stub; switching is instant; the selected sidebar
   button is visually marked.
4. Connection dot and status label reflect a fake-driven `Connection` in dev mode.
5. Toast queue receives `Toast::push`, animates in/out, caps at 3.
6. `cargo test -p octos-app-store` passes for navigation + toasts.
7. Four screenshot fixtures committed; W10 reproduces them.

## Risks

| Risk | Mitigation |
|---|---|
| `WindowDragQuery` constants assume composer at the bottom; strip math may misfire without it | Derive `RIGHT_TOOLBAR_WIDTH` from `top_bar`'s measured width, not hardcoded 260 |
| `glass_opacity_values` returns four alphas; not always a composer | `apply_glass_opacity` tolerates `None` per panel |
| `PageFlip` redraw may flash on heavy DSL trees | Pages are Views `visible: false` unless active |
| RubberView `smoothing: 0.3` may not stack cleanly | Start with aichat default; revisit if diffs show jitter |
| Profile picker stub leaks in M1 if W08 slips | Hide the dropdown ToolbarGlass until W08 is ready |
| Toast layer in slider's alpha map could regress aichat invariants | Toast layer is *not* a GlassPanel; alpha independent of slider |

## Open questions

1. **Sidebar collapse?** Not in W02. Revisit at M2 if width complaints appear.
2. **"+ New chat" placement?** Decision for W02: `nav_new` fires `NavigateTo(Home)
   + Home's New Chat`; W04 adds a per-session-list "+" later.
3. **Connection dot tooltip.** Hover the full status string? Deferred to M2.
4. **Drag-and-drop on shell.** Decision: ignore in M1; revisit at M3 with files.
5. **Sticky reconnect toast vs. status label redundancy.** Possibly drop the sticky
   toast in M2 if feedback agrees.
