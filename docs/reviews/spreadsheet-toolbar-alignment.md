# Spreadsheet toolbar alignment — 2026-09-07

Reference: inspected Tencent Docs spreadsheet https://docs.qq.com/sheet/DTlV4VmRGSEtHU0Ri in the browser, including Start, Insert and the row/column menu. Reference sheet contents were not changed.

## Findings and changes

- The old data group allocated two rows but rendered twelve actions. Six row/column actions now live in the existing Start → Insert menu and Insert → Rows/columns menu. All original semantic callbacks remain connected.
- Font group width was fixed at 310–330 px. Groups now size to their contents. Two 26 px rows establish a shared baseline, with 16–18 px icons and 12 px labels.
- The font group hid arbitrary spans, including color glyphs, and the fill glyph had no styling. Icon-only controls now explicitly omit their labels while retaining accessible names. Font and fill controls display the selected color and close their palettes after selection. Border selection also closes its menu.
- Dropdown arrows use inline layout; tile arrows stay adjacent to their icons. Merge is a direct action and no longer advertises a nonexistent menu.
- Insert is divided into pivot, cell/row/column/graphics, pictures, files, and comments. Row/column operations appear immediately after Cells.
- The white ribbon, understated separators, button sizes, hover, selected and focus states follow the reference. Wide layouts expose secondary home shortcuts; narrower layouts keep essential groups and allow horizontal scrolling without shrinking or clipping controls vertically.

## Scope

This is a layout and interaction repair, not a claim of Tencent feature parity. Existing unsupported tools remain disabled. At 900 px horizontal scrolling is needed for the full ribbon. Artifact persistence and engine semantics were not changed.

## Validation

- Typecheck passed.
- Existing spreadsheet browser suite: 19 passed (formatting, sorting, clipboard, row/column insertion, undo, persistence, merge, input, conditional formatting).
- New layout browser suite: 3 passed at 900/1280/1680 px. Checks vertical clipping, button overlap, full-width overflow, reachable row/column menus, and fill palette closing/color feedback.
- Frontend unit suite: 127 files, 1036 tests passed.
- Production build passed; existing bundle-size and presentation CSS warnings remain.
- Browser visual review of Start and Insert at 1280 px completed.

## Functional panel follow-up — 2026-09-07

The user's subsequent screenshots exposed functional gaps beyond ribbon geometry:

- Find previously only exposed Replace All. It counted matches across the sheet but submitted a replacement for the current selection. The new non-modal panel separates Find and Replace, supports previous/next match with cell navigation, current-sheet/frozen-selection scopes, case sensitivity, exact-cell matching, single replacement and atomic batch replacement. Counts use the same targets as replacement, replacement text is literal (including `$&`), and existing cell attributes/styles survive edits. Formula cells are explicitly excluded. Navigation reveals offscreen matches without changing scroll during ordinary formatting.
- Table styles now offer 21 previews: seven colors across alternating row fills, alternating row borders, and alternating column fills. Header row, header column, outline and filter controls produce semantic commands in one undoable batch. Clear is accurately named “清除选区格式”, since these are range appearance presets rather than structured data tables. Custom saved table styles and structured-table conversion remain unavailable.
- Conditional formatting now uses a category menu followed by rule creation or management. All six supported numeric comparisons are selectable; rules can be deleted individually or cleared for the current worksheet. The server currently evaluates only CellIs predicates. Duplicate/blank/top-average/formula/color-scale/data-bar/icon-set rules remain explicitly disabled, rather than appearing to work through a view-only implementation.
- Merge exposes a separate menu with Merge/Unmerge and Merge & Center. Merge & Center submits one atomic batch; Merge Identical Cells remains disabled.
- Gallery and condition popovers opt into a wider overlay instead of inheriting the shared 360 px cap. Browser screenshots were reviewed for the gallery, conditional-format menu/editor and find panel.

Validation: 25 spreadsheet browser tests (including functional panel tests and three layout sizes); frontend unit suite, typecheck and production build. Tests exercise real saved styles, conditional hits, merge geometry, undo, literal replacement, palette clipping and existing spreadsheet operations. Advanced disabled categories are not claimed as feature parity with Tencent Docs.

## Contextual table-style ribbon

Applying a gallery preset now reveals and selects a “表格样式” tab. Its compact ribbon contains header-row/column, outline and filter options, seven color previews with selected state, stripe-pattern selection and Clear Table Style. Edits target the original styled range even when only one cell within it is selected. The contextual entry hides outside that range and returns on re-entry. Checkbox feedback is immediate with rollback on submission failure. Clearing preserves values and number formats.

The context is an editing-session affordance for the most recently styled region; it is not a persisted structured-table object. Formatting is saved in the canonical snapshot, but refreshing the page does not restore this contextual entry. Structured-table metadata, custom-style management and conversion to an ordinary range remain outside this change.

Validation includes a browser flow for automatic tab activation, changing a preset over the original range, updating header options, leaving/re-entering the region and clearing its style. Existing formatting/undo coverage now switches back to Start before using its history buttons.
