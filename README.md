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
annotator open plan.md          # opens a tab, blocks until Done (exit 0) or Abandon (exit 2)
annotator open plan.md --json   # summary as JSON
annotator status                # daemon and sessions
annotator stop                  # stop the daemon
annotator skill                 # print agent instructions (SKILL.md)
```

The daemon (`annotator serve`) starts on demand and exits after 15 idle minutes. It listens on
`127.0.0.1:7717` only (random port if busy), requires a per-run token, and only ever touches files
registered through `annotator open`.

In the browser:
- **Rendered** view: select text, click **Comment** (or Ctrl/Cmd-Alt-M). Click a highlight to focus
  its thread; click a thread's quote to jump to the text.
- **Source** view: CodeMirror with comment markers hidden; edits autosave.
- Sidebar: reply, edit, delete, resolve/reopen, delete thread; filter open/all.
- Round 2+: changed blocks are outlined (hover for the old text), `n` / `Shift-n` step through
  changes, new threads and replies are badged.
- **Done** sends the review back; **Abandon** ends it without action.

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
