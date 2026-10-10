# Selection spike

```sh
cargo run --example spike -- fixtures/sample.md > spike/out.html
xdg-open spike/out.html   # or open it in any browser
```

`index.html` is the template (rendered HTML is substituted at `<!--RENDERED-->`); `out.html`
is ignored by git. Select text in the left pane: the right pane shows the exact
`ClientMsg::AddComment` payload the frontend must send.

## Frontend algorithm for `Selection`

1. `range = getSelection().getRangeAt(0)`; ignore collapsed selections.
2. `block` = closest element to `range.commonAncestorContainer` that is a block tag
   (`p, li, h1-h6, td, th, pre, blockquote, ul, ol, table, tr`) with `data-sourcepos`.
   If the selection spans blocks, this is a container (or nothing) and the daemon answers
   `CrossBlock`.
3. `sourcepos = block.dataset.sourcepos` (verbatim).
4. `text = range.toString()`.
5. `offset` = `toString().length` of a range from `(block, 0)` to
   `(range.startContainer, range.startOffset)`: UTF-16 units into `block.textContent`.

Note: `<mark data-cid>` elements carry no `data-sourcepos`; they are transparent to this
algorithm. Inline elements (`strong`, `em`, `code`, `a`, `del`) do carry `data-sourcepos`;
the daemon accepts those too, offsets then being relative to that element.
