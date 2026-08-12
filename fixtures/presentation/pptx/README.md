# PPTX adapter fixtures

- `basic-supported-deck.json` is the strict A-round-trip fixture. It contains only the writer's
  verified surface: page size, ordered slides, basic text, unstyled preset shapes and transforms.
  The `oo-pptx` test suite performs `Deck → PPTX → Deck` and requires an empty `semantic_diff`.
- `rich_text_runs_roundtrip_without_semantic_diff` is the generated strict run-style fixture. It
  covers `a:r/a:rPr` bold, italic, single underline, single strikethrough, font family, font size,
  and RGB/theme colour. It remains generated because the assertion deliberately ignores ZIP part
  ordering and relationship ids while requiring a zero semantic diff.
- Complex run variants are not coerced: double underline/strike, baseline and text alpha are
  reported as unsupported, and the strict writer rejects them. This keeps the fixture boundary
  honest instead of converting them to a visually approximate style.
- `../v5/minimal-deck.json` is intentionally **not** an export fixture. It has a master, layout,
  placeholder and theme metadata, which the current writer must report and the strict writer must
  reject instead of dropping.

Binary PPTX fixtures are generated in tests so relationship ids, package ordering and ZIP metadata
cannot be mistaken for semantic assertions. Tests compare the canonical semantic projection and
also assert that unsafe or unsupported relationship/media input is reported or rejected.
