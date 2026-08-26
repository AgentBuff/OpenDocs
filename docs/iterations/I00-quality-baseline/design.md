# I00 design

## Capability evidence model

The matrix has one row per capability and the following required columns:

| Column | Meaning |
| --- | --- |
| Domain model | typed schema field/payload that persists the feature |
| Semantic write | engine command and server `typeId` |
| Read projection | renderer and, when relevant, API projection |
| Interaction | pointer, keyboard and selection behavior |
| History | undo/redo, revision and refresh behavior |
| Import/export | explicit support or explicit loss report |
| Automated evidence | Rust/TS/browser/visual test names |
| Status | complete / partial / planned / blocked |

The first inventory covers paragraph/rich text, headings, inline formatting, list behavior, block insertion, context menus, image, table, code, todo, link, quote, callout, divider, document history, import, export and autosave.

## Browser test topology

```text
Playwright worker
  ├─ starts oo-server with isolated test database/blob directory
  ├─ starts Vite with API proxy to that server
  ├─ creates fixture artifact through /api/artifacts
  ├─ exercises browser interaction
  └─ verifies DOM + GET snapshot/revision/events
```

The isolated database and blob directory are per worker. Cleanup may only remove paths created under the test temp root.

## Test categories

- `smoke`: editor boot, load, focus and one persisted text change.
- `interaction`: keyboard, pointer, selection and overlay rules.
- `persistence`: refresh/reload, undo/redo, conflict and autosave behavior.
- `visual`: viewport-stable screenshot tests with animation disabled.
- `regression`: a user-reported defect first encoded as a failing browser test.

## Screenshot policy

Screenshots are deterministic only when test font, viewport, color scheme, device scale factor, data fixture and animation state are pinned. Baselines must not include timestamps, random document titles, cursor blinking or network-dependent images.
