# Release plan

Select any text and right-click to leave a comment. Comments are stored in this file as HTML comments.

We should <!--c:k3f9-->ship on Friday<!--/c:k3f9--> regardless of the QA timeline, because the release window closes Monday.
<!--co:k3f9 by:kyle at:2026-06-17T10:00:00.000Z status:open quote:"ship on Friday"
kyle (2026-06-17T10:00:00.000Z): I thought we agreed Thursday?
sam (2026-06-17T10:05:00.000Z): Thursday is better for QA.
+👍 sam, mike
-->

Here is a second paragraph with <!--c:a7b2-->another highlighted span<!--/c:a7b2--> placed close to the first.
<!--co:a7b2 by:sam at:2026-06-17T10:05:00.000Z status:open quote:"another highlighted span"
sam (2026-06-17T10:05:00.000Z): Two anchors near each other.
-->

And a third point that is already <!--c:zz1q-->resolved<!--/c:zz1q-->.
<!--co:zz1q by:kyle at:2026-06-17T09:00:00.000Z status:resolved quote:"resolved"
kyle (2026-06-17T09:00:00.000Z): Handled.
-->

## Code

<!--c:p7k2-->
```python
def process(order):
    validate(order)
```
<!--/c:p7k2-->
<!--co:p7k2 by:agent status:open quote:"    validate(order)" line:1
agent: should this run before logging?
-->

This body has no anchor any more.
<!--co:orph1 by:me at:2026-06-17T12:00:00.000Z status:open quote:"deleted text"
me (2026-06-17T12:00:00.000Z): orphaned thread
-->

Example syntax in a fence is ignored:

```markdown
<!--c:ex01-->example span<!--/c:ex01-->
<!--co:ex01 status:open
me: not a real comment
-->
```

## Diagrams

Mermaid fences render as diagrams. Hover one for **Comment** / **Source**; click it to zoom.

### Flowchart

<!--c:mm01-->
```mermaid
flowchart LR
  subgraph Browser
    A[Reviewer] -->|selects text| B(Annotator UI)
  end
  subgraph Daemon
    C{Valid comment?} -->|yes| D[Write markers into the file]
    C -->|no| E[Show an error]
  end
  B -->|WebSocket intent| C
  D -->|Doc update| B
  classDef ok fill:#dcfce7,stroke:#15803d
  class D ok
```
<!--/c:mm01-->
<!--co:mm01 by:agent at:2026-06-17T11:00:00.000Z status:open quote:"flowchart LR" line:0-11
agent (2026-06-17T11:00:00.000Z): Should the error path also notify the agent?
-->

### Sequence diagram

```mermaid
sequenceDiagram
  autonumber
  actor R as Reviewer
  participant UI as Browser UI
  participant D as Daemon
  participant F as File
  R->>UI: Select text, add comment
  UI->>D: add_comment
  activate D
  D->>F: Insert markers
  Note right of F: Comments live in the Markdown file
  D-->>UI: Doc (html + comments)
  deactivate D
  UI-->>R: Highlight and sidebar card
```

### Class diagram

```mermaid
classDiagram
  class Session {
    +String id
    +int round
    +open_round()
    +finish(done)
  }
  class DocState {
    +String html
    +CommentView[] comments
  }
  class CommentView {
    +String id
    +String status
    +String block_sourcepos
  }
  Session --> DocState : builds
  DocState "1" *-- "many" CommentView
```

### State diagram

```mermaid
stateDiagram-v2
  [*] --> Active
  Active --> Closed : Done
  Closed --> Active : Agent starts next round
  Active --> Abandoned : Abandon
  Abandoned --> [*]
  state Active {
    [*] --> Reviewing
    Reviewing --> Editing : switch to Source
    Editing --> Reviewing : autosave
  }
```

### ER diagram

```mermaid
erDiagram
  SESSION ||--o{ ROUND : has
  ROUND ||--o{ THREAD : contains
  THREAD ||--|{ ENTRY : holds
  THREAD {
    string id PK
    string status
    string quote
  }
  ENTRY {
    string author
    string text
  }
```

### Pie chart

```mermaid
pie showData title Review comments by kind
  "Anchored text" : 24
  "Code blocks" : 9
  "Orphaned" : 3
  "Replies" : 17
```

### Gantt chart

```mermaid
gantt
  title Review plan
  dateFormat YYYY-MM-DD
  section Draft
  Write plan      :done, a1, 2026-06-01, 4d
  Review round 1  :active, a2, after a1, 3d
  section Ship
  Fix findings    :crit, b1, after a2, 4d
  Release         :milestone, b2, after b1, 0d
```

### An invalid diagram

```mermaid
flowchart LR
  A --> B
  B -->> C
  C[unclosed
```
