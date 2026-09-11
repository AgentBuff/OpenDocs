# Unified real fonts across artifacts

Goal: unify font options across Document, Spreadsheet, Presentation, Mindmap and Whiteboard; broaden open-source coverage; every offered family must load matching font files and preview its own face.

Status: implemented and verified for all offered font families and all five artifact editors.

## Implemented (2026-09-08)

- One catalog and searchable, grouped picker across all five editors. The picker previews each family, labels script coverage, supports keyboard navigation, loads the actual font before saving a choice, and allows retry without changing the saved font on failure.
- 114 version-pinned open-source families, including 15 Chinese families, Japanese, Korean, Arabic, Hebrew, Devanagari and Thai. 4,442 WOFF2 files, 143,840,652 bytes. Assets are local and lazy-loaded; each family includes its license. Metadata locks, hashes and repeatable acquisition/integrity scripts are in `web/scripts/fonts`.
- Fontsource metadata was cross-checked with actual pinned package contents because metadata can list nonexistent weight/style combinations. Such entries are excluded rather than replaced with fallback files.
- Reopening documents registers persisted fonts. Presentation text, table cells, playback and text thumbnails consume rich-text fonts. EMU coordinates are converted correctly to point sizes; editing text preserves Unicode run styles.
- Mindmap DOM text measurements wait for the chosen fonts, respect node width/border constraints and wrapping, and feed the canonical Rust layout/edge projection. Measurements remain ephemeral view input; the snapshot is unchanged. Image space is reserved during measurement.
- Whiteboard has a basic text-object editor using canonical Scene Graph commands. Text and font saves are serialized, merge against the current revision, and retain unrelated attributes.
- DOCX/PPTX export resolves CSS stacks to native family names, including East Asian and complex-script fields. XLSX imports/exports font family, size, bold, italic, underline, strike and RGB color; unformatted cells retain a neutral default style. Unsupported theme/indexed colors and advanced font effects remain in the import loss report.
- Presentation table-cell font selection, content-edit retention, playback and thumbnails use the same font settings; the final browser verification passed.

## Verification

- Offline integrity: all 114 families, 4,442 files, licenses and local CSS references passed.
- Browser: all 114 matching FontFace families load; every individual WOFF2 file decodes. The 44-test pass includes Document persistence, all prior Spreadsheet/Mindmap regressions, every artifact font path, long Chinese wrapping, font-download failure and keyboard retry, and whiteboard draft retention.
- Follow-up checks passed for Presentation thumbnail font restoration and Mindmap image/wrapping measurements after final refinements.
- Frontend: 128 unit files / 1,038 tests passed; canonical WASM builds, typecheck and production build passed. Targeted Presentation/font tests passed after thumbnail changes.
- Rust workspace: 351 tests passed, including new font export round trips and renderer-measurement immutability checks. The final XLSX refinements also passed its seven tests.
- Final checks passed: all five font-surface browser scenarios (including Presentation table cells and retry), typecheck, 41 targeted unit tests and production build. The earlier full regression covered 44 scenarios; adding table cells brings the covered scenario set to 45.

## Boundaries

- This is a broad, extensible collection of 114 open-source families, not every open-source font project in existence. Adding a family requires its actual assets, license, metadata and the same loading checks.
- Each font covers its own scripts. Browser fallback handles characters absent from that font; the picker labels primary script coverage.
- Office exports preserve native font names and supported styling. Fonts are not embedded into DOCX/PPTX/XLSX; a receiving desktop application needs the named fonts installed for identical glyphs.
- The whiteboard entry supplies text editing for this typography goal; it is not a claim that every whiteboard drawing tool is implemented.
- No persistence schema fields changed in this font work. The new schema-side family-name helper is an export utility; browser model fields and snapshot versions remain unchanged. Mindmap measurements are read-only projection input.

Reference sources: https://fontsource.org/docs/api/introduction and https://fontsource.org/docs/getting-started/variable. Exact family provenance and versions are recorded in the locked catalog and local manifest.
