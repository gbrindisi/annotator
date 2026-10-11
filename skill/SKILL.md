---
name: annotator
description: Hand a Markdown document to the human for review in the browser with annotator, wait for them to send it back, then address their inline comments. Use whenever you wrote or updated a Markdown doc (plan, design, spec, report) the user should read, or the user asks you to "open" a doc for review.
---

# annotator

`annotator open <file.md>` opens the file rendered in the user's browser, where they edit it,
highlight text and leave comments. The command **blocks until the user clicks Done** (exit 0) or
Abandon (exit 2), then prints a factual summary of what they did.

## Running it

Run it in the background so you stay responsive, and disable the timeout: reviews can take hours.

- pi: `async_run` with `command: "annotator open /abs/path/plan.md"` and `timeout: 0`.
  The completion message is your signal that the review is back.
- Other harnesses: any background-shell facility without a timeout; otherwise run it in the
  foreground and accept that you are blocked.

The URL is printed on stderr in case the user needs it again.

Do not pass `--no-wait`: it is for launchers with no agent waiting; you would never get the review back.

## While the review is open

**Do not modify the file.** The user's browser autosaves to it. Work on something else or end
your turn.

## When it returns

Exit 0 prints something like:

```
annotator: review of /abs/plan.md finished (round 1)
- 3 new comments: k3f9, a1b2, zz9q
- 1 reply on an existing comment: p7k2
- resolved: none; deleted: none
- edits: 2 hunks, +5 -1 lines
```

`--json` gives the same as JSON. Exit 2 means the user abandoned the review: don't act on it
unless they say so. Exit 1 is an error (message on stderr).

Then **re-read the file**: the user may have edited the text directly, and their edits win.

## The comment format

Comments are stored inline as HTML comments:

```markdown
We should <!--c:k3f9-->ship on Friday<!--/c:k3f9--> regardless of QA.
<!--co:k3f9 by:bomber at:2026-09-24T14:30:00.000Z status:open quote:"ship on Friday"
bomber (2026-09-24T14:30:00.000Z): I thought we agreed Thursday?
-->
```

- `<!--c:ID-->` / `<!--/c:ID-->` wrap the commented text; `<!--co:ID ...-->` holds the thread, one
  `author (timestamp): text` line per entry. `line:N` in the header anchors to lines of a code block.
- To answer a comment, append a line to its body, before `-->`:
  `agent (<ISO-8601 UTC now>): <your answer>`. Keep answers short; say what you changed.
- Leave `status:` alone unless the user asked you to resolve threads; resolving is their call.
- When you rewrite commented text, keep the markers around the new text. Only delete a thread
  (markers + body) if the user asked you to.
- Never touch markers inside fenced code blocks: those are literal text.

## Next round

After addressing the comments, call `annotator open` on the same file again (in the background,
`timeout: 0`). It starts round N+1 in the same browser tab, which shows what you changed and your
replies. Repeat until the user is satisfied.
