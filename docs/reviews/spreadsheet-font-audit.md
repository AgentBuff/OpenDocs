# Spreadsheet font audit — 2026-09-09

Scope: the web spreadsheet editor and XLSX font import/export, not a running Microsoft Excel application or a user-supplied workbook.

Found and repaired:

1. Editing a formatted cell switched to Arial and 13 px. A browser regression reproduced the font-family mismatch. The edit overlay now inherits font family, size, weight, decoration, alignment and color from the cell.
2. Fixed 28 px rows clipped large text. Sparse, renderer-only row metrics now grow rows according to their font sizes and consistently update row headers, selection rectangles, merged-cell bounds, virtualized windows and keyboard/search reveal positions. Other rows retain their normal height; no domain content is changed by layout.
3. XLSX cells without an `s` attribute were treated as unformatted. OOXML defines them as using cellXfs[0]. The importer now preserves that default font, while keeping genuinely neutral cells neutral. Explicit per-cell styles still override the default.

Checks:

- 27 spreadsheet browser scenarios passed, including a real Noto Serif SC font load, 24 pt / 32 px editing, bold, undo/redo, reload and a 50 px row height. Prior merged-cell, range-formatting, filtering, toolbar and search checks passed.
- XLSX: eight tests passed, including default-font inheritance and font family/size/decorations/RGB export-import preservation.
- Frontend: 129 unit files / 1,039 tests, typecheck, canonical Document WASM rebuild and production build passed.
- Full Rust workspace tests and formatting checks passed.
- Rebuilt and restarted the local backend on port 8787; health is OK and the artifact listing is unchanged.
- New row geometry test checks boundaries and sparse lookup for a million-row sheet.

Limitations: fonts not bundled or installed locally still use browser fallback. XLSX does not embed font files; recipients need the named font installed. This audit does not claim full support for every Excel theme/indexed-color or row/column-style inheritance feature.
