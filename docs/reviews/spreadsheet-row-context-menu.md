# Spreadsheet row context menu

The Tencent Docs screenshot is the reference for the row-header entry point, operation groups and whole-row behavior.

Implemented:

- Right-click a row header to select that entire row. Shift-click extends to multiple rows; right-click inside that selection preserves it.
- Cut/copy/paste, paste values or formats, insert a configurable number of rows above/below, delete selected rows, clear contents/formats/all, merge and number-format settings.
- Hide/unhide rows, reveal hidden rows using the adjacent header affordance, explicit point-based row heights and reset to automatic font-based height.
- Copy a document/sheet/row-range link and restore the selection when opening it.
- Viewport-clamped menu, scrolling on smaller windows, focus management, Escape and arrow-key navigation.

Row heights and visibility are sparse typed `SheetMetadata.rowLayout` entries. The `spreadsheet.setRowLayout` semantic command updates a bounded range in the Rust spreadsheet engine, validates heights and coordinates, and records an undoable sheet mutation. Insert/delete remaps these entries. Editing layout below an imported used range extends its row count, and undo restores the original extent. The browser consumes the immutable snapshot to derive geometry; it does not keep persistent row state. XLSX import/export preserves explicit heights and hidden rows, including empty rows.

Validation:

- 31 spreadsheet browser tests passed, including four new row-context scenarios for persistence/history, hidden row recovery, multi-row structure edits, clipboard formatting and viewport/keyboard behavior.
- Spreadsheet/XLSX engine tests include row-layout validation, undo/redo, structural remapping and XLSX round-trip.
- Frontend boundary and geometry tests cover invalid/duplicate entries and hidden-row hit testing. Current canonical frontend suite (2026-09-10): 41 unique source files / 283 tests passed; nested workspace `node_modules` are excluded from Vitest discovery.
- Typecheck, canonical Document WASM rebuild, production build and full Rust workspace tests passed.
- Latest backend and frontend are running locally (8787 / 5174); health checks passed and the artifact listing was unchanged after the backend update.

Scope boundaries: this is the functioning row-editing menu, not Tencent's complete product catalog. Screenshot entries for copy-as-image, row grouping, data-validation UI, cell revision UI, plugins/quick tools and range-level permission protection are not implemented here. Number-format settings expose the existing supported presets. Automatic height restores font-based layout; it does not claim Excel's full wrapped-text AutoFit algorithm. Clipboard remains the app's internal clipboard and does not import arbitrary system clipboard tables; relative formula rebasing is not implemented by this menu.
