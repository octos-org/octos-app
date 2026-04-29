# W07 — Studio / Slides / Sites

## Mission

Stand up the three producer surfaces — **Studio**, **Slides**, **Sites** — as the smallest
useful triptych on top of the chat machinery W03 shipped. Each is structurally the same:
source on the left, AI chat in the middle, generated output on the right. Composer,
streaming pipeline, and message renderer reuse W03 verbatim; what's new is per-producer
state, system-prompt context, output preview, and producer-specific affordances. Per the
charter, no parity with `octos-web` on day one — this is a *shell* proving the producers
are reachable through the UI Protocol.

## Header

| | |
|---|---|
| **ID** | W07 |
| **Lane** | C — Producers |
| **Depends on** | W04 (sessions, tasks, files, message store) |
| **Lifts from** | aichat composer + thread (`aichat:994–1142, 343–614`); `octos-web/src/{studio,slides,sites}/layouts/*` |
| **Milestone** | M3 |
| **Size** | ~2,400 lines, ~50% reuse from W03 |

## Scope

**In:** three sub-screens (`StudioScreen`, `SlidesScreen`, `SitesScreen`); per-producer
project model with create/open/list; triptych GlassPanel layout; embedded chat thread per
project with system-prompt context; Studio source picker + options + output cards; Slides
thumbnail list + files; Sites file tree + screenshot preview; producer-flavoured task
dock.

**Out:** slide **present mode**; **embedded browser** for live site preview (screenshot
only, `robius_open` link out); **real-time collaborative editing**; in-app PPTX rendering
(hand .pptx off to the OS handler); dev-server proxy; fork from research session.

## Shared layout pattern

All three screens share a triptych built from the GlassPanel primitive (lifted in W02,
`aichat:635–865`). The frame is identical so all three fit one `PageFlip` slot:

```
┌────────────────────────┬────────────────────────────┬──────────────────────────┐
│ source / files panel   │ chat panel (composer +     │ output preview panel     │
│ - sources / slides /   │   thread, lifted from W03) │ - studio: card grid      │
│   files tree           │                            │ - slides: thumb deck     │
│ - selection toggles    │                            │ - sites: screenshot      │
└────────────────────────┴────────────────────────────┴──────────────────────────┘
```

The chat panel instantiates W03's `ChatList` with a per-project `SessionId` and wires the
same streaming pipeline. Per-producer differences live **around** the composer, not inside
it. The header strip is a thin variant of `top_bar` carrying project title, status pill,
and panel toggles (mirrors `octos-web/src/slides/layouts/slides-editor-layout.tsx:151–177`).
Outer panels are collapsible; layout state lives in `AppState.ephemeral` keyed by producer
and never roundtrips through the protocol.

## StudioScreen

Layout reference: `octos-web/src/studio/layouts/studio-layout.tsx:11–67`.

- **Source panel (left):** `SourcePicker` with URL / file / pasted-text modes plus a
  source list with per-item toggles. Shape from
  `octos-web/src/studio/components/source-panel.tsx:13–117`; uploads route through W04.
- **Chat panel (centre):** W03 composer + thread on the project's `chatSessionId`;
  `studio.system_prompt.v1` wraps each turn.
- **Output panel (right):** card grid of `StudioOutput`s (summary / report / podcast /
  slides) with status, preview, "open" action — inline for markdown, `robius_open` for
  audio / PPTX.
- **Generation options form:** sheet with `llm`, `style`, `format`. Prompt shape from
  `octos-web/src/studio/components/studio-panel.tsx:14–60`, submitted via the turn API.
  Outputs land via W04's tool/file machinery.

## SlidesScreen

Layout reference: `octos-web/src/slides/layouts/slides-editor-layout.tsx:100–224`.

- **Slide thumbnail list (left):** PortalList of server-rendered slide PNGs (mirrors
  `octos-web/src/slides/store.ts` manifest polling). Selecting a slide prefixes the next
  composer message with "Re: slide N". Per-slide messages aren't a separate stream —
  they're a pre-prefix on the thread; `currentSlide` lives in `SlidesProject`.
- **Chat panel (centre):** composer + thread, system prompt `slides.system_prompt.v1`,
  augmented with slide context.
- **Files / preview panel (right):** file list (`/api/files/list?path=slides/<slug>`)
  above a thumbnail pane. "Download PPTX" is primary CTA when the file exists.
- **In-band PPTX export:** if Octos exposes `octos-office`
  (`capabilities.pptx.export.v1`), surface "Export PPTX" wired to `tool/export.pptx.v1`.
  **Out for M3 if not ready** — the agent already produces PPTX via tools.

No present mode in M3.

## SitesScreen

Layout reference: `octos-web/src/sites/layouts/sites-editor-layout.tsx:118–248`.

- **Files tree (left):** tree of files under `sites/<slug>` with type icons. Click opens
  the W04 `ContentViewer` overlay. File CRUD delegated to chat — no in-app mutation.
- **Chat panel (centre):** composer + thread, system prompt `sites.system_prompt.v1`,
  with an intake heuristic inferring a preset (Astro / Next.js / Quarto / React-Vite)
  from the first message and triggering `/new sites <preset> <slug>` (same flow as
  `octos-web/src/sites/components/sites-chat.tsx`).
- **Preview pane (right):** **screenshot fallback only**. If Octos serves built bytes
  via `/api/files/list?path=sites/<slug>/dist`, render an `index.html` first-paint
  screenshot. "Open preview" calls `robius_open::Uri` on the gateway `previewUrl`
  (`SiteSession.preview_url`, `octos-web/src/sites/api.ts`). Embedded browser
  **deferred**. No preview URL → disabled pill + status note.

