# I01 file-level tasks

## I01-01 — interaction types and store — complete

- Create `web/apps/editor/src/interaction/types.ts` with `EditorSelection`, transition event and overlay types.
- Create `web/apps/editor/src/interaction/interactionStore.ts` using `useSyncExternalStore`-compatible subscriptions.
- Create `web/apps/editor/src/interaction/selectionReducer.ts`; reducer is pure and unit-tested.
- Add `web/apps/editor/test/interaction/selectionReducer.test.ts` for exclusivity, invalid state rejection and Escape transitions.

## I01-02 — DOM range adapter — complete

- Create `web/apps/editor/src/interaction/domSelection.ts`.
- Implement `readDomTextSelection(root, blockId)` and `applyDomTextSelection(root, TextRange)`.
- Reuse existing RichText offset rules; do not add a second UTF-16/scalar conversion.
- Update `web/apps/editor/src/blocks/BlockNode.tsx` so it reports focus/input/selection changes to the interaction store instead of inferring selection from `activeBlockId`.
- Add composition and selection collapse tests for CJK/emoji/inline marks.

Progress recorded 2026-08-23:

- Chromium now creates a native UTF-16 range over `甲🙂`, verifies scalar-safe semantic formatting and confirms the visible rich-text run after model reconciliation.
- Focused DOM-first content now compares its parsed RichText runs with the canonical projection: ordinary input with identical text/runs is never rebuilt, while semantic inline formatting is immediately rendered and the browser range can be restored.

## I01-03 — pointer and keyboard routers — in progress

- Create `web/apps/editor/src/interaction/pointerRouter.ts`.
- Create `web/apps/editor/src/interaction/keyboardRouter.ts`.
- Move page-level Escape/outside-click listeners out of `BlockNode.tsx`, image and table components.
- Register one editor root listener in `web/apps/editor/src/Editor.tsx`.
- Preserve native editable behavior unless a behavior handler explicitly consumes the event.

Progress recorded 2026-08-23:

- `BlockEditor` now owns the editor-root `onKeyDownCapture` path through `interaction/keyboardRouter.ts`.
- Root routing resolves the focused atomic block from its stable DOM block id, then dispatches only that block's registered behavior. Editable paragraph and table-cell keystrokes are therefore left to native DOM editing.
- Image `ArrowLeft`/`ArrowRight`/`Enter` navigation is registered through `imageBehavior`, rather than passed as an image-renderer callback. Browser coverage verifies an existing trailing paragraph receives focus on Enter instead of a duplicate paragraph being created.

## I01-04 — overlay coordinator — in progress

- Create `web/apps/editor/src/interaction/OverlayCoordinator.tsx` and `overlayStore.ts`.
- Update `BlockNode.tsx`, `BlockContextMenu.tsx`, `ImageBlockToolbar.tsx`, `TableSelectionToolbar.tsx`, `TableContextMenu.tsx`, `ColorPalette.tsx`, and UI Popover integration to register overlays.
- Add semantic z-index tokens in `web/packages/ui/src/theme.css`; remove editor-local z-index constants from `web/apps/editor/src/styles/blocks.css`.
- Add `web/apps/editor/test/interaction/overlayCoordinator.test.tsx` and `e2e/overlays/stacking.spec.ts`.

Progress recorded 2026-08-23:

- Shared UI tokens now distinguish inline content/control, editor affordance/resize, table selection, selection layer/corner and selection toolbar. Table and image CSS no longer use raw local z-index constants for these layers.
- `OverlayStore` continues to arbitrate priority and outside/Escape dismissal; its focused unit contract passes.
- Chromium verifies a table-cell context menu is portalled outside its block and is the topmost element over the table selection layer.

## I01-05 — extend the block registry — in progress

- Update `web/apps/editor/src/blocks/registry.ts` with `BlockBehavior` types.
- Create `web/apps/editor/src/blocks/behaviors/contentBehavior.ts`.
- Create `web/apps/editor/src/blocks/behaviors/tableBehavior.ts`.
- Create `web/apps/editor/src/blocks/behaviors/imageBehavior.ts`.
- Keep renderer components presentation-oriented; handlers move to behavior modules.
- Add registry validation: each atomic/object block must define selection and keyboard behavior.

Progress recorded 2026-08-23:

- `tableBehavior` now owns the table-selection-to-InteractionStore transition through the registry's `tableSelection` capability. `BlockNode` only forwards the controller's semantic selection event; it no longer contains table selection state rules.

## I01-06 — migrate image behavior — complete

