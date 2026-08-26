# Document capability matrix

> Status is evidence-based. `complete` means schema, semantic command, renderer, interaction,
> persistence/history and automated evidence are all available. A visible toolbar item alone is never completion.
> Baseline audited: 2026-08-22. Kind coverage audited against schema v5 on 2026-08-26.

## Block kind coverage

Every current `DocumentBlockKind` must map to a row above or carry an explicit policy.
Audited kinds (`crates/oo-schema/src/lib.rs`): Paragraph, Heading, Quote, Code, Image,
Table, Callout, Todo, Divider, Page, Columns, Column, Link, Extension, Unknown.

| Block kind | Covered by | Note |
| --- | --- | --- |
| Paragraph / Heading | "Paragraph and headings" row | heading levels typed in schema |
| Quote / Callout / Divider / Link | "Quote/callout/divider/link" row | insertion via gutter menu and typed commands |
| Todo | "Todo" row | checkbox renderer + `setTodoChecked` |
| Code | "Code block" row | config + resize UI |
| Image | "Image object" row | object selection/move/resize/crop |
| Table | table rows below | stable row/column/cell IDs |
| Page / Columns / Column | structural container kinds | engine structure commands + schema validation tests; no dedicated toolbar action by design |
| Extension { type_id } | forward-compatible payload | round-trip unit tests keep unknown data intact |
| Unknown { type_id, raw } | preserved raw data | clients that do not understand a future kind still persist it verbatim |

## Toolbar/menu action coverage

Exposed interaction surfaces and their capability rows:

- Gutter insert menu (`BlockGutter`): paragraph/heading/todo/quote/callout/code/divider/link/table/image actions → rows "Paragraph and headings", "Todo", "Quote/callout/divider/link", "Code block", "Image object", table rows. Actions live in `blocks/behaviors/gutterActions.ts`.
- Format toolbar (`BlockToolbar` + toolbar packages): inline style, alignment, list toggle → rows "Inline style", "Block presentation", "Ordered/bullet lists".
- Table selection toolbar + context menu: merge/split, borders, row/column insert/delete → table rows; command path owned by `table/useTableCommandController.ts`.


| Capability | Typed domain / semantic command | UI/read projection | Evidence status | Current result |
| --- | --- | --- | --- | --- |
| Paragraph and headings | `DocumentBlock`, `replaceBlockText`, `convertBlock` | DOM-first block renderer | Rust/TS/Vitest; `e2e/document/boot.spec.ts` | partial — persistence/reload is covered; IME and cross-block coverage remain |
| Inline style | RichText runs, `patchInlineRange` | toolbar and editable projection | Rust/TS unit coverage | partial — range mapping and cross-block selection are not unified |
| Block presentation | `BlockPresentation`, `setBlockPresentation` | toolbar, block menu | Rust protocol test; list E2E | partial — wire-level tri-state clear and list keyboard behavior are covered; visual/other keyboard behavior remains |
| Ordered/bullet lists | `presentation.list`, `setBlockPresentation` | block marker and Enter handler | `e2e/document/list.spec.ts` | partial — Enter continuation and empty-item exit are covered; nested/cross-block/paste behavior remains |
| Todo | `TodoBlock`, `setTodoChecked` | checkbox renderer | engine/schema tests | partial — accessibility and browser persistence need coverage |
| Quote/callout/divider/link | typed kind/data and insertion commands | block menu/renderers | engine/schema/unit tests | partial — no complete interaction/import-export proof |
| Code block | `CodeBlockConfig`, `setCodeConfig` | code editor, resize/config UI | code unit tests | partial — selection/object keyboard and visual suite absent |
| Image object | `ImageBlock`, `setImageConfig` | selection, crop, resize, move, toolbar | image unit tests; `e2e/document/table-image.spec.ts` | partial — behavior routed through registered imageBehavior; crop-commit/Escape-cancel covered in Chromium |
| Table data and formatting | stable row/column/cell IDs; table commands | DOM grid, selection layer, toolbars, menus | engine/schema/table unit tests; `e2e/document/table-image.spec.ts` | partial — cell edit, Shift/drag range expansion, row selection and context menu covered in Chromium; cross-block regressions remain open |
| Table merge/split | `mergeTableCells`, `splitTableCells` | row/col span projection | Rust engine tests; Chromium coverage in `table-image.spec.ts` | partial — reported overlap defects require E2E closure for nested tables |
| Table border/size | border commands, row/column width commands | border menu and resize UI | Rust/TS tests; adjacent-boundary drag coverage in Chromium | partial — visual acceptance missing |
| Context/block menus | no persistent model | `BlockMenu`, `BlockContextMenu`, table menu | `e2e/overlays/menu.spec.ts`, `e2e/visual/chrome.spec.ts` | partial — Escape/outside dismissal and dark block-menu baseline are covered; nested/table overlays remain |
| Undo/redo/history | mutation journal, history transaction | toolbar/history API | server/engine tests | partial — browser conflict/reload/recovery proof incomplete |
| Autosave/outbox | session outbox + artifact transaction | status UI | unit implementation exists | partial — debounce/max-wait/visibility behavior needs E2E and metrics |
| Import/export DOCX | `oo-docx` writer/importer, artifact export route | import/export controls; `x-docx-losses` header | round-trip text/format/image tests; loss-report unit test; `api.rs::docx_export_reports_semantic_losses` | partial — todo state/link target/containers are reported as losses instead of silently dropped; fidelity matrix and visual tests remain |
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
