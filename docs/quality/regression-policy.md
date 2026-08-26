# Document editor regression policy

## Purpose

User-reported interaction defects are product requirements, not one-off visual fixes. Every defect must become a durable automated regression before its fix is considered complete.

## Required lifecycle

1. Reproduce on a disposable canonical Artifact fixture.
2. Classify the failure: `engine`, `schema`, `input-selection`, `object`, `table-grid`, `overlay`, `persistence`, `import-export`, `visual`, or `accessibility`.
3. Add a failing unit test, browser test, or both before changing production code.
4. Make the smallest semantic fix at the owning layer.
5. Prove persistence through revision/snapshot reload whenever a write occurs.
6. Add a visual baseline whenever geometry, layering, color, spacing or contrast changes.
7. Update `document-capability-matrix.md` with the final evidence.

## Prohibited fixes

- Writing directly to React state or the DOM to bypass `DocumentEngine`.
- Adding a generic block payload patch for a typed capability.
- Raising a local z-index without registering the overlay in the shared coordinator.
- Marking the issue complete based on a manually observed screenshot only.
- Leaving an old event listener or compatibility branch active after a replacement is introduced.

## Severity guide

| Severity | Definition | Release effect |
| --- | --- | --- |
| P0 | Data loss, incorrect persisted document structure, undo/history corruption | blocks release |
| P1 | Core edit/selection/menu/overlay interaction unavailable or misleading | blocks capability completion |
| P2 | Major visual or accessibility inconsistency with a working workaround | must be scheduled before release candidate |
| P3 | Minor visual polish issue | tracked with visual regression |
