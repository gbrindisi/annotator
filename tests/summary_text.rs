//! `summary::to_text` output.

use annotator::summary::{compute, to_text};
use pretty_assertions::assert_eq;

#[test]
fn no_changes() {
    let s = compute("/p.md", 1, "x\n", "x\n");
    assert_eq!(
        to_text(&s),
        "annotator: review of /p.md finished (round 1)\n- no changes\n"
    );
}

#[test]
fn representative_round() {
    let before = "\
# Plan

We ship on Friday regardless of QA.

```rust
fn run(order: Order) {
    validate(order)
    log(order)
}
```

The release window is open.
<!--c:zz9q-->the release window<!--/c:zz9q-->
<!--co:zz9q by:agent status:open
agent: ok?
-->

<!--c:p7k2-->
```rust
fn go() {
    validate(order)
    log(order)
}
```
<!--/c:p7k2-->
<!--co:p7k2 by:agent status:open quote:\"    validate(order)\" line:1-2
agent: order?
-->

Old closing paragraph.
Tail one.
Tail two.
Tail three.
Tail four.
";
    let after = "\
# Plan

We <!--c:k3f9-->ship on Friday<!--/c:k3f9--> regardless of QA.
<!--co:k3f9 by:bomber at:2026-09-24T14:30:00.000Z status:open quote:\"ship on Friday\"
bomber (2026-09-24T14:30:00.000Z): I thought we agreed Thursday?
Or at least Wednesday.
-->

```rust
fn run(order: Order) {
    validate(order)
    log(order)
}
```

The release window is open.
<!--c:zz9q-->the release window<!--/c:zz9q-->
<!--co:zz9q by:agent status:resolved
agent: ok?
-->

<!--c:p7k2-->
```rust
fn go() {
    validate(order)
    log(order)
}
```
<!--/c:p7k2-->
<!--co:p7k2 by:agent status:open quote:\"    validate(order)\" line:1-2
agent: order?
bomber (2026-09-24T14:31:00.000Z): yes, before logging
-->

New closing paragraph.
Tail one.
Tail two.
Tail three.
Tail four.
";
    let s = compute("/abs/plan.md", 2, before, after);
    let expected = "\
annotator: review of /abs/plan.md finished (round 2)

## Comments (1 new, 1 reply, 1 resolved)
[k3f9] new on \"ship on Friday\"
  bomber: I thought we agreed Thursday?
    Or at least Wednesday.
[zz9q] resolved on \"the release window\"
[p7k2] reply on code lines 2-3 (\"validate(order)\")
  bomber: yes, before logging

## Edits (1 hunk, +1 -1)
--- plan.md (round 2 start)
+++ plan.md (after review)
@@ -19,7 +19,7 @@
 }
 ```
 
-Old closing paragraph.
+New closing paragraph.
 Tail one.
 Tail two.
 Tail three.
";
    assert_eq!(to_text(&s), expected);
}

#[test]
fn deleted_and_edited_show_entries() {
    let before =
        "a <!--c:d1-->gone<!--/c:d1-->\n<!--co:d1 by:a status:open quote:\"gone\"\nme: bye\n-->\n";
    let s = compute("/p.md", 1, before, "a gone\n");
    assert_eq!(
        to_text(&s),
        "annotator: review of /p.md finished (round 1)\n\n## Comments (1 deleted)\n[d1] deleted on \"gone\"\n  me: bye\n"
    );
}
