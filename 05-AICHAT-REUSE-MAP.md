# 05 — aichat Reuse Map

What lifts from `~/home/aichat/examples/aichat/src/main.rs` into octos-app, with file:line citations
and adaptation notes. Read this once before W02/W03/W05 — it answers "do I build this or copy it?"
for nearly every chat-shaped widget.

The aichat example is 2,890 lines, ~80% live-DSL UI definitions and ~20% Rust glue. We lift
roughly 60–70% of the DSL verbatim into `app/src/main.rs`, ~30% of the Rust glue (Agent dispatch,
streaming pipeline, message safety scans), and ignore the parts that are aichat-specific
(BackendType enum, multi-LLM-direct dispatch, glass-opacity tests).

## Lift table

| octos-app surface | aichat source | Lift mode | Notes |
|---|---|---|---|
| Window + transparent macOS chrome | `:618–633` | Verbatim | Keep `borderless`, `inner_size: 900,700`, transparent pass |
| Drag strip behaviour | `:1179, 2552–2559` (`should_start_window_drag`) | Verbatim | Same OS, same constraints |
| `app_shell` GlassPanel | `:635–656` | Verbatim style | Same liquid-glass aesthetic |
| `sidebar` GlassPanel | `:658–678` | Layout verbatim | Replace nav buttons with octos sections (Home/Chat/Coding/Studio/Slides/Sites/Content/Settings) |
| Sidebar nav buttons | `:714–844` (`nav_new`, `nav_search`, `nav_plugins`, `nav_automation`, `nav_project`) | Style verbatim | Repoint actions; some inactive in M1 (search/plugins/automation) |
| `main_area` GlassPanel | `:845–865` | Verbatim | Adjust padding for octos-app density |
| `top_bar` ToolbarGlass | `:867–959` | Verbatim style | Replace `backend_dropdown` (8 LLMs) with profile picker; keep `opacity_slider` |
| `empty_state` (centred prompt + sub-label) | `:966–984` | Style verbatim | Repoint copy: "Connect your agents", "Start a new session" |
| `chat_shell` (Overlay: empty + ChatList) | `:961–987` | Verbatim | Same Overlay flow |
| `composer` GlassPanel | `:994–1015` | Verbatim style | Same input + actions row layout |
| `input` TextInput w/ font fallbacks | `:1017–1046` | Verbatim | Includes the per-instance theme.font_regular override w/ NotoSans + LXGW Mono + emoji |
| Composer actions (`+`, `@`, `⌘`, Thinking, cancel, clear, send) | `:1048–1112` | Mostly verbatim | Drop `Thinking` toggle in M1 (Octos handles thinking server-side per profile); add `task_dock` toggle |
| `status_label` | `:1116–1123` | Verbatim | Repurposed to show connection + cursor state |
| `ChatList` (Rust widget + DSL template) | `:343–614, 1774–1881` | Verbatim | The whole streaming PortalList. Renamed `aichat-history.json` → per-session SQLite cache (see 01-ARCHITECTURE §6) |
| `User` and `Assistant` PortalList templates | `:356–612` | Verbatim | Both already wire `code_block`, `splash_block`, `diagram_block`, `mermaid_block`, `inline_math`, `display_math` |
| Markdown widget config (font fallbacks, `text_style_fixed` for inline code) | `:368–448, 485–582` | Verbatim | The CJK/symbols/emoji font fallback work is non-trivial — keep |
| `MermaidSvgView` widget (custom Rust widget) | `:1384–1722` | Verbatim | Owns `streaming_markdown_kit::render_mermaid_to_svg`; depends on the SVG renderer — port intact |
| Streaming-markdown remend pipeline | `:14–16, 1799–1819` (`streaming_display_with_latex_autowrap_remend`, `SanitizeOptions`) | Verbatim | This is the secret sauce that keeps mid-stream code/math/tables stable |
| Per-character fade-in shader | `:501–509` | Verbatim | Same `get_color` fragment |
| `unwrap_outer_markdown_fence` helper | `:1203–1236` | Verbatim | Some LLMs wrap responses in ```markdown — same problem on Octos |
| Diagram safety scans (`assistant_message_is_safe_to_store` / `_for_history`) | `:1238–1335` | Verbatim, behaviour-equivalent | Octos may emit different fenced types, but the diagram safety rules are universal |
| `Splash` widget integration | `:415, 549, 2538–2543` (`script_mod` registration in `AppMain`) | Verbatim | Subject to sandbox review (see 00-CHARTER §"Splash inline UI" decision) |
| `code_view` CodeView from makepad-code-editor | `:404–411, 514–542` | Verbatim | Same per-instance font override needed |
| `inline_math` / `display_math` MathView | `:442–447, 576–581` | Verbatim | Same |
| `wrap_bare_latex` helper | `:14, 1847` | Verbatim | Wraps `\frac{...}` outside delimiters in `$…$` |
| Cmd/Ctrl-click link routing through `robius_open` | `:2434–2469` | Verbatim | Cross-platform URL open |
| Theme overrides (font_code, font_regular w/ multi-script fallback) | `:39–58` | Verbatim | The whole thing |
| `script_mod!` registration of dependent kits | `:2538–2543` | Verbatim shape | Add octos-app's own `script_mod` if needed |
| `MatchEvent` action handlers (send/cancel/clear/dropdown/toggle) | `:2413–2515` | Adapt | Keep dispatch shape; replace `BackendType` switching with `OctosUiAgent` profile/session switching |
| `AgentEvent` consumer loop | `:2565–2654` | Adapt | Replace `Box<dyn Agent>` of `BackendType`-built-agent with `OctosUiAgent` only; map new event variants (`tool/started`, `approval/requested`, etc.) |
| `app_main!(App)` / `AppMain` impl | `:18, 2537` | Verbatim | Standard Makepad entry |

## Stuff we drop or replace

These are aichat features not relevant to octos-app and should be deleted in the port to avoid
drag.

| aichat thing | Why we drop it |
|---|---|
| `BackendType` enum + `ALL_BACKENDS` + `from_index/to_index` | Octos serves all LLMs server-side. We don't pick a backend; we pick a profile |
| `BackendType::system_prompt` (huge diagram-prompt + splash.md preamble) | Octos profiles configure their own system prompts |
| Multi-LLM `create_agent` switch (`ClaudeCode/Splash/Acp/Api/Gemini/.../Moonshot`) | Replaced by single `OctosUiAgent::new(transport)` |
| `read_key_file` / `read_key` / env var probes for API keys | We use OS keychain for token, not file-based API keys |
| Moonshot thinking toggle (`thinking_toggle` checkbox) | Server-side concern |
| `glass_opacity_values` test invariant suite + slider tests | We keep the slider but the tests can be replaced with a simpler smoke test in W10 |
| `aichat_history.json` flat file persistence | Replaced by per-session SQLite cache + REST hydrate (01-ARCHITECTURE §6) |
| `default_backend` priority list | Replaced by "use the profile the user picked, default to first" |

## Stuff that needs new design

aichat is single-tenant, single-window, single-session. Octos isn't. These are net-new for
octos-app:

- **Session list pane** in the sidebar (PortalList, similar style to ChatList templates).
- **Profile picker** in the top bar (looks like `backend_dropdown`, but pulls from `/api/sessions`'s
  profile metadata or a separate profile listing endpoint).
- **TaskDock** under the composer — collapsible, shows live tools and tasks. New widget; reuses
  RubberView-style smoothing for collapse animation.
- **Approval cards** — payload-aware ApprovalCard with file diff hunks, command preview, network
  request preview (W05).
- **Connection indicator** in the status bar (green/amber/red).
- **Toast queue** for transient errors and reconnect notices (a small RubberView stack at
  bottom-right of `main_area`).
- **CodingScreen / StudioScreen / SlidesScreen / SitesScreen** layouts (M3).

## Lift cost estimate

Rough sizing for the chat surface (W03 deliverable):

- Verbatim copy of `script_mod!` UI block (rooted from `aichat:343–1142`): **~700 lines**
  copied with minor renames.
- Verbatim copy of `MermaidSvgView` and helper functions: **~400 lines**.
- New code (session list widget, profile picker, task dock, approvals stub): **~400 lines**.
- Adapted `App` struct + `MatchEvent` + `AppMain`: **~300 lines** (down from aichat's 600+ since
  we drop the multi-backend complexity).

Total chat surface estimate: ~1,800 lines, of which ~70% is direct port. Compared to
`octos-web/src/components/chat-thread.tsx` (1,519 LOC of React + assistant-ui), this is a roughly
similar size budget but in declarative Makepad DSL.

## How to read aichat alongside octos-app

When you're implementing a workstream and need a reference:

1. Find the closest aichat widget in the table above.
2. Open `aichat/examples/aichat/src/main.rs` at the cited line.
3. Read 30 lines on either side to capture the surrounding live-DSL context.
4. Copy verbatim into the equivalent file in `octos-app/app/src/main.rs`.
5. Diff against the aichat behaviour with the smoke checklist below.

## Smoke checklist after lift

After porting any chat-related widget, verify:

- [ ] CJK in inline code renders as glyphs, not tofu (LXGW Mono fallback worked)
- [ ] Unicode arrows / math symbols (`α`, `→`, `≤`) render in prose (NotoSans fallback worked)
- [ ] Mid-stream code blocks don't reflow on every token (remend pipeline worked)
- [ ] Streaming text fade-in is smooth (`get_color` shader applied)
- [ ] Cancel mid-stream freezes partial reply, doesn't corrupt history
- [ ] Cmd-click on a markdown link opens the OS browser
- [ ] Top drag strip moves the window, edges still resize
- [ ] Glass slider re-tints all four panels in the documented order

If any of these fails, the port lost something. The aichat tests at `:2658–2889` are a good
regression target; we keep them adapted for octos-app's variants.
