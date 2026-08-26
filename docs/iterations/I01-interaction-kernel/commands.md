# I01 command contract

## Production semantic commands

I01 adds no generic `setSelection`, `setAttrs`, `patchBlock`, or renderer command. Selection/pointer state remains client-only. Existing semantic writes remain unchanged:

```text
document.replaceBlockText
document.setBlockPresentation
document.patchInlineRange
document.setImageConfig
document.replaceTableCellText
document.formatTableCells
document.setTableBorders
document.setTableColumnWidth
document.setTableRowHeight
document.mergeTableCells
document.splitTableCells
document.insertBlock / moveBlock / deleteBlock
```

## Required session API additions

The session may add navigation-oriented helpers, but each mutating helper must map to existing semantic commands:

```ts
focusEditableBefore(blockId: string): FocusTarget | null;
focusEditableAfter(blockId: string): FocusTarget | null;
insertParagraphBefore(blockId: string): string | null;
insertParagraphAfter(blockId: string): string | null;
```

`focus*` is UI-only and must not produce a transaction. `insertParagraph*` becomes one `insertBlock` command, with canonical parent/index resolution in the session.

## Server/API impact

None. I01 must not add endpoints or alter snapshot payloads. If a migration reveals missing semantic commands, that command belongs in a separately specified engine iteration, not as an unreviewed frontend workaround.

## Telemetry boundary

Optional local debug instrumentation may log state transitions only in development. It must not emit document text, image bytes, DOM paths or user data.
