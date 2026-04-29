# 04 — Information Architecture & Navigation

What screens exist, how the user moves between them, and which routes lift directly from
`octos-web`'s layout vs. require new design.

Reference for the existing IA: `~/home/octos-web/src` (React Router config + page components).
The web app uses URL routes; `octos-app` is single-window so we use Makepad route IDs (live-DSL
`PageFlip` or sidebar-driven view switch) but keep names parallel.

## Top-level shell

Single window, three regions, fixed across all routes. Lifted from `aichat`'s
`app_shell` → `sidebar` → `main_area` layout (`aichat/examples/aichat/src/main.rs:635, 658, 845`).

```
┌──────────────────────────────────────────────────────────────────────┐
│  app_shell  (GlassPanel, transparent macOS chrome, drag strip top)   │
│  ┌──────────────┬─────────────────────────────────────────────────┐  │
│  │              │  top_bar: profile picker · backend status ·     │  │
│  │   sidebar    │           glass slider · connection indicator   │  │
│  │              │  ─────────────────────────────────────────────  │  │
│  │  - Home      │                                                 │  │
│  │  - Chat ▼    │                                                 │  │
│  │  - Coding    │                                                 │  │
│  │  - Studio    │            main_area (PageFlip)                 │  │
│  │  - Slides    │                                                 │  │
│  │  - Sites     │            (Home | Chat | Coding | …)           │  │
│  │  - Content   │                                                 │  │
│  │  - Settings⤴ │                                                 │  │
│  │              │  ─────────────────────────────────────────────  │  │
│  │              │  status_label: "Connected · 12ms · cursor 4382" │  │
│  └──────────────┴─────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
```

The sidebar's "Settings ⤴" item opens the web dashboard in the OS browser via `robius_open`
(already a dep in `aichat`). Native admin UI is out of scope (charter).

## Routes

`octos-web` route → `octos-app` page mapping. "Lifted" = aichat already has the widgets we need;
"New" = built in this project.

| Web route | Page | octos-app surface | Source |
|---|---|---|---|
| `/login` | Email + OTP | `LoginScreen` | New (W08) |
| `/` | Home / quick actions | `HomeScreen` | New small surface (W02) |
| `/chat/*` | Chat thread + sidebar | `ChatScreen` (within main_area PageFlip) | Lifted from `aichat` (W03) |
| `/coding` | Coding workspace | `CodingScreen` | New (W06) — heavy approvals + diffs |
| `/studio/new`, `/studio/:projectId` | Studio | `StudioScreen` | New (W07) |
| `/slides`, `/slides/:id`, `/slides/:id/present` | Slides | `SlidesScreen` (no Present mode in M3) | New (W07) |
| `/sites`, `/sites/:id` | Sites | `SitesScreen` | New (W07) |
| `/settings` | Admin redirect | external link to web dashboard | Out of scope |

Within `ChatScreen`, the URL-style nesting (`/chat/:session_id`) becomes a sidebar selection that
swaps the active `SessionId` in `AppState.current`. No deep-linking in M1; revisit if we need to
share chat URLs.

## Per-screen breakdown

For each, "Lifts from" cites a specific aichat widget; "New" calls out what we build.

### LoginScreen

- Email field, "Send code" button, code field, "Verify" button, error label.
- Calls `POST /api/auth/send-code` then `POST /api/auth/verify`.
- Stores token in OS keychain (`keyring` crate).
- After login, shows profile picker if user has multiple profiles; otherwise drops to Home.
- **Lifts from**: `aichat`'s `TextInput` styling and `PillButton`.
- **New**: keychain integration, profile picker.

### HomeScreen

- Octopus title, status callouts, four large quick-action buttons: New Chat, Studio, Slides, Coding.
- Recent sessions list (top 5).
- **Lifts from**: aichat's empty-state layout (`aichat:966–984`) but with action buttons.
- **New**: action button styles, recent-sessions snippet (small `PortalList`).

### ChatScreen — the carry vehicle for M1

The whole point of leveraging `aichat` is that this screen is mostly a port. Layout:

```
┌──────────────────────┬────────────────────────────────────────────┐
│  session_list        │  chat_thread (ChatList → PortalList)       │
│  - "New chat"        │                                            │
│  - active session ●  │  User: …                                   │
│  - "yesterday"       │  Assistant: streaming markdown w/ code +   │
│    - session         │             math + diagram + mermaid +     │
│    - session         │             splash blocks                  │
│  - "last week"       │                                            │
│    - session         ├────────────────────────────────────────────┤
│                      │  composer (GlassPanel)                     │
│                      │  - input (TextInput, multiline)            │
│                      │  - + @ ⌘ · Thinking · cancel · clear · ↑   │
│                      ├────────────────────────────────────────────┤
│                      │  task_dock (collapsible) — tools + tasks   │
└──────────────────────┴────────────────────────────────────────────┘
```

