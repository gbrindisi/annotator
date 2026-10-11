# Changelog

All notable changes to annotator. Format: [Keep a Changelog](https://keepachangelog.com),
versions follow [SemVer](https://semver.org). Add entries under `[Unreleased]` as you go;
`make release VERSION=X.Y.Z` rolls them into a dated section.

## [Unreleased]

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
