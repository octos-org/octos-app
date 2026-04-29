# Audit — 2026-04-28

## Verdict

**Cross-refs: amber.** Three broken section links in workstreams (two in W08, one
in W03), all fixable with one-line edits and now repaired. **Naming: green.**
Canonical types from `01-ARCHITECTURE.md` (`OctosUiAgent`, `octos-app-{transport,
store,render}`, `AppState.cursor`, `Connection`) survive intact across all ten
workstream docs; no `UiProtocolAgent`/`octos-app-net`/`AppState.last_cursor`
forks. **Coverage: green.** Every `Wnn` row in `06-WORKSTREAMS.md § "Workstream
catalog"` has a matching `workstreams/Wnn-*.md` file. **Length: green/amber.**
Only `W09-build-packaging.md` overshot the 1500-word target by more than 15%
(1736 vs 1725 ceiling — 11 words over the cap).

## Fixes made

- `workstreams/W08-auth-tenancy.md` § Profile picker UI: changed
  `02-API-DRIFT.md "Open asks"` (no such heading) to
  `02-API-DRIFT.md § "What we ask the server team for"`.
- `workstreams/W08-auth-tenancy.md` § Risks: same broken ref, same fix.
- `workstreams/W03-chat.md` § Scope (Splash bullet): changed
  `00-CHARTER.md "Splash inline UI"` (no such heading) to
  `00-CHARTER.md § Risks` (Risks table is the canonical mention site in 00).
- `workstreams/W04-sessions-tasks-files.md` § 5: fixed
  `03-PROTOCOL-CONTRACT.md § Tool / task / progress` to
  `… § Tool / task / progress events` (heading carries the trailing word).

## Issues flagged but not fixed

- **W09 over the length cap (1736 words; 11 over).** Suggested resolution:
  trim the macOS signing & notarization step list (currently 7 numbered items
  with prose) into a tighter prose paragraph; or merge the "Linux (.deb +
  .AppImage)" and "Windows MSI" subsections under one § "Per-OS notes". Saves
  ~50 words without losing content. Holding because trimming risks rewriting
  paragraphs.

- **`05-AICHAT-REUSE-MAP.md` line 37 contains the same broken
  `00-CHARTER §"Splash inline UI"` reference.** This is a foundation doc
  (out of scope for this audit's edit-in-place mandate). Suggested resolution:
  re-point to `01-ARCHITECTURE.md § 9` decision #2.

- **Minor ref inconsistency, W04 line 79** (`04-IA-AND-NAVIGATION.md
  ChatScreen`, no `§`). Heading exists. Cosmetic; could be `§ ChatScreen` for
  consistency with W04 lines 50, 55.

- **Module-path drift in workstream module trees.** `01-ARCHITECTURE.md § 2`
  prescribes `app/src/app/<file>.rs`. W03 / W04 / W08 use `app/app/<file>.rs`
  (drop `src/`); W06 uses `app/src/coding/{screen,approval_queue,…}.rs`
  (subdir-style modules) where the canonical layout is a single `coding.rs`.
  Suggested resolution: a coordinated path-rewrite pass during W02
  scaffolding, when the actual repo lands; doing it now risks invalidating
  cross-workstream cite line numbers without buying anything.

- **`Session` vs `SessionMeta` (W04 § 9 AppState slice).** `01-ARCHITECTURE.md`
  line 132 comments `SessionMap // SessionId → Session`; W04 introduces
  `SessionMeta` as the inner type. Suggested resolution: rename to
  `Session` in W04 when the store crate's first PR lands, or update
  `01-ARCHITECTURE.md`'s comment to `SessionMeta`. No blocker.

- **W01 `ConnectionState` vs `AppState.connection: Connection`.** Different
  layers, related names. W01's `ConnectionState` is the transport state
  machine; `AppState.connection: Connection` is the store-side projection.
  No fix needed — the names live at different layers — but a one-line W01
  callout would prevent confusion.

## Next

Workstream owners should read `06-WORKSTREAMS.md` for the dependency DAG and
sequencing before opening their assigned `workstreams/Wnn-*.md`. Coordination
items (server-team asks, open architectural decisions) live in
`06-WORKSTREAMS.md § Coordination & open asks` and § Open decisions.
