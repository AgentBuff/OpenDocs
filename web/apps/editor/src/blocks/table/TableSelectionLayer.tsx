import type { MouseEvent as ReactMouseEvent } from "react";
import type { TableBlock } from "@open-office/schema/artifact";
import { Icon } from "@open-office/ui";
import type { TableGeometry, TableSelection } from "./model.js";

interface TableSelectionLayerProps {
  table: TableBlock;
  geometry: TableGeometry;
  selection: TableSelection | null;
  onSelect: (selection: TableSelection) => void;
  onContextMenu: (event: ReactMouseEvent<HTMLButtonElement>, selection: TableSelection) => void;
}

function isSelected(selection: TableSelection | null, kind: "row" | "column", id: string): boolean {
  return selection?.kind === "all" || (selection?.kind === kind && selection.id === id);
}

/**
 * The gutters are a separate interaction surface, like Univer's selection
 * layer. They never participate in table layout and therefore cannot shift
 * cell content or create a fake row below the grid.
 */
export function TableSelectionLayer({
  table,
  geometry,
  selection,
  onSelect,
  onContextMenu,
}: TableSelectionLayerProps) {
  return (
    <>
      <div className="block-table__selection-layer" aria-label="表格行列选择区">
        {geometry.rows.map((row, index) => {
          const selected = isSelected(selection, "row", row.id);
          return (
            <button
              key={`row-selector-${row.id}`}
              className={`block-table__row-selector${index === geometry.rows.length - 1 ? " block-table__row-selector--last" : ""}${selected ? " is-selected" : ""}`}
              type="button"
              style={{ left: `${geometry.tableLeft - 12}px`, top: `${row.top}px`, height: `${row.height}px` }}
              aria-label={`选择第 ${index + 1} 行`}
              aria-pressed={selected}
              data-table-selector="row"
              data-table-row-id={row.id}
              onPointerDown={(event) => {
                event.preventDefault();
                event.stopPropagation();
              }}
              onClick={() => onSelect({ kind: "row", id: row.id })}
              onContextMenu={(event) => onContextMenu(event, { kind: "row", id: row.id })}
            >
              <Icon name="block-handle" />
            </button>
          );
        })}
        {geometry.columns.map((column, index) => {
          const selected = isSelected(selection, "column", column.id);
          return (
            <button
              key={`column-selector-${column.id}`}
              className={`block-table__column-selector${selected ? " is-selected" : ""}`}
              type="button"
              style={{ left: `${column.left}px`, top: `${geometry.tableTop - 12}px`, width: `${column.width}px` }}
              aria-label={`选择第 ${index + 1} 列`}
              aria-pressed={selected}
              data-table-selector="column"
              data-table-column-id={column.id}
              onPointerDown={(event) => {
                event.preventDefault();
                event.stopPropagation();
              }}
              onClick={() => onSelect({ kind: "column", id: column.id })}
              onContextMenu={(event) => onContextMenu(event, { kind: "column", id: column.id })}
            >
              <Icon name="block-handle" />
            </button>
          );
        })}
        <span className="sr-only">{table.rows.length} 行，{table.columns.length} 列</span>
      </div>
      <button
        className={`block-table__corner-selector${selection?.kind === "all" ? " is-selected" : ""}`}
        type="button"
        style={{ left: `${geometry.tableLeft - 12}px`, top: `${geometry.tableTop - 12}px` }}
        aria-label="选择整个表格"
        aria-pressed={selection?.kind === "all"}
        data-table-selector="all"
        onPointerDown={(event) => {
          event.preventDefault();
          event.stopPropagation();
        }}
        onClick={() => onSelect({ kind: "all" })}
        onContextMenu={(event) => onContextMenu(event, { kind: "all" })}
      />
    </>
  );
}
