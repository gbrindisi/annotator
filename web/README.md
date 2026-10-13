# annotator web UI

Vanilla TypeScript + Vite + CodeMirror 6. Built into `web/dist` (`make web` = `npm ci && npm run build`)
and served by the daemon at `/s/<session>`; talks to `/ws/<session>` using the contract in
`src/protocol.rs` (mirrored in `src/protocol.ts`). The browser is a thin view: it renders the
pushed `DocState` and sends `ClientMsg` intents.

```sh
npm ci
npm run dev        # http://localhost:5173/ runs against the in-browser mock daemon (src/dev/mock.ts)
npm test           # vitest + jsdom
npm run typecheck
npm run build
```

Mock console helpers: `__mock.drop()` (disconnect), `__mock.down(5000)` (daemon down for 5s),
`__mock.touch()` (external file edit -> "changed on disk" notice while you have unsent edits),
`__mock.nextRound()`, `__mock.error('msg')`. Clicking **Done** closes the round; the mock agent
answers with round 2 (changes, a reply, a new thread) after ~4s.

Layout: `src/app.ts` (shell, state, WS handling), `src/ui/*` (sidebar, cards, composer, adapted
from the MarkEdit plugin), `src/selection.ts` (spike selection algorithm), `src/marks.ts`,
`src/changes.ts`, `src/mermaid.ts` + `src/mermaidTheme.ts` + `src/zoom.ts` (diagrams, lazy), `src/decorations.ts` + `src/source.ts` (CodeMirror), `src/connection.ts`.
