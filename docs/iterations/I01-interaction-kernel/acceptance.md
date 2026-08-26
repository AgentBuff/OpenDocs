# I01 acceptance

## Unit and contract evidence

- [ ] Selection reducer rejects two simultaneous primary selections.
- [ ] DOM Range conversion round-trips CJK, emoji, links and styled text without offset drift.
- [ ] Table selection uses stable IDs and survives a renderer re-render.
- [ ] Overlay coordinator closes one highest-priority overlay per Escape.
- [ ] Block registry rejects an atomic block definition without object behavior.

## Browser evidence

- [ ] Clicking a paragraph edits text; it does not create object/table selection.
- [ ] Clicking a table cell edits that cell; drag/Shift selects a range; toolbar visibility follows the documented selection state.
- [ ] Image selection has one object frame and one toolbar; click outside and Escape clear it.
- [ ] Left/right arrows leave an image to a valid editable position.
- [ ] Enter before/after image inserts exactly one paragraph at the semantic parent/index.
- [ ] Right-click menus remain above selected table/image/callout content and retain the correct target selection.
- [ ] Light and dark overlay screenshots pass.

## Architecture evidence

- [ ] No new generic model patch command exists.
- [ ] No renderer writes `DocumentModel` or table/image payload directly.
- [ ] No duplicate outside-click/Escape global listeners remain in block renderers.
- [ ] `BlockNode.tsx` and `renderers.tsx` meet the code-size budget or a documented exception exists.
- [ ] Rust workspace, TypeScript, architecture checks, Vitest, browser and visual suites pass.

## Exit decision

I01 closes only when table and image are both running through the shared interaction contract. A component merely importing the store is insufficient; its old local selection and global listener path must be deleted.
