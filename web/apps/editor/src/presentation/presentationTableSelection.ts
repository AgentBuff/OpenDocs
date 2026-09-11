import type { PresentationV5Node, PresentationV5NodeKind } from "@open-office/schema";

export type PresentationTableNode = PresentationV5Node & {
  kind: Extract<PresentationV5NodeKind, { type: "table" }>;
};

export type TableCellAddress = { row: number; column: number };
export type TableSelection = {
  nodeId: string;
  anchor: TableCellAddress;
  focus: TableCellAddress;
};

export function tableRange(selection: Pick<TableSelection, "anchor" | "focus">) {
  return {
    start: {
      row: Math.min(selection.anchor.row, selection.focus.row),
      column: Math.min(selection.anchor.column, selection.focus.column),
    },
    end: {
      row: Math.max(selection.anchor.row, selection.focus.row),
      column: Math.max(selection.anchor.column, selection.focus.column),
    },
  };
}

export function tableAnchorAt(node: PresentationTableNode, address: TableCellAddress) {
  return node.kind.data.cells.find((cell) =>
    address.row >= cell.row
    && address.row < cell.row + cell.rowSpan
    && address.column >= cell.column
    && address.column < cell.column + cell.columnSpan
  ) ?? null;
}

export function tableAnchorsInSelection(
  node: PresentationTableNode,
  selection: Pick<TableSelection, "anchor" | "focus">,
) {
  const range = tableRange(selection);
  return node.kind.data.cells.filter((cell) =>
    cell.row <= range.end.row
    && cell.row + cell.rowSpan - 1 >= range.start.row
    && cell.column <= range.end.column
    && cell.column + cell.columnSpan - 1 >= range.start.column
  );
}

export function tableSelectionCanMerge(
  node: PresentationTableNode,
  selection: Pick<TableSelection, "anchor" | "focus">,
) {
  const range = tableRange(selection);
  if (range.start.row === range.end.row && range.start.column === range.end.column) return false;
  return tableAnchorsInSelection(node, selection).every((cell) =>
    cell.row >= range.start.row
    && cell.column >= range.start.column
    && cell.row + cell.rowSpan - 1 <= range.end.row
    && cell.column + cell.columnSpan - 1 <= range.end.column
    && cell.rowSpan === 1
    && cell.columnSpan === 1
  );
}
