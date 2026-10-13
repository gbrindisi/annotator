# Changelog

All notable changes to annotator. Format: [Keep a Changelog](https://keepachangelog.com),
versions follow [SemVer](https://semver.org). Add entries under `[Unreleased]` as you go;
`make release VERSION=X.Y.Z` rolls them into a dated section.

## [Unreleased]

### Added
- Mermaid diagrams: ` ```mermaid ` fences render as SVG in the rendered view (mermaid v11,
  lazy-loaded only when a document has a diagram, `securityLevel: strict`) with a custom
  high-contrast theme for light and dark mode. Invalid diagrams show an error box above the code.
  Click a diagram to zoom (fit to screen, wheel/pinch zoom, drag to pan, Esc closes).
- Comments on diagrams: hover toolbar with **Comment** (anchors a line comment on the whole code
  block) and **Source** (show the code block instead, to comment on specific lines). Diagrams with
  comments get a highlight frame and a count badge that focuses the thread; a code comment's
  "jump to" scrolls to its diagram / code block.
- Round changes on diagrams: added/modified diagrams get the change bar, modified ones a
  "Previous version" toggle, removed ones show the old diagram in the removed-block box.
- `CommentView.block_sourcepos` (protocol, additive): the `data-sourcepos` of the `<pre>` a
  code-line comment belongs to, exactly as in `DocState.html`.
- `fixtures/mermaid.md`: sample document with every supported diagram kind plus an invalid one.

## [0.2.0] - 2026-10-11

### Added
- One tab per file: every `annotator open` brings the file's existing browser tab to the front
  (closing duplicates, reloading it if it lost its connection) and only opens a new tab if none
  exists. macOS with Chrome-family browsers or Safari; elsewhere, a tab opens when none is connected.
- `annotator open <file> --no-wait`: register/join the round and open the tab, then exit 0
  without waiting (for launchers such as Finder "Open With"); `--json` prints the open response.
- macOS `Annotator.app` (`make install-app`): Finder "Open With > Annotator" for Markdown files,
  never the default handler; runs `annotator open --no-wait`.
- `DocState.waiters` (protocol, additive): pending agent waits for the round, pushed live.
- UI: "No agent waiting" hint in the top bar, and a "Review closed" banner after Done when no
  agent was waiting. Dev mock: `__mock.waiters(n)` / `__mock.toggleWaiters()`.

## [0.1.0] - 2026-10-11

### Added
- `annotator open <file>`: blocking review CLI (exit 0 on Done with a factual summary, 2 on
  Abandon, `--json`), plus `serve`, `status`, `stop`, `skill`.
- Lazy local daemon (127.0.0.1, per-run token, Host/Origin checks, idle shutdown) with sessions
  and rounds persisted under `~/.annotator`, file watching and live WebSocket updates.
- Inline comment format compatible with markedit-comments / Obsidian Document Comments.
- Browser UI embedded in the binary: rendered view with exact selection-to-source anchoring,
  source mode (CodeMirror), threads sidebar, autosave, inline track-changes between rounds.
- Agent skill (`skill/SKILL.md`, printed by `annotator skill`).