## Per-producer state model

Each producer screen is parameterised by a `ProjectId`:

```rust
pub struct ProducerState {
    pub studio: HashMap<ProjectId, StudioProject>,
    pub slides: HashMap<ProjectId, SlidesProject>,
    pub sites:  HashMap<ProjectId, SiteProject>,
}
```

Each project carries `id`, `title`, timestamps, `chatSessionId`, `messages: Vec<MessageId>`
(handles into the W04 message store — no duplication), and a producer-specific generation
history (Studio: `outputs`; Slides: slide list + manifest timestamp; Sites: scaffold
status + preview URL). A `scaffolded: bool` gates UI affordances. The polling pattern from
`octos-web/src/slides/context/slides-context.tsx:43–124` and
`octos-web/src/sites/context/sites-context.tsx:34–107` becomes a Tokio task feeding
`AgentEvent::ProjectScaffolded` into the event loop. Web Provider components translate to
`Arc<RwLock<ProducerState>>` accessors plus `App` action methods.

## Where this differs from chat

- **System-prompt context** wraps each turn — we send `system_prompt_id` (`studio.v1` /
  `slides.v1` / `sites.v1`) on the turn create RPC; the server resolves the body. No
  prompt content is stored client-side.
- **Producer-flavoured task dock.** W04 dock filters by tool family — Studio:
  `web_search`, `voice_synthesize`, `read_pdf`; Slides: `slide_render`, `pptx_compose`;
  Sites: `site_scaffold`, `site_build`, `site_preview`.
- **Composer attachments** point at the project source list. Clicking `+` in Studio
  offers "Add a source" rather than "Upload file" — same `attach.put.v1`, producer-aware
  label.

## Deliverables

### W07-A — Studio
1. `StudioScreen` live-DSL widget (three GlassPanel cells).
2. `SourcePicker` (URL / file / text) + `SourceList` with select / remove.
3. `GenerationOptionsSheet` (model / style / format).
4. `OutputCard` with status, preview, open action.
5. `StudioProject` store + actions.

### W07-B — Slides
1. `SlidesScreen` widget (thumbnails / chat / files+preview).
2. `SlideThumbnailList` (auth pattern from `octos-web/src/slides/components/authenticated-file-image.tsx`).
3. `ProjectFiles` list, `SlidePreview` (thumbnail + "Download PPTX").
4. PPTX export wired to `tool/export.pptx.v1` if capability present.
5. `SlidesProject` store + manifest poller.

### W07-C — Sites
1. `SitesScreen` widget (files-tree / chat / preview).
2. `FileTree` collapsible directory widget.
3. `SitePreview` — screenshot fallback + "Open preview" link.
4. Preset intake heuristic (mirrors `octos-web/src/sites/intake.ts`).
5. `SiteProject` store + scaffold poller.

### W07-D — Shared
1. `ProducerLayout` triptych frame.
2. Sidebar second-pane project lists.
3. `system_prompt_id` wiring on turn create.
4. Producer-flavoured task dock filter.
5. `AppState.current` extension for `Studio` / `Slides` / `Sites`.

## Tests & verification

- **Unit:** project store CRUD round-trips; source-picker paths produce consistent
  records; manifest polling collapses idempotently.
- **Smoke:** open each producer, scaffold, send one turn, confirm streaming (chat-port
  checklist from `05-AICHAT-REUSE-MAP.md`).
- **Integration** (W10 test server): Studio summary lands `ready`; Slides scaffold
  completes with a thumbnail; Sites scaffold completes with `previewUrl` populated.
- **Manual:** open PPTX via `robius_open`; "Open preview" on Sites opens the gateway URL.

## Exit criteria

- **Studio:** user creates a project, adds a source, picks a tile, submits, sees streamed
  output land in the right-panel grid, opens the artifact when applicable.
- **Slides:** project scaffolds via `/new slides`, thumbnails appear as the agent
  generates, the file list shows the PPTX, clicking opens the OS handler.
- **Sites:** project scaffolds via `/new sites`, files appear in the tree, screenshot
  renders if available, "Open preview" opens the gateway URL.

## Risks

- **PPTX rendering.** We rely on the server for thumbnails. If `slide_render` fails
  silently the editor looks empty. Mitigation: error toast when `manifestGeneratedAt`
  doesn't advance after N polls.
- **Sites preview.** Screenshot only; users will expect live edit-and-refresh.
  Mitigation: scope in empty-state copy; revisit CEF/WebView in M4.
- **Iteration speed of three surfaces in parallel.** Drift comes easy; the triptych
  frame must be one widget — padding / toggles / resize land in `ProducerLayout` once.
- **Capability negotiation.** PPTX export and site preview are optional server
  capabilities; gate on the W01 capability probe.

## Open questions

- **Native vs OS handoff for output rendering.** Do we open producer outputs in OS apps
  via `robius_open` or render natively? PPTX and audio hand off (no native renderers).
  Markdown already renders natively in chat. **Default for M3:** inline for markdown/text,
  OS handoff for binary.
- **Site preview eventually embedded.** `cef` would embed live; cleaner to wait for
  upstream WebView. Deferred.
- **Per-slide chat threads.** M3 uses one thread with a slide prefix. If per-slide
  conversations prove valuable, promote `currentSlide` to a thread filter — bigger lift.
- **Project list pagination.** M3 ships 50 most recent, matching `octos-web`. Search
  deferred.
