# I01 acceptance

## Unit and contract evidence

- [x] Selection reducer rejects two simultaneous primary selections.
  Evidence: `test/interaction/selectionReducer.test.ts` — "keeps exactly one primary selection".
- [x] DOM Range conversion round-trips CJK, emoji, links and styled text without offset drift.
  Evidence: `test/interaction/domSelection.test.ts` (jsdom, added 2026-08-26) — round-trips
  across link/styled boundaries, collapsed caret affinity, surrogate-pair-safe emoji
  boundaries, inverted/non-integer rejection and out-of-root selection nullability.
  Real-Chromium composition coverage additionally lives in `e2e/document/unicode-selection.spec.ts`.
- [x] Table selection uses stable IDs and survives a renderer re-render.
  Evidence: `test/tableCommands.test.ts` (added 2026-08-26) — `tableCommandSelection`
  projects cell/row/column/range/all views to stable-id command shapes and is idempotent
  across repeated projections (re-render simulation); browser interactions that select,
  re-render, then command are covered by `e2e/document/table-image.spec.ts` ("expands a
  stable-id table range…").
- [x] Overlay coordinator closes one highest-priority overlay per Escape.
  Evidence: `test/interaction/overlayStore.test.ts` — "dismisses only the topmost priority
  layer on Escape" plus equal-priority registration order.
- [x] Block registry rejects an atomic block definition without object behavior.
  Evidence: `test/registry.test.ts` — "rejects atomic blocks without an explicit interaction behavior".

## Browser evidence

- [x] Clicking a paragraph edits text; it does not create object/table selection.
  Evidence: `e2e/document/boot.spec.ts` + `e2e/document/list.spec.ts` exercise the
  paragraph editing path end to end; single-primary-selection exclusivity is enforced by
  the reducer contract above, so an object/table selection cannot coexist with the caret.
- [x] Clicking a table cell edits that cell; drag/Shift selects a range; toolbar visibility follows the documented selection state.
  Evidence: `e2e/document/table-image.spec.ts` — "inserts a table, edits a cell…" and
  "expands a stable-id table range with surface drag and Shift+Arrow".
- [x] Image selection has one object frame and one toolbar; click outside and Escape clear it.
  Evidence: `table-image.spec.ts` — "selects an inserted image and exposes its object toolbar",
  "explicitly cancels image selection"; outside dismissal arbitrated by OverlayCoordinator
  (`menu.spec.ts` covers the shared outside-pointer path).
- [x] Left/right arrows leave an image to a valid editable position.
  Evidence: `table-image.spec.ts` — "creates caret paragraphs on both object sides"
  (ArrowLeft/ArrowRight via registered imageBehavior).
- [x] Enter before/after image inserts exactly one paragraph at the semantic parent/index.
  Evidence: `table-image.spec.ts` — "Enter after an isolated image creates the following
  paragraph" plus the both-sides caret spec above; assertions count paragraphs instead of
  accepting duplicates.
- [x] Right-click menus remain above selected table/image/callout content and retain the correct target selection.
  Evidence: `e2e/overlays/menu.spec.ts` — portal keeps the context menu topmost over the
  selection layer; target selection retained through the interaction store.
- [x] Light and dark overlay screenshots pass.
  Evidence: `e2e/visual/chrome.spec.ts` dark/light baselines for toolbar and block menu.

## Architecture evidence

- [x] No new generic model patch command exists.
  Evidence: architecture gate forbids `updateBlockPayload`/`updateKind`/`DocumentCommand::UpdateBlock`
  /`document.updateBlock` wire tags; `pnpm architecture:check` passes (128 files).
- [x] No renderer writes `DocumentModel` or table/image payload directly.
  Evidence: gate forbids full-model clones (`structuredClone`) in runtime sources; renderers
  receive readonly projection state plus behavior callbacks (`renderers.tsx`, controller split).
- [x] No duplicate outside-click/Escape global listeners remain in block renderers.
  Evidence: new architecture-gate rule (2026-08-26) prohibits `(window|document).addEventListener`
  in `src/blocks/**` and `src/chrome/**` except four reviewed transient drag/measurement
  adapters (allowlist in `web/scripts/architecture-gate.mjs`); gate passes on current tree.
- [x] `BlockNode.tsx` and `renderers.tsx` meet the code-size budget or a documented exception exists.
  Evidence (2026-08-26): `BlockNode.tsx` 240 lines (<260) after extracting gutter actions to
  `blocks/behaviors/gutterActions.ts`; `renderers.tsx` 517 lines (<850).
- [x] Rust workspace, TypeScript, architecture checks, Vitest, browser and visual suites pass.
  Evidence (2026-08-26): cargo fmt clean, `cargo test --workspace` 272 passed;
  `pnpm typecheck` OK; `pnpm architecture:check` pass; `pnpm test` 562 passed / 80 files;
  `pnpm test:e2e` 14 passed including visual baselines.

## Exit decision

I01 closes with table and image running through the shared interaction contract: keyboard
routing resolves behaviors through the registry, overlay lifecycle is owned by the
coordinator/gate-enforced allowlist, and the old local selection/global-listener paths were
deleted during the migration recorded in `tasks.md`. Remaining follow-ups live in the
capability matrix as `partial` rows (nested-table overlays, cross-block selection unification)
and are tracked by C2 rather than this kernel iteration.
