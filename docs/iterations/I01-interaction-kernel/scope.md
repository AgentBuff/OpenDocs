# I01 — unified Document interaction kernel

## Objective

Replace per-block selection, pointer, keyboard and overlay patches with a shared editor interaction runtime. I01 does not attempt to complete every table or image feature; it makes those features implementable without duplicating event logic.

## In scope

- One typed editor selection state for text, blocks, objects and table ranges.
- DOM selection mapping with explicit ownership and validity rules.
- Unified pointer/keyboard routing and object navigation.
- A block behavior contract that owns interaction metadata without owning domain data.
- Central overlay coordination and z-index policy.
- Migration of ordinary text blocks, table and image to the new contract.

## Out of scope

- CRDT/OT collaboration.
- New Document block kinds or generic `attrs` fields.
- Page layout/word wrapping engine.
- Completing crop UI, table border UI or all context menu commands beyond migration safety.

## Dependencies

- I00 browser suite must exist; I01 may land only with interaction regressions.
- Existing `DocumentCommandBatch` remains the sole write path.
- Existing block model and table stable IDs remain canonical.

## Non-negotiable invariants

1. Interaction state is ephemeral client state; it is never persisted in `DocumentModel`.
2. DOM selection is an input/output projection, not the document truth.
3. A renderer cannot write domain state directly; it invokes `BlockSessionApi` semantic methods.
4. At most one primary selection exists at a time. Submenu/hover state is not selection.
5. Only `OverlayCoordinator` assigns overlay layer and handles outside-click/Escape arbitration.
