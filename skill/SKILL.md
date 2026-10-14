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

Exit 0 prints the review itself: the comments and the reviewer's direct edits.

```
annotator: review of /abs/plan.md finished (round 2)

## Comments (2 new, 1 reply)
[k3f9] new on "ship on Friday"
  bomber: I thought we agreed Thursday?
[p7k2] reply on code lines 2-3 ("validate(order)")
  bomber: yes, before logging

## Edits (1 hunk, +1 -1)
--- plan.md (round 2 start)
+++ plan.md (after review)
@@ -10,3 +10,3 @@
...
```

- Each comment shows its id, kind (`new`, `reply`, `edited`, `resolved`, `reopened`, `deleted`),
  the text it is anchored to, and the entries added this round. Answer each by id.
- The diff is the reviewer's direct text edits, comment markers excluded. **Their edits win** over
  your version: keep them, don't revert them.
- Still **re-read the file** before editing: the diff has no comment markers and may not show
  everything in context.
- `- no changes` means the reviewer sent it back untouched.

`--json` gives the same as JSON (`summary.diff`, `summary.comments[]` with `id`, `kind`, `quote`,
`new_entries`, ...). Exit 2 means the user abandoned the review: don't act on it unless they say
so. Exit 1 is an error (message on stderr).

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
