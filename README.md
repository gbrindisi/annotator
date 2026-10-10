# annotator

Local review sessions for agent-written Markdown. An agent runs `annotator open plan.md`; the file
opens rendered in a browser tab where you edit, highlight and comment; clicking **Done** ends the
agent's blocking command with a summary of what you did. The agent addresses the comments and
calls `open` again for the next round, in the same tab.

Comments are stored inline, byte-compatible with
[markedit-comments](https://codebahn.net/g/markedit-comments) and Obsidian Document Comments.

Status: under construction. `make help` for targets.

`reference/markedit-comments/` holds the TypeScript format implementation being ported to
`src/format/` (MIT, see its LICENSE and NOTICE). It is removed once the port lands.