- **Lifts from**: `aichat`'s `ChatList` widget verbatim (`aichat:343–614, 1774–1881`), composer
  block (`aichat:994–1112`), the entire `splash_view` / `code_view` / `diagram_view` / `mermaid_view`
  / `MathView` pipeline, the streaming-markdown remend pass, and the per-character fade-in shader.
  See `05-AICHAT-REUSE-MAP.md` for the full table.
- **New**: session list (sidebar pane, not in `aichat`), task dock under the composer
  (collapsible), connection indicator in the status bar, profile picker in top bar.

### CodingScreen (M3)

Two-pane layout:

```
┌────────────────────────────┬────────────────────────────────────┐
│  approvals queue           │  preview pane (PageFlip)           │
│  - "ban writes outside…"   │  - diff view (FileTree + Hunk list)│
│    [approve] [deny] [⌘]   │  - command preview                 │
│  - "run shell: ls…"        │  - network preview                 │
│    [approve] [deny]        │  - tool output tail                │
│                            │                                    │
│  history                   │                                    │
│  - earlier approvals       │                                    │
│  - tasks list              │                                    │
└────────────────────────────┴────────────────────────────────────┘
```

- **Lifts from**: aichat's `code_view` (CodeView from makepad-code-editor) for diff hunks.
- **New**: `ApprovalCard` (typed-payload-aware), `DiffView` widget (file tree + hunk renderer),
  `CommandPreview`, `NetworkPreview`. All in W05/W06.

### StudioScreen / SlidesScreen / SitesScreen (M3)

Same triptych for all three: source panel · chat sidebar · output preview.

- **Lifts from**: chat composer + thread (`aichat`), markdown renderer.
- **New**: source picker (URL / PDF / text), generation options panel, output card grid,
  per-format preview pane (slides preview, site iframe — Makepad doesn't have a native browser;
  use `cef` integration or thumbnail-only).

### Sidebar — the global session list

Always visible. Two main blocks: navigation (Home / Chat / Coding / Studio / Slides / Sites /
Content / Settings) and active-section context (when in Chat, the session list; when in Studio,
the project list; etc.).

- **Lifts from**: `aichat:680–844` sidebar layout, button styles, Chinese-friendly labels.
- **New**: dynamic content area (PageFlip inside sidebar bottom half).

## Cross-cutting affordances

| Affordance | Where | Source |
|---|---|---|
| Profile picker | top bar dropdown | New (W08); look-and-feel from `aichat` `backend_dropdown` |
| Connection indicator (green / amber / red dot) | top bar right | New (W01) |
| Glass-opacity slider | top bar | Lifted from `aichat:949` |
| Toast notifications (errors, reconnect) | bottom-right of `main_area` | New (W02) — a small `RubberView`-based toast queue |
| Context menu on messages (copy, delete) | right-click on bubble | Lifted from `aichat:589, 600` (copy/delete buttons) |
| Drag-strip for window move | top edge of `app_shell` | Lifted from `aichat:1179, 2552–2559` |
| Cmd/Ctrl-click on links opens browser | markdown links | Lifted from `aichat:2434–2469` (`robius_open::Uri`) |

## Theming

- Single dark theme for M1. `aichat`'s liquid-glass palette (cream / cyan / gold accents on
  deep teal) ports verbatim.
- Light theme deferred to M2+; `octos-web` has it via CSS variables, we'd need a mirrored palette
  switch (Makepad theme override blocks).

## Internationalisation

- `aichat` already mixes English + Chinese in its DSL ("新对话", "搜索", "插件", "我们该做什么？").
- Backend strings (error messages, tool names) come from server in English.
- Treat M1 as bilingual EN/ZH (matching `aichat`); add full i18n table in M3 if we get user
  demand. `octos-web` is currently English-only.

## Navigation state machine

`AppState.current` is a flat enum:

```rust
pub enum CurrentScreen {
    Login,
    Home,
    Chat { session: Option<SessionId> },
    Coding,
    Studio { project: Option<ProjectId> },
    Slides { project: Option<ProjectId> },
    Sites { project: Option<ProjectId> },
}
```

Navigation events:

- `NavigateTo(CurrentScreen)` — sidebar click
- `OpenSession(SessionId)` — selects Chat with that session
- `OpenProject(Producer, ProjectId)` — selects Studio/Slides/Sites with that project
- `Logout` — clears keychain, returns to `Login`

The `main_area` PageFlip key is derived from `current.discriminant()`. Per-screen state
(scroll, focus) lives in `AppState.ephemeral`; it doesn't roundtrip through the protocol.

## What we don't ship in M1

- Search inside sessions (`octos-web`'s `nav_search`). Affordance present in sidebar but inactive.
- Plugins / automation / project tabs (`octos-web`'s `nav_plugins`, `nav_automation`,
  `nav_project`). Inactive.
- Slide present mode.
- Multi-window detach (sessions in their own window).
- Drag-and-drop file upload from desktop. (Click the `+` attach button — modal file picker.)
