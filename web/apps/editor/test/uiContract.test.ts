import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const sourceFile = (relative: string) => fileURLToPath(new URL(relative, import.meta.url));
const readSource = (relative: string) => readFileSync(sourceFile(relative), "utf8");

const responsiveCss = readSource("../src/styles/responsive.css");
const blockToolbar = readSource("../src/chrome/BlockToolbar.tsx");
const paragraphSettingsDialog = readSource("../src/chrome/ParagraphSettingsDialog.tsx");
const pageSetupPanel = readSource("../src/chrome/PageSetupPanel.tsx");
const blockMenu = readSource("../src/blocks/BlockMenu.tsx");
const blockGutter = readSource("../src/blocks/BlockGutter.tsx");
const blockContextMenu = readSource("../src/blocks/BlockContextMenu.tsx");
const blockNode = readSource("../src/blocks/BlockNode.tsx");
const blockEditor = readSource("../src/blockEditor.tsx");
const blockSession = readSource("../src/hooks/useBlockSession.ts");
const colorPalette = readSource("../src/chrome/ColorPalette.tsx");
const builtinIcons = readSource("../../../packages/ui/src/icons/builtin.tsx");
const toolbarItems = readSource("../src/toolbar/items.ts");
const toolbarSelectionState = readSource("../src/toolbar/selectionState.ts");
const insertMenu = readSource("../src/toolbar/InsertMenu.tsx");
const themeCss = readSource("../../../packages/ui/src/styles/theme.css");
const toolbarCss = readSource("../../../packages/ui/src/styles/toolbar.css");
const toolbarPrimitive = readSource("../../../packages/ui/src/navigation/toolbar.tsx");
const editorShellCss = readSource("../src/styles/editor-shell.css");
const codeBlockView = readSource("../src/blocks/code/CodeBlockView.tsx");
const renderers = readSource("../src/blocks/renderers.tsx");
const contentRenderer = readSource("../src/blocks/content/ContentBlockRenderer.tsx");
const tableCellView = readSource("../src/blocks/table/TableCellView.tsx");
const tableSelectionController = readSource("../src/blocks/table/useTableSelectionController.ts");
const tableGeometryController = readSource("../src/blocks/table/useTableGeometryController.ts");
const tableSelectionLayer = readSource("../src/blocks/table/TableSelectionLayer.tsx");
const tableSelectionToolbar = readSource("../src/blocks/table/TableSelectionToolbar.tsx");
const tableBehavior = readSource("../src/blocks/behaviors/tableBehavior.ts");
const tableToolbarContract = readSource("../src/blocks/table/toolbarContract.ts");
const tableContextMenu = readSource("../src/blocks/table/TableContextMenu.tsx");
const tableModel = readSource("../src/blocks/table/model.ts");
const tableProjection = readSource("../src/blocks/table/projection.ts");
const blocksCss = readSource("../src/styles/blocks.css");
const blockSelection = readSource("../src/utils/blockSelection.ts");
const richText = readSource("../src/blocks/richText.ts");

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = `${directory}/${entry.name}`;
    if (entry.isDirectory()) return sourceFiles(path);
    return /\.(css|ts|tsx)$/.test(entry.name) ? [path] : [];
  });
}

