export interface BlockTextSelection {
  blockId: string;
  start: number;
  end: number;
}

export interface TableCellTextSelection {
  rowId: string;
  cellId: string;
  start: number;
  end: number;
}

/** Read a non-empty native selection contained by one editable table cell. */
export function readTableCellTextSelection(root: HTMLElement): TableCellTextSelection | null {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
  const range = selection.getRangeAt(0);
  const cellForNode = (node: Node): HTMLElement | null => {
    const element = node.nodeType === Node.ELEMENT_NODE ? node as Element : node.parentElement;
    const cell = element?.closest<HTMLElement>("[data-table-cell-id]") ?? null;
    return cell && root.contains(cell) ? cell : null;
  };
  const startCell = cellForNode(range.startContainer);
  const endCell = cellForNode(range.endContainer);
  if (!startCell || startCell !== endCell) return null;
  const rowId = startCell.dataset.tableRowId;
  const cellId = startCell.dataset.tableCellId;
  if (!rowId || !cellId) return null;
  const start = scalarOffsetWithin(startCell, range.startContainer, range.startOffset);
  const end = scalarOffsetWithin(startCell, range.endContainer, range.endOffset);
  return end > start ? { rowId, cellId, start, end } : null;
}

/** Restore a cell-local native selection after a semantic transaction rerenders the cell. */
export function restoreTableCellTextSelection(root: HTMLElement, selectionRange: TableCellTextSelection): void {
  const cell = root.querySelector<HTMLElement>(
    `[data-table-row-id="${CSS.escape(selectionRange.rowId)}"][data-table-cell-id="${CSS.escape(selectionRange.cellId)}"]`,
  );
  if (!cell) return;
  const start = scalarBoundaryAtOffset(cell, selectionRange.start);
  const end = scalarBoundaryAtOffset(cell, selectionRange.end);
  const selection = window.getSelection();
  if (!start || !end || !selection) return;
  const range = document.createRange();
  range.setStart(start.node, start.offset);
  range.setEnd(end.node, end.offset);
  selection.removeAllRanges();
  selection.addRange(range);
}

/**
 * Reads the browser selection as local ranges for every editable text block it
 * intersects. Native Selection can span sibling contentEditable roots, but
 * the document engine needs block-local offsets for a typed transaction.
 */
export function readBlockTextSelection(page: HTMLElement | null = document.querySelector<HTMLElement>(".block-editor__page")): BlockTextSelection[] {
  const selection = window.getSelection();
  if (!page || !selection || selection.rangeCount === 0 || selection.isCollapsed) return [];
  const range = selection.getRangeAt(0);
  if (!containsBoundary(page, range.startContainer) || !containsBoundary(page, range.endContainer)) return [];

  const ranges: BlockTextSelection[] = [];
  for (const element of page.querySelectorAll<HTMLElement>(".block-row__content[contenteditable]")) {
    const textLength = Array.from(element.textContent ?? "").length;
    if (textLength === 0 || !intersects(range, element)) continue;
    const blockId = element.closest<HTMLElement>("[data-block-id]")?.dataset.blockId;
    if (!blockId) continue;
    const start = containsBoundary(element, range.startContainer)
      ? scalarOffsetWithin(element, range.startContainer, range.startOffset)
      : 0;
    const end = containsBoundary(element, range.endContainer)
      ? scalarOffsetWithin(element, range.endContainer, range.endOffset)
      : textLength;
    if (end > start) ranges.push({ blockId, start, end });
  }
  return ranges;
}

/** Restore a previously captured multi-block range after React refreshes the DOM. */
export function restoreBlockTextSelection(ranges: readonly BlockTextSelection[], page: HTMLElement | null = document.querySelector<HTMLElement>(".block-editor__page")): void {
  if (!page || ranges.length === 0) return;
  const first = ranges[0];
  const last = ranges[ranges.length - 1];
  const firstRoot = page.querySelector<HTMLElement>(`[data-block-id="${CSS.escape(first.blockId)}"] .block-row__content[contenteditable]`);
  const lastRoot = page.querySelector<HTMLElement>(`[data-block-id="${CSS.escape(last.blockId)}"] .block-row__content[contenteditable]`);
  if (!firstRoot || !lastRoot) return;
  const start = scalarBoundaryAtOffset(firstRoot, first.start);
  const end = scalarBoundaryAtOffset(lastRoot, last.end);
  if (!start || !end) return;
  const selection = window.getSelection();
  if (!selection) return;
  const range = document.createRange();
  range.setStart(start.node, start.offset);
  range.setEnd(end.node, end.offset);
  selection.removeAllRanges();
  selection.addRange(range);
}

function intersects(range: Range, element: HTMLElement): boolean {
  try {
    return range.intersectsNode(element);
  } catch {
    return false;
  }
}

function containsBoundary(root: Node, node: Node): boolean {
  return root === node || root.contains(node);
}

/** Convert a DOM boundary to the editor's Unicode scalar offset. */
export function scalarOffsetWithin(root: HTMLElement, node: Node, offset: number): number {
  try {
    const range = document.createRange();
    range.selectNodeContents(root);
    range.setEnd(node, offset);
    return Array.from(range.toString()).length;
  } catch {
    return 0;
  }
}

/** Convert an editor Unicode scalar offset back into a concrete DOM boundary. */
export function scalarBoundaryAtOffset(root: HTMLElement, offset: number): { node: Node; offset: number } | null {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let remaining = Math.max(0, offset);
  let node: Node | null;
  while ((node = walker.nextNode())) {
    const length = Array.from(node.textContent ?? "").length;
    if (remaining <= length) return { node, offset: codeUnitOffset(node.textContent ?? "", remaining) };
    remaining -= length;
  }
  return { node: root, offset: root.childNodes.length };
}

function codeUnitOffset(value: string, codePointOffset: number): number {
  let codeUnits = 0;
  let codePoints = 0;
  for (const character of value) {
    if (codePoints >= codePointOffset) break;
    codeUnits += character.length;
    codePoints += 1;
  }
  return codeUnits;
}
