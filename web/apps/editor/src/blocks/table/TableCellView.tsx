import { useEffect, useRef, type CSSProperties, type KeyboardEvent, type MouseEvent, type PointerEvent } from "react";

import type { RichText, TableCellStyle } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";
import { richTextFromHtml, richTextToDom } from "../richText.js";

export interface TableCellViewProps {
  blockId: string;
  rowId: string;
  columnId: string;
  rowSpan?: number;
  colSpan?: number;
  cellId: string;
  content: RichText;
  style?: TableCellStyle;
  session: BlockSessionApi;
  selected?: boolean;
  onFocus?: () => void;
  onPointerDown?: (event: PointerEvent<HTMLTableCellElement>) => void;
  onPointerEnter?: (event: PointerEvent<HTMLTableCellElement>) => void;
  onContextMenu?: (event: MouseEvent<HTMLTableCellElement>) => void;
  onKeyDown?: (event: KeyboardEvent<HTMLTableCellElement>) => void;
}

/**
 * A DOM-first cell editor. The table controller owns selection and commands;
 * this view owns only the browser editable surface for one stable cell ID.
 */
export function TableCellView({
  blockId,
  rowId,
  columnId,
  rowSpan,
  colSpan,
  cellId,
  content,
  style,
  session,
  selected = false,
  onFocus,
  onPointerDown,
  onPointerEnter,
  onContextMenu,
  onKeyDown,
}: TableCellViewProps) {
  const ref = useRef<HTMLTableCellElement>(null);
  useEffect(() => {
    const element = ref.current;
    if (!element || document.activeElement === element) return;
    element.replaceChildren(richTextToDom(content));
  }, [content]);
  const borderStyle = (edge: "top" | "right" | "bottom" | "left"): string | undefined => {
    const border = style?.borders?.[edge];
    return border ? `${border.width}px ${border.style} ${border.color}` : undefined;
  };
  const diagonal = (edge: "diagonalDown" | "diagonalUp") => {
    const border = style?.borders?.[edge];
    if (!border) return undefined;
    const direction = edge === "diagonalDown" ? "to bottom right" : "to top right";
    const half = Math.max(0.5, border.width / 2);
    return `linear-gradient(${direction}, transparent calc(50% - ${half}px), ${border.color} 50%, transparent calc(50% + ${half}px))`;
  };
  return (
    <td
      ref={ref}
      className={`block-table__cell${selected ? " block-table__cell--selected" : ""}`}
      contentEditable
      suppressContentEditableWarning
      data-table-cell-id={cellId}
      data-table-column-id={columnId}
      rowSpan={rowSpan}
      colSpan={colSpan}
      data-table-row-id={rowId}
      style={{
        backgroundColor: style?.fillColor,
        textAlign: style?.horizontalAlign,
        verticalAlign: style?.verticalAlign,
        borderTop: borderStyle("top"),
        borderRight: borderStyle("right"),
        borderBottom: borderStyle("bottom"),
        borderLeft: borderStyle("left"),
        "--oo-table-diagonal-down": diagonal("diagonalDown") ?? "none",
        "--oo-table-diagonal-up": diagonal("diagonalUp") ?? "none",
      } as CSSProperties}
      onPointerDown={(event) => onPointerDown?.(event)}
      onPointerEnter={(event) => onPointerEnter?.(event)}
      onFocus={() => { session.setActiveBlock(blockId); onFocus?.(); }}
      onContextMenu={onContextMenu}
      onInput={() => {
        if (ref.current) session.updateTableCell(blockId, rowId, cellId, richTextFromHtml(ref.current));
      }}
      onKeyDown={(event) => {
        onKeyDown?.(event);
        if (event.defaultPrevented || event.key !== "Enter") return;
        event.preventDefault();
        const selection = window.getSelection();
        const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
        if (!range || !ref.current?.contains(range.commonAncestorContainer)) return;
        range.deleteContents();
        const newline = document.createTextNode("\n");
        range.insertNode(newline);
        range.setStartAfter(newline);
        range.collapse(true);
        selection?.removeAllRanges();
        selection?.addRange(range);
        session.updateTableCell(blockId, rowId, cellId, richTextFromHtml(ref.current));
      }}
      onBlur={() => void session.save()}
    />
  );
}
