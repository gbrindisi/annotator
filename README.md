# annotator

Review agent-written Markdown in your browser. An agent runs `annotator open plan.md`; the file
opens rendered in a browser tab where you read it, highlight text, leave comments and edit the
source. Clicking **Done** ends the agent's (blocking) command with a summary of what you did.
The agent addresses your comments and calls `open` again: round 2 shows up in the same tab, with
the changed blocks outlined and new replies badged.

```
agent ── annotator open plan.md ──▶ daemon ──▶ browser tab (render, comment, edit, autosave)
  ▲                                                     │
  └──────────── summary on stdout ◀── Done ◀────────────┘
```

Comments are stored inline as HTML comments, byte-compatible with
[markedit-comments](https://codebahn.net/g/markedit-comments) and Obsidian
[Document Comments](https://github.com/kylemcd/obsidian-document-comments), so they travel with
the file and agents can read them without any tooling.

## Install

Requires Rust (stable) and Node 22+.

```bash
make install      # builds the web UI + release binary, installs to ~/bin (BINDIR=... to override)
```

## Use

```bash
annotator open plan.md          # opens a tab, blocks until Done (exit 0) or Abandon (exit 2); prints the comments and a diff of the edits
annotator open plan.md --json   # the review as JSON (comments + diff of the reviewer's edits)
annotator open plan.md --no-wait  # register/join the round, open the tab, exit 0 (no agent waiting)
annotator status                # daemon and sessions
annotator stop                  # stop the daemon
annotator skill                 # print agent instructions (SKILL.md)
```

Each file gets one browser tab: `open` brings it to the front (macOS, Chrome-family or Safari;
the first time, macOS asks to let annotator control the browser) or opens it if it doesn't exist.

The daemon (`annotator serve`) starts on demand and exits after 15 idle minutes. It listens on
`127.0.0.1:7717` only (random port if busy), requires a per-run token, and only ever touches files
registered through `annotator open`.

In the browser:
- **Read** view: select text, click **Comment** (or Ctrl/Cmd-Alt-M). Click a highlight to focus
  its thread; click a thread's quote to jump to the text.
- **Edit** view: type directly in the rendered document; each edited block is saved live as Markdown
  for that block only (the rest of the file stays byte-identical, comment anchors are kept).
  Ctrl/Cmd-B / I / E / K for bold, italic, code, link; code blocks and diagrams are edited in Source.
- ` ```mermaid ` fences render as diagrams in a high-contrast theme (light and dark): click to zoom,
  hover for **Comment** (whole diagram) / **Source** (select lines to comment on them).
- **Source** view: CodeMirror with comment markers hidden; edits autosave.
- Sidebar: reply, edit, delete, resolve/reopen, delete thread; filter open/all.
- Round 2+: changed blocks are outlined (hover for the old text), `n` / `Shift-n` step through
  changes, new threads and replies are badged.
- **Done** sends the review back; **Abandon** ends it without action.

## macOS: Open With

```bash
make install-app   # builds Annotator.app into ~/Applications and registers it
```

Right-click a Markdown file in Finder, **Open With > Annotator**. The app is a tiny launcher that
runs `~/bin/annotator open --no-wait <file>`: the review opens in your browser as usual, edits
autosave, and since no agent is waiting, Done simply closes the round. It never becomes the default
handler for `.md` files (rank `Alternate`). Launched on its own it shows a file picker.

## Agent setup

Give your agent the skill: `annotator skill > ~/.pi/agent/skills/annotator/SKILL.md` (or the
equivalent for your harness). The gist: run `annotator open <file>` in the background with no
timeout (pi: `async_run`, `timeout: 0`), don't touch the file while it's under review, re-read it
when the command returns, answer in the threads, and call `open` again for the next round.

## Configuration

`~/.annotator/config.toml`:

```toml
author = "bomber"   # name on your comments; defaults to $USER
```

Environment: `ANNOTATOR_HOME` (state dir, default `~/.annotator`), `ANNOTATOR_PORT`,
`ANNOTATOR_IDLE_SECS`, `ANNOTATOR_BROWSER` (`none` or a launcher command).

## Development

```bash
make help
make test         # Rust tests + web typecheck/tests
make lint
cd web && npm run dev   # UI against an in-browser mock daemon, no Rust needed
```

Layout: `src/format` (comment format), `src/render.rs` + `src/mapping.rs` (Markdown to HTML with
source positions; rendered selection to source range), `src/daemon` (HTTP/WebSocket server,
sessions, rounds, file watching), `src/client.rs` (the `open` CLI), `src/diff.rs` and
`src/summary.rs` (round changes and Done summary), `web/` (TypeScript UI, embedded at build time).
`spike/` documents the selection payload the UI sends.

## Releasing

See [RELEASING.md](RELEASING.md): `make release VERSION=X.Y.Z`, changelog in `CHANGELOG.md`.

## License

MIT. See LICENSE and NOTICE.