- Replace image-local global keyboard handling in `blocks/object/useObjectBlockKeyboardNavigation.ts` with behavior registration.
- Keep `ImageResizeHandles`, `useImageMove`, and `ImageCropOverlay` as image-specific projection helpers only.
- Route object selection, toolbar visibility and before/after navigation through interaction store.
- Add E2E: select, deselect, move, ArrowLeft, ArrowRight, Enter-before, Enter-after, Escape.

Progress recorded 2026-08-23:

- Chromium coverage now verifies image insertion, object selection, toolbar visibility, ArrowRight insertion after the image and ArrowLeft insertion before it.
- Chromium coverage additionally verifies real pointer movement, south-east resize, crop-mode commit and Escape cancellation in `e2e/document/table-image.spec.ts`.
- The shared image behavior is now invoked by `keyboardRouter.ts`; Chromium verifies ArrowLeft creates/focuses a preceding paragraph when no neighbour exists, Enter inserts/focuses a following paragraph for an isolated image, and Escape explicitly clears the image selection and object toolbar.

## I01-07 — migrate table behavior — in progress

- Update `blocks/table/TableSelectionLayer.tsx` to dispatch `table` selection transitions only.
- Update `TableSelectionToolbar.tsx` to render solely when interaction selection is a qualifying table range or text selection.
- Update `TableContextMenu.tsx` to receive selected stable IDs from the store, not derive them from DOM CSS classes.
- Ensure a cell click starts editing without treating the whole table as selected.
- Add E2E for cell edit, Shift extension, row/column/all selection, context menu and Escape.

Progress recorded 2026-08-23:

- Chromium coverage now exercises cell editing, Shift range selection, merge, split, row selection and the row context menu.
- `table/useTableSelectionController.ts` now owns stable-id normalization/validation, interaction publication, pointer-drag promotion, Shift expansion and Shift+Arrow expansion. `TableBlockView` supplies only the address of a rendered cell and table command callbacks.
- Chromium coverage verifies both surface-drag range expansion and Shift+Arrow range expansion, in addition to cell editing, merge/split and row context actions.

## I01-08 — simplify BlockNode and renderers — in progress

- Reduce `BlockNode.tsx` to DOM projection, block menu trigger and behavior attachment.
- Extract text input event handling to `blocks/behaviors/contentBehavior.ts`.
- Move image/table event branches out of `blocks/renderers.tsx`; renderer only receives readonly interaction state plus behavior callbacks.
- Set a code-size budget: `BlockNode.tsx` under 260 lines and `renderers.tsx` under 850 lines after migration. Any exception requires ADR justification.

Progress recorded 2026-08-23:

- `BlockGutter.tsx` now owns the gutter popover composition; `BlockNode.tsx` is 255 lines and only supplies semantic session callbacks.
- `content/ContentBlockRenderer.tsx` now owns the normal text/todo editable surface, including composition and paste-to-image behavior.
- `table/TableCellView.tsx` now owns the DOM-first cell editing surface. Table selection, geometry, resize and mutation orchestration remain in the table controller.
- `table/useTableGeometry.ts` and `table/useTableResize.ts` now own measurement and adjacent-boundary resize previews; persistent row/column dimensions still commit through `BlockSessionApi` on pointer release.
- `table/commands.ts` now owns stable-id selection projection and inline formatting validation. `renderers.tsx` is now 579 lines; selection gestures no longer reside there.
- `table/useTableCommandController.ts` now owns table menu/toolbar command coordination (format, borders, merge/split, clipboard, row/column insertion and deletion). Both table menus use this same semantic-command path.
- `table/useTableGeometryController.ts` now forms the geometry behavior boundary: it joins DOM-only measurement with transient resize previews and delegates persistent dimensions to `BlockSessionApi` only on pointer release.
- Chromium coverage verifies that a column boundary changes only its adjacent columns and a row boundary changes only the row above it. The geometry/resize path is no longer wired directly from `TableBlockView`.

## I01-09 — remove obsolete paths — pending

- Remove duplicate `window` listeners once coordinator/router owns the lifecycle.
- Remove CSS state selectors that infer domain selection from focus alone.
- Search for `z-index:` in editor styles; every retained declaration must reference a semantic token.
- Extend `web/scripts/architecture-check` rules to prohibit direct `window.addEventListener` in block renderer modules except approved adapters.

Progress recorded 2026-08-23:

- Added `web/vitest.config.ts` so browser scenarios under `apps/editor/e2e` are excluded from the unit-test runner. `pnpm test` no longer imports a second Playwright runtime and now provides a meaningful unit-test gate.