describe("editor UI contracts", () => {
  it("keeps the compact toolbar usable on narrow viewports", () => {
    expect(responsiveCss).toMatch(/@media \(max-width: 960px\)/);
    expect(responsiveCss).toContain(".oo-toolbar__row {");
    expect(responsiveCss).toContain("overflow-x: auto;");
    expect(responsiveCss).toContain("scrollbar-width: none;");
    expect(responsiveCss).toContain(".oo-toolbar__row::-webkit-scrollbar");
    expect(responsiveCss).toContain(".toolbar__utility-button-label { display: none; }");
  });

  it("keeps utility actions icon-backed while preserving accessible labels", () => {
    expect(blockToolbar).toContain('<Icon name="history" />');
    expect(blockToolbar).toContain('<Icon name="download" />');
    expect(blockToolbar).toContain('<Icon name="settings" />');
    expect(blockToolbar).not.toMatch(/(?:HistoryIcon|DownloadIcon|SettingsIcon|HighlightIcon|LineHeightIcon)/);
    expect(blockToolbar).toContain('aria-label="历史版本"');
    expect(blockToolbar).toContain('aria-label="导出 DOCX"');
    expect(blockToolbar).toContain('aria-label="页面设置"');
    expect(blockToolbar).toContain('className="toolbar__utility-button"');
  });

  it("keeps the insert trigger and popup discoverable to assistive technology", () => {
    expect(insertMenu).toContain('aria-label="插入内容"');
    expect(insertMenu).toContain('aria-haspopup="menu"');
    expect(insertMenu).toContain('role="menu" aria-label="插入内容"');
    expect(insertMenu).toContain("<Icon name=\"arrow-down\" className=\"toolbar__insert-chevron\" />");
    expect(insertMenu).not.toContain("⌄");
    expect(insertMenu).not.toMatch(/aria-label="插入"/);
    expect(insertMenu).not.toMatch(/from "\.\.\/icons\/index\.js"/);
  });

  it("keeps block controls on the shared icon registry", () => {
    expect(blockMenu).toContain('left: "align-left"');
    expect(blockMenu).toContain('<Icon name={alignIcons[value]} />');
    expect(blockMenu).toContain('<Icon name="bullet-list" />');
    expect(blockMenu).toContain('<Icon name="delete" />');
    expect(blockMenu).not.toMatch(/from "\.\.\/icons\/index\.js"/);
    expect(blockGutter).toContain('<Icon name="block-handle" />');
    expect(blockGutter).toContain('<Icon name="insert" />');
    expect(blockGutter).not.toContain("BlockHandleIcon");
  });

  it("keeps the color surface structured like the product palette", () => {
    expect(colorPalette).toContain('role="menu"');
    expect(colorPalette).toContain("toolbar-color-theme-grid");
    expect(colorPalette).toContain("toolbar-color-standard-row");
    expect(colorPalette).toContain("toolbar-color-recent-row");
    expect(colorPalette).toContain("更多颜色");
    expect(colorPalette).toContain('type="color"');
    expect(colorPalette).toContain('name="arrow-right"');
    expect(blockToolbar).toContain('"--oo-toolbar-color-value"');
    expect(builtinIcons).not.toContain('M9 41H39');
    expect(builtinIcons).not.toContain('M6 42H42');
  });

  it("keeps toolbar icon semantics explicit", () => {
    expect(toolbarItems).toContain('label: "格式刷", icon: "brush"');
    expect(toolbarItems).toContain('label: "清除格式", icon: "eraser"');
    expect(toolbarItems).not.toContain('icon: "format-painter"');
    expect(toolbarItems).not.toContain('icon: "clear-format"');
    expect(blockToolbar).toContain('<Icon name="font-colors" />');
    expect(blockToolbar).toContain('<Icon name="highlight" />');
    expect(blockToolbar).not.toContain('primaryContent={<Icon name="bg-colors" />}');
    expect(blockToolbar).not.toContain('<Icon name="font-color" />');
    expect(blockToolbar).not.toContain('<Icon name="background-brush" />');
  });

  it("keeps block insertion and paragraph settings in the toolbar, not destructive chrome", () => {
    expect(blockToolbar).toContain('aria-label="插入引用"');
    expect(blockToolbar).toContain('aria-label="插入高亮块"');
    expect(blockToolbar).toContain('aria-label="段落设置"');
    expect(blockToolbar).not.toContain('action: "delete"');
    expect(paragraphSettingsDialog).toContain('role="dialog"');
    expect(paragraphSettingsDialog).toContain("缩进和间距");
    expect(paragraphSettingsDialog).toContain("spacingBefore");
    expect(paragraphSettingsDialog).toContain("spacingAfter");
    expect(paragraphSettingsDialog).toContain("lineHeight");
  });

  it("keeps toolbar select labels ahead of the disclosure icon", () => {
    expect(toolbarCss).toContain("min-width: max-content;");
    expect(toolbarCss).toContain(".oo-toolbar-select__value { min-width: 0;");
    expect(toolbarCss).toContain(".oo-toolbar-select__arrow");
    expect(blockToolbar).toContain('popupClassName="oo-toolbar-select__popover--line-height"');
    expect(blockToolbar).toContain("1.3");
    expect(editorShellCss).toContain(".oo-toolbar-select__popover--line-height");
  });

  it("derives toolbar active state from native ranges without creating a second model", () => {
    expect(toolbarSelectionState).toContain("readBlockTextSelection");
    expect(toolbarSelectionState).toContain("hasTextSelection: true");
    expect(toolbarSelectionState).toContain("styles.every");
    expect(toolbarSelectionState).toContain("commonValue");
    expect(toolbarSelectionState).not.toContain("DocumentCommand");
    expect(toolbarSelectionState).not.toContain("updateBlock");
    expect(blockToolbar).toContain("hasTextSelection");
    expect(blockSession).toContain('type: "patchInlineRange"');
  });

  it("keeps page setup orientation on the themed controlled listbox", () => {
    expect(pageSetupPanel).toContain("ToolbarSelect");
    expect(pageSetupPanel).toContain('aria-label="纸张方向"');
    expect(pageSetupPanel).toContain("onValueChange");
    expect(pageSetupPanel).toContain('value={landscape ? "landscape" : "portrait"}');
    expect(pageSetupPanel).not.toContain("<select");
    expect(pageSetupPanel).not.toContain("<option");
    expect(editorShellCss).toContain(".page-setup-panel .oo-toolbar__select");
  });

  it("keeps the canonical dark and compact theme contract available", () => {
    expect(themeCss).toContain(".oo-theme--office-dark");
    expect(themeCss).toContain("color-scheme: dark;");
    expect(themeCss).toContain('.oo-theme-root[data-density="compact"]');
    expect(themeCss).toContain("--oo-toolbar-height: 40px;");
  });

  it("keeps code blocks aligned with the document-card interaction contract", () => {
    expect(codeBlockView).toContain('placeholder="请输入代码块名称"');
    expect(codeBlockView).toContain('aria-label="代码主题"');
    expect(codeBlockView).toContain('<Icon name={copied ? "check" : "copy"} />');
    expect(codeBlockView).toContain('<Icon name="ellipsis" />');
    expect(codeBlockView).toContain('<Icon name="delete" />');
    expect(blocksCss).toContain(".code-block:focus-within");
    expect(blocksCss).toContain(".code-block__toolbar::-webkit-scrollbar");
    expect(blocksCss).toContain(".code-block__select--theme");
    expect(codeBlockView).toContain('aria-label="调整代码块高度"');
    expect(codeBlockView).toContain("CODE_BLOCK_MAX_HEIGHT");
    expect(blocksCss).toContain(".code-block__resize-handle");
    expect(blocksCss).toContain("cursor: ns-resize");
    expect(blocksCss).not.toContain("border-top-color: var(--oo-color-border-focus)");
    expect(codeBlockView).toContain("code-block__tool--copy");
    expect(blocksCss).toContain("min-height: 160px");
    expect(blocksCss).toContain("scrollbar-color");
  });

  it("keeps code settings on the themed select primitive", () => {
    expect(codeBlockView).toContain('aria-label="设置代码主题"');
    expect(codeBlockView).toContain('aria-label="缩进模式"');
    expect(codeBlockView).toContain('aria-label="缩进宽度"');
    expect(codeBlockView).toContain("code-block__settings-select");
    expect(codeBlockView).not.toMatch(/<select\b/);
    expect(codeBlockView).not.toMatch(/<option\b/);
    expect(blocksCss).not.toContain(".code-block__settings select");
  });

  it("keeps table insertion affordances out of the document flow", () => {
    expect(renderers).toContain('className="block-table__affordance block-table__row-affordance"');
    expect(renderers).toContain('className="block-table__affordance block-table__column-affordance"');
    expect(renderers).toContain("rowBoundaries.map");
    expect(renderers).toContain("columnBoundaries.map");
    expect(renderers).toContain("insertTableRow(block.id, boundaryIndex)");
    expect(renderers).toContain("insertTableColumn(block.id, boundaryIndex)");
    expect(renderers).toContain('<Icon name="plus" />');
    expect(blocksCss).toContain(".block-table-wrap {\n  position: relative;");
    expect(blocksCss).toContain(".block-table__controls {\n  position: absolute;");
    expect(blocksCss).toContain(".block-table__affordance:hover");
    expect(blocksCss).toContain("pointer-events: auto;");
    expect(renderers).not.toContain("＋ 添加行");
    expect(renderers).not.toContain("＋ 添加列");
  });

  it("keeps table row and column selection in a dedicated interaction layer", () => {
    expect(renderers).toContain("<TableSelectionLayer");
    expect(tableSelectionLayer).toContain('className="block-table__selection-layer"');
    expect(tableSelectionLayer).toContain("block-table__row-selector");
    expect(tableSelectionLayer).toContain("block-table__column-selector");
    expect(tableSelectionLayer).toContain("block-table__corner-selector");
    expect(tableSelectionLayer).toContain("选择整个表格");
    expect(tableModel).toContain("selectionIncludesCell");
    expect(tableProjection).toContain("class TableGridProjection");
    expect(renderers).toContain("createTableGridProjection");
    expect(tableSelectionLayer).toContain('data-table-selector="all"');
    expect(tableCellView).toContain("block-table__cell--selected");
    expect(renderers).toContain("mergedCellProjection(tablePayload.data, row.id, column.id)");
    expect(renderers).toContain("if (projection === null) return null;");
    expect(renderers).toContain("rowSpan={projection?.rowSpan}");
    expect(renderers).toContain("colSpan={projection?.colSpan}");
    expect(tableModel).toContain("return { rowSpan: endRow - startRow + 1, colSpan: endColumn - startColumn + 1 }");
    expect(tableSelectionToolbar).toContain('role="toolbar"');
    expect(tableSelectionToolbar).toContain("block-table__selection-toolbar");
    expect(tableSelectionToolbar).toContain('name="font-increase"');
    expect(tableSelectionToolbar).toContain('name="font-decrease"');
    expect(tableSelectionToolbar).toContain('icon="highlight"');
    expect(tableSelectionToolbar).toContain('icon="bg-colors"');
    expect(tableSelectionToolbar).toContain('icon="vertical-align"');
    expect(tableSelectionToolbar).toContain('name="merge-cells"');
    expect(tableSelectionToolbar).toContain('name="split-cells"');
    expect(tableSelectionToolbar).toContain("单元格填充颜色");
    expect(tableSelectionToolbar).toContain("边框和框线");
    expect(tableSelectionToolbar).toContain("插入行列");
    expect(tableSelectionToolbar).not.toContain('aria-label="复制选区"');
    expect(tableSelectionToolbar).not.toContain('tone="danger"');
    expect(tableToolbarContract).toContain('actions: ["mergeOrSplit", "insertRowColumn"]');
    expect(tableToolbarContract).not.toContain('"copy"');
    expect(tableToolbarContract).not.toContain('"delete"');
    expect(tableSelectionController).toContain("readTableCellTextSelection");
    expect(renderers).toContain("onContextMenu");
    expect(tableContextMenu).toContain("插入行列");
    expect(tableContextMenu).toContain("<Portal>");
    expect(tableContextMenu).toContain("getBoundingClientRect()");
    // The shared top-left hit region belongs exclusively to the select-all
    // corner.  Boundary affordances start at the first internal edge so the
    // row/column +/- controls cannot cover the corner or its focus ring.
    expect(renderers).toContain("boundaryIndex === 0 ? null");
    expect(blocksCss).toContain(".block-table__selection-layer");
    expect(blocksCss).toContain("z-index: var(--oo-z-editor-selection-layer);");
    expect(blocksCss).toContain(".block-table__controls");
    expect(blocksCss).toContain("z-index: var(--oo-z-editor-selection);");
    expect(blocksCss).toContain(".block-table__row-selector");
    expect(blocksCss).toContain(".block-table__column-selector");
    expect(blocksCss).toContain(".block-table__corner-selector");
    expect(blocksCss).toContain(".block-table__cell--selected");
    expect(renderers).toContain('is-selection-active');
    expect(blocksCss).toContain('.block-table-wrap.is-selection-active');
    expect(blocksCss).toContain('box-shadow: none;');
    expect(blocksCss).toContain(".block-table__selection-toolbar");
    expect(blocksCss).toMatch(/\.block-table__cell:focus\s*\{[^}]*box-shadow:\s*none;/);
    expect(blocksCss).toContain(".block-table__context-menu");
    expect(blocksCss).toContain(".block-table__context-submenu");
    expect(blocksCss).toContain("position: fixed;");
    expect(blocksCss).toContain("z-index: calc(var(--oo-z-overlay, 1000) + 10);");
    expect(tableContextMenu).toContain("<Portal>");
    expect(tableSelectionController).toContain("onCellKeyDown");
    expect(tableSelectionController).toContain("event.shiftKey");
    expect(tableSelectionController).toContain('kind: "range"');
    expect(tableSelectionController).toContain("focus({ preventScroll: true })");
    expect(tableGeometryController).toContain("beginColumnResize");
    expect(renderers).toContain('data-table-resize="column"');
    expect(renderers).toContain('data-table-column-next-id');
    expect(blocksCss).toContain(".block-table__resize-handle");
    expect(blocksCss).toContain("cursor: col-resize;");
    expect(tableGeometryController).toContain("beginRowResize");
    expect(renderers).toContain('data-table-resize="row"');
    expect(renderers).toContain('data-table-row-next-id');
    expect(renderers).not.toContain('setTableRowHeights');
    expect(blockSession).not.toContain('setTableRowHeights');
    expect(blocksCss).toContain(".block-table__row-resize-handle");
    expect(blocksCss).toContain("cursor: row-resize;");
    expect(blocksCss).toContain(".block-table__resize-handle");
    expect(blocksCss).toContain("pointer-events: auto;");
    expect(blocksCss).toContain("is-resizing");
  });

  it("exposes table row and column deletion through the session command facade", () => {
    expect(blockSession).toContain("deleteTableRow");
    expect(blockSession).toContain("deleteTableColumn");
    expect(blockSession).toContain('type: "insertTableRow"');
    expect(blockSession).toContain('type: "insertTableColumn"');
    expect(blockSession).toContain('type: "setTableRowHeight"');
  });

  it("keeps table merge selection semantic and pointer-driven", () => {
    expect(tableSelectionController).toContain("normalizeTableSelection");
    expect(tableSelectionController).toContain('window.addEventListener("pointermove", extendCellDrag)');
    expect(tableSelectionController).toContain("shouldPromotePointerToTableRange");
    expect(renderers).toContain('selection?.kind !== "cell" && selectionIncludesCell');
    expect(renderers).toContain("tableContextSelectionForCell(");
    expect(tableCellView).toContain("data-table-row-id={rowId}");
    expect(tableModel).toContain("export function tableRangeForSelection");
    expect(tableModel).toContain("export function tableContextSelectionForCell");
    expect(tableModel).toContain("export function tableBoundaryCrossesMerge");
    expect(renderers).toContain('tableBoundaryCrossesMerge(tablePayload.data, "row", boundaryIndex)');
    expect(renderers).toContain('tableBoundaryCrossesMerge(tablePayload.data, "column", boundaryIndex)');
    expect(tableBehavior).toContain("tableSelection(selection, { blockId, interaction })");
    expect(blockNode).toContain("behavior?.tableSelection?.(tableSelection");
  });

  it("keeps list markers inside the block content box", () => {
    expect(blockNode).toContain('block-row__content-shell--list');
    expect(blocksCss).toContain('.block-row__content-shell--list');
    expect(blocksCss).toContain('grid-template-columns: max-content minmax(0, 1fr);');
    expect(blocksCss).toContain('align-items: center;');
    expect(blocksCss).toContain('min-height: 32px;');
    expect(blocksCss).not.toMatch(/\.block-row__list-marker\s*\{[^}]*position:\s*absolute/);
    expect(blocksCss).toContain('.block-row__content-shell--list > :not(.block-row__list-marker)');
    expect(blocksCss).toContain('.block-image__content');
    expect(blocksCss).toContain('max-width: 100%;');
    expect(contentRenderer).toContain('onPaste={(event) =>');
    expect(contentRenderer).toContain('session.insertPastedImage(block.id, file)');
    expect(blockSession).toContain('buildPastedImageInsertCommands');
  });

  it("gives selected image blocks an object outline and an object-local toolbar", () => {
    expect(blockNode).toContain('interactionSelection.kind === "object"');
    expect(renderers).toContain('<ImageBlockToolbar');
    expect(renderers).toContain('onSelectObject?.() ?? session.setActiveBlock(block.id)');
    expect(blocksCss).toContain('.block-image.is-selected');
    expect(blocksCss).toContain('.block-image__toolbar');
    expect(blocksCss).toContain('z-index: var(--oo-z-popover)');
    expect(blocksCss).toContain('background: #1f232b;');
    expect(blocksCss).toContain('.block-image__toolbar .oo-toolbar__item:hover:not(:disabled)');
  });

  it("owns the document context menu instead of exposing the browser menu", () => {
    expect(blockEditor).toContain('onContextMenu={(event) => event.preventDefault()}');
    expect(blockNode).toContain("<BlockContextMenu");
    expect(blockContextMenu).toContain("文档右键菜单");
    expect(blockContextMenu).toContain("仅文本粘贴");
    expect(blockContextMenu).toContain("批注");
    expect(blockContextMenu).toContain("插入链接");
    expect(blockContextMenu).toContain("<Portal>");
    expect(blocksCss).toContain(".block-row__context-menu");
  });

  it("continues list semantics when creating the next block with Enter", () => {
    const contentBehavior = readSource("../src/blocks/behaviors/contentBehavior.ts");
    expect(contentRenderer).toContain("onKeyDown={onKeyDown}");
    expect(contentBehavior).toContain('const nextAttrs = listKind && block.content?.text.trim()');
    expect(contentBehavior).toContain('session.insertAfter(blockId, { type: "paragraph" }, nextAttrs)');
    expect(contentBehavior).toContain('session.setBlockPresentation(blockId, { listType: null, listLevel: null, indentLevel: null })');
    expect(blockNode).toContain('listOrdinalFor(block.children, childIndex, store)');
    expect(blockSession).toContain('type: "setBlockPresentation"');
  });

  it("routes inline formatting through the typed range command", () => {
    const start = blockSession.indexOf("const toggleMark =");
    const end = blockSession.indexOf("const submitHistory =", start);
    const inlineSession = blockSession.slice(start, end);
    expect(inlineSession).toContain('type: "patchInlineRange"');
    expect(inlineSession).toContain("range: { start, end }");
    expect(inlineSession).not.toContain('type: "updateBlock"');
  });

  it("maps native selections to block-local ranges", () => {
    expect(blockSelection).toContain('querySelectorAll<HTMLElement>(".block-row__content[contenteditable]")');
    expect(blockSelection).toContain("range.intersectsNode(element)");
    expect(blockSelection).toContain("restoreBlockTextSelection");
    expect(blockSession).toContain("readBlockTextSelection()");
    expect(blockSession).toContain("dispatchCommands(commands)");
    expect(blockSession).toContain("deleteTextSelection");
    expect(blockSession).toContain('type: "replaceBlockText"');
    expect(richText).toContain("sliceRichText");
    expect(richText).toContain("concatRichText");
  });

  it("keeps IME composition provisional input out of the semantic queue", () => {
    expect(contentRenderer).toContain("onCompositionStart");
    expect(contentRenderer).toContain("onCompositionEnd");
    expect(contentRenderer).toContain("composingRef.current");
  });

  it("preserves a text selection while toolbar controls are clicked", () => {
    expect(toolbarPrimitive).toContain('if (!event.defaultPrevented && event.button === 0) event.preventDefault();');
    expect(toolbarPrimitive).toContain('onMouseDown={(event: ReactMouseEvent<HTMLButtonElement>)');
  });

  it("does not reintroduce legacy toolbar aliases into editor source", () => {
    const editorSource = sourceFiles(sourceFile("../src")).map((path) => readFileSync(path, "utf8")).join("\n");
    expect(editorSource).not.toMatch(/\b(?:tbtn|tselect|tcolor)\b/);
  });
});
