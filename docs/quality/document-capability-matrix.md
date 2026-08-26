# Document capability matrix

> Status is evidence-based. `complete` means schema, semantic command, renderer, interaction,
> persistence/history and automated evidence are all available. A visible toolbar item alone is never completion.
> Baseline audited: 2026-08-22.

| Capability | Typed domain / semantic command | UI/read projection | Evidence status | Current result |
| --- | --- | --- | --- | --- |
| Paragraph and headings | `DocumentBlock`, `replaceBlockText`, `convertBlock` | DOM-first block renderer | Rust/TS/Vitest; `e2e/document/boot.spec.ts` | partial — persistence/reload is covered; IME and cross-block coverage remain |
| Inline style | RichText runs, `patchInlineRange` | toolbar and editable projection | Rust/TS unit coverage | partial — range mapping and cross-block selection are not unified |
| Block presentation | `BlockPresentation`, `setBlockPresentation` | toolbar, block menu | Rust protocol test; list E2E | partial — wire-level tri-state clear and list keyboard behavior are covered; visual/other keyboard behavior remains |
| Ordered/bullet lists | `presentation.list`, `setBlockPresentation` | block marker and Enter handler | `e2e/document/list.spec.ts` | partial — Enter continuation and empty-item exit are covered; nested/cross-block/paste behavior remains |
| Todo | `TodoBlock`, `setTodoChecked` | checkbox renderer | engine/schema tests | partial — accessibility and browser persistence need coverage |
| Quote/callout/divider/link | typed kind/data and insertion commands | block menu/renderers | engine/schema/unit tests | partial — no complete interaction/import-export proof |
| Code block | `CodeBlockConfig`, `setCodeConfig` | code editor, resize/config UI | code unit tests | partial — selection/object keyboard and visual suite absent |
| Image object | `ImageBlock`, `setImageConfig` | selection, crop, resize, move, toolbar | image unit tests | partial — behavior is split across local handlers; E2E pending |
| Table data and formatting | stable row/column/cell IDs; table commands | DOM grid, selection layer, toolbars, menus | engine/schema/table unit tests | partial — selection/merge/menu/resize regressions remain open |
| Table merge/split | `mergeTableCells`, `splitTableCells` | row/col span projection | Rust engine tests | partial — reported overlap and UI selection defects require E2E closure |
| Table border/size | border commands, row/column width commands | border menu and resize UI | Rust/TS tests | partial — real pointer drag and visual acceptance missing |
| Context/block menus | no persistent model | `BlockMenu`, `BlockContextMenu`, table menu | `e2e/overlays/menu.spec.ts`, `e2e/visual/chrome.spec.ts` | partial — Escape/outside dismissal and dark block-menu baseline are covered; nested/table overlays remain |
| Undo/redo/history | mutation journal, history transaction | toolbar/history API | server/engine tests | partial — browser conflict/reload/recovery proof incomplete |
| Autosave/outbox | session outbox + artifact transaction | status UI | unit implementation exists | partial — debounce/max-wait/visibility behavior needs E2E and metrics |
| Import/export DOCX | `oo-docx`, artifact import/export routes | import/export controls | adapter tests | partial — fidelity matrix and round-trip visual tests absent |
| Outline/projections/events | artifact projection/event APIs | API client boundaries | server/API tests | partial — editor and external consumer acceptance incomplete |
| Presence | ephemeral presence API | no mature Document UI | route/module exists | planned — not real-time collaboration |
| Comments/@mentions/suggestions | no canonical Document domain model | no complete UI | none | planned |
| Find/replace, TOC, headers/footers, footnotes | no canonical domain model | no complete UI | none | planned |
| Permissions, sharing, audit | metadata seam only | no product UI | none | planned |
| Real-time coediting/rebase | revision/events/presence primitives | no remote cursor/edit runtime | none | planned |

## Source anchors

- Schema: `web/packages/schema/src/artifact.ts`
- Engine: `crates/oo-document/src/lib.rs`, `crates/oo-document/src/table_grid.rs`
- Server command/API boundary: `crates/oo-server/src/artifact_routes.rs`, `crates/oo-server/src/routes.rs`
- Editor: `web/apps/editor/src/blocks/`, `web/apps/editor/src/hooks/useBlockSession.ts`
- Existing unit tests: `web/apps/editor/test/`, Rust crate tests.

## Rule for updates

When a capability changes, update this row in the same change set. The pull request must reference the exact new test names; a claim may move from `partial` to `complete` only after all I00 acceptance gates pass.
