import { useMemo, useRef, useState, type CSSProperties, type MouseEvent as ReactMouseEvent } from "react";
import type { DocumentBlock } from "@open-office/schema/artifact";
import { Icon } from "@open-office/ui";
import type { BlockSessionApi } from "../hooks/useBlockSession.js";
import { CodeBlockView } from "./code/CodeBlockView.js";
export { ContentBlockRenderer } from "./content/ContentBlockRenderer.js";
import { ImageBlockToolbar } from "./image/ImageBlockToolbar.js";
import { ImageCropOverlay } from "./image/ImageCropOverlay.js";
import { ImageResizeHandles } from "./image/ImageResizeHandles.js";
import { useImageMove } from "./image/useImageMove.js";
import type { BlockRendererProps } from "./registry.js";
import type { EditorSelection } from "../interaction/types.js";
import { TableContextMenu } from "./table/TableContextMenu.js";
import { TableCellView } from "./table/TableCellView.js";
import { TableSelectionLayer } from "./table/TableSelectionLayer.js";
import { TableSelectionToolbar } from "./table/TableSelectionToolbar.js";
import { useTableCommandController } from "./table/useTableCommandController.js";
import { useTableGeometryController } from "./table/useTableGeometryController.js";
import {
  mergedCellProjection,
  selectionIncludesCell,
  tableContextSelectionForCell,
  tableSelectionSpansMultipleCells,
  tableBoundaryCrossesMerge,
  type TableContextTarget,
  type TableSelection,
} from "./table/model.js";
import { createTableGridProjection } from "./table/projection.js";
import { useTableSelectionController } from "./table/useTableSelectionController.js";

export function DividerBlockRenderer({ block, session }: BlockRendererProps) {
  return <button className="block-divider" type="button" onClick={() => session.setActiveBlock(block.id)} aria-label="分割线" />;
}

export function TableBlockRenderer({ block, session, onTableSelection, editorSelection }: BlockRendererProps) {
  return <TableBlockView block={block} session={session} onTableSelection={onTableSelection} editorSelection={editorSelection} />;
}

export function ImageBlockRenderer({ block, session, selected, onSelectObject }: BlockRendererProps) {
  return <ImageBlockView block={block} session={session} selected={selected} onSelectObject={onSelectObject} />;
}

export function CodeBlockRenderer({ block, session }: BlockRendererProps) {
  return <CodeBlockView block={block} session={session} />;
}

export function TableBlockView({ block, session, onTableSelection, editorSelection }: {
  block: DocumentBlock;
  session: BlockSessionApi;
  onTableSelection?: (selection: TableSelection | null) => void;
  editorSelection?: EditorSelection;
}) {
  const tablePayload = block.data.type === "table" ? block.data : null;
  const wrapRef = useRef<HTMLDivElement>(null);
  const [contextMenu, setContextMenu] = useState<TableContextTarget | null>(null);
  const {
    geometry,
    columnPreview: resizePreview,
    rowPreview: rowResizePreview,
    beginColumnResize,
    beginRowResize,
  } = useTableGeometryController({
    blockId: block.id,
    table: tablePayload?.data ?? null,
    rootRef: wrapRef,
    session,
  });
  const {
    selection,
    cellTextSelection,
    isCellSelecting,
    commitSelection,
    selectSelection,
    onCellFocus,
    onCellPointerDown,
    onCellPointerEnter,
    onCellKeyDown,
  } = useTableSelectionController({
    table: tablePayload?.data ?? null,
    rootRef: wrapRef,
    onSelectionChange: onTableSelection,
  });
  const tableCommands = useTableCommandController({
    blockId: block.id,
    table: tablePayload?.data ?? null,
    selection,
    cellTextSelection,
    session,
    rootRef: wrapRef,
    commitSelection,
    onDismiss: () => setContextMenu(null),
  });

  // TableGridProjection is the read-only runtime index for this render. It
  // keeps stable-id lookups out of cell JSX while retaining the canonical
  // TableBlock reference; commands still go through BlockSessionApi below.
  const grid = useMemo(() => tablePayload ? createTableGridProjection(tablePayload.data) : null, [tablePayload?.data]);
  if (!tablePayload || !grid) return null;

  const interactionTableSelection = editorSelection?.kind === "table" && editorSelection.blockId === block.id
    ? editorSelection.selection
    : null;

  const openContextMenu = (event: ReactMouseEvent, nextSelection: TableSelection) => {
    event.preventDefault();
    event.stopPropagation();
    const normalizedSelection = commitSelection(nextSelection);
    if (!normalizedSelection) return;
    // Context menus live in the global overlay portal, so their coordinates
    // stay in viewport space rather than the table's stacking context.
    setContextMenu({ selection: normalizedSelection, x: event.clientX, y: event.clientY });
  };


  const closeContextOnContentPointerDown = (event: ReactMouseEvent<HTMLDivElement>) => {
    const target = event.target as Element;
    if (target.closest(".block-table__context-menu") || target.closest(".block-table__context-submenu") || target.closest(".block-table__row-selector") || target.closest(".block-table__column-selector") || target.closest(".block-table__corner-selector") || target.closest(".block-table__selection-toolbar")) return;
    setContextMenu(null);
  };

  return (
    <div
      ref={wrapRef}
      className={`block-table-wrap${selection ? " is-selection-active" : ""}${isCellSelecting ? " is-cell-selecting" : ""}`}
      onPointerDown={closeContextOnContentPointerDown}
    >
      <table className="block-table" aria-label="表格块" data-table-grid="document">
        <colgroup>
          {grid.columns.map((column) => {
            const previewWidth = resizePreview?.leftColumnId === column.id
              ? resizePreview.leftWidth
              : resizePreview?.rightColumnId === column.id
                ? resizePreview.rightWidth
                : null;
            return <col key={column.id} style={previewWidth !== null
              ? { width: `${previewWidth}px` }
              : column.width ? { width: `${column.width}px` } : undefined} />;
          })}
        </colgroup>
        <tbody>
          {grid.rows.map((row) => (
            <tr key={row.id} style={rowResizePreview?.rowId === row.id
              ? { height: `${rowResizePreview.height}px` }
              : row.height ? { height: `${row.height}px` } : undefined}>
              {grid.cellsInRow(row.id).map((cell) => {
                const columnIndex = row.cells.indexOf(cell);
                const column = columnIndex >= 0 ? grid.columns[columnIndex] : undefined;
                if (columnIndex < 0 || !column) return null;
                if (!grid.cellAt(row.id, column.id)) return null;
                const projection = column ? mergedCellProjection(tablePayload.data, row.id, column.id) : undefined;
                if (projection === null) return null;
                return <TableCellView
                  key={cell.id}
                  blockId={block.id}
                  rowId={row.id}
                  columnId={column?.id ?? ""}
                  cellId={cell.id}
                  rowIndex={grid.rows.indexOf(row)}
                  columnIndex={columnIndex}
                  content={cell.content}
                  session={session}
                  selected={selection?.kind !== "cell" && selectionIncludesCell(tablePayload.data, selection, row.id, tablePayload.data.columns[columnIndex]?.id ?? "", cell.id)}
                  style={cell.style}
                  onFocus={() => {
                    session.setActiveBlock(block.id);
                    onCellFocus({ rowId: row.id, columnId: column.id, cellId: cell.id });
                    setContextMenu(null);
                  }}
                  onPointerDown={(event) => onCellPointerDown(event, { rowId: row.id, columnId: column.id, cellId: cell.id })}
                  onPointerEnter={(event) => onCellPointerEnter(event, { rowId: row.id, columnId: column.id, cellId: cell.id })}
                  onKeyDown={(event) => onCellKeyDown(event, { rowId: row.id, columnId: column.id, cellId: cell.id })}
                  onContextMenu={(event) => openContextMenu(
                    event,
                    tableContextSelectionForCell(
                      tablePayload.data,
                      selection,
                      row.id,
                      tablePayload.data.columns[columnIndex]?.id ?? "",
                      cell.id,
                    ),
                  )}
                  rowSpan={projection?.rowSpan}
                  colSpan={projection?.colSpan}
                />;
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="block-table__controls" aria-label="表格操作">
        {geometry.columnBoundaries.map((left, boundaryIndex) => {
          // Only internal boundaries are resizable. Each boundary owns the
          // two adjacent columns, so the grid total stays fixed and no other
          // column is redistributed by the browser.
          if (boundaryIndex <= 0 || boundaryIndex >= geometry.columns.length) return null;
          if (tableBoundaryCrossesMerge(tablePayload.data, "column", boundaryIndex)) return null;
          const leftColumn = geometry.columns[boundaryIndex - 1];
          const rightColumn = geometry.columns[boundaryIndex];
          const tableHeight = (geometry.rowBoundaries.at(-1) ?? geometry.tableTop) - geometry.tableTop;
          return (
            <button
              key={`resize-column-${leftColumn.id}-${rightColumn.id}`}
              className={`block-table__resize-handle${resizePreview?.leftColumnId === leftColumn.id && resizePreview.rightColumnId === rightColumn.id ? " is-resizing" : ""}`}
              style={{ left: `${left}px`, top: `${geometry.tableTop}px`, height: `${Math.max(0, tableHeight)}px` }}
              type="button"
              data-table-resize="column"
              data-table-column-id={leftColumn.id}
              data-table-column-next-id={rightColumn.id}
              aria-label={`调整第 ${boundaryIndex} 与第 ${boundaryIndex + 1} 列宽度`}
              title="拖动调整列宽"
              onPointerDown={(event) => beginColumnResize(event, leftColumn.id, rightColumn.id, leftColumn.width, rightColumn.width)}
            />
          );
        })}
        {geometry.rowBoundaries.map((top, boundaryIndex) => {
          // Only internal boundaries are resizable. Each boundary owns the
          // two adjacent rows, preserving the table's total height.
          if (boundaryIndex <= 0 || boundaryIndex >= geometry.rows.length) return null;
          if (tableBoundaryCrossesMerge(tablePayload.data, "row", boundaryIndex)) return null;
          const topRow = geometry.rows[boundaryIndex - 1];
          const bottomRow = geometry.rows[boundaryIndex];
          const tableWidth = geometry.tableWidth;
          return (
            <button
              key={`resize-row-${topRow.id}-${bottomRow.id}`}
              className={`block-table__row-resize-handle${rowResizePreview?.rowId === topRow.id ? " is-resizing" : ""}`}
              style={{ left: `${geometry.tableLeft}px`, top: `${top}px`, width: `${Math.max(0, tableWidth)}px` }}
              type="button"
              data-table-resize="row"
              data-table-row-id={topRow.id}
              data-table-row-next-id={bottomRow.id}
              aria-label={`调整第 ${boundaryIndex} 与第 ${boundaryIndex + 1} 行高度`}
              title="拖动调整行高"
              onPointerDown={(event) => beginRowResize(event, topRow.id, topRow.height)}
            />
          );
        })}
        {geometry.rowBoundaries.map((top, boundaryIndex) => (
          boundaryIndex === 0 ? null : (
          <button
            key={`row-${boundaryIndex}`}
            className="block-table__affordance block-table__row-affordance"
            style={{ left: `${geometry.tableLeft - 9}px`, top: `${top - 9}px` }}
            type="button"
            aria-label={boundaryIndex === geometry.rowBoundaries.length - 1 ? "在表格末尾添加行" : `在第 ${boundaryIndex + 1} 行前添加行`}
            title={boundaryIndex === geometry.rowBoundaries.length - 1 ? "添加行" : "在此处添加行"}
            onClick={() => session.insertTableRow(block.id, boundaryIndex)}
          >
            <Icon name="plus" />
            <span className="sr-only">添加行</span>
          </button>
          )
        ))}
        {geometry.columnBoundaries.map((left, boundaryIndex) => (
          boundaryIndex === 0 ? null : (
          <button
            key={`column-${boundaryIndex}`}
            className="block-table__affordance block-table__column-affordance"
            style={{ left: `${left - 9}px`, top: `${geometry.tableTop - 9}px` }}
            type="button"
            aria-label={boundaryIndex === geometry.columnBoundaries.length - 1 ? "在表格末尾添加列" : `在第 ${boundaryIndex + 1} 列前添加列`}
            title={boundaryIndex === geometry.columnBoundaries.length - 1 ? "添加列" : "在此处添加列"}
            onClick={() => session.insertTableColumn(block.id, boundaryIndex)}
          >
            <Icon name="plus" />
            <span className="sr-only">添加列</span>
          </button>
          )
        ))}
      </div>
      <TableSelectionLayer
        table={tablePayload.data}
        geometry={geometry}
        selection={selection}
        onSelect={selectSelection}
        onContextMenu={openContextMenu}
      />
      {interactionTableSelection && (tableSelectionSpansMultipleCells(tablePayload.data, interactionTableSelection) || cellTextSelection !== null) && (
        <TableSelectionToolbar
          selection={interactionTableSelection}
          geometry={geometry}
          onInsert={tableCommands.insertAtSelection}
          onMerge={tableCommands.mergeSelection}
          onSplit={tableCommands.splitSelection}
          onApplyBorderPreset={tableCommands.applyBorderPreset}
          canMerge={tableCommands.canMerge}
          canSplit={tableCommands.canSplit}
          formatState={tableCommands.formatState}
          onFormat={tableCommands.formatSelection}
        />
      )}
      {contextMenu && (
        <TableContextMenu
          target={contextMenu}
          onDismiss={() => setContextMenu(null)}
          onCut={tableCommands.cutSelection}
          onCopy={() => void tableCommands.copySelection()}
          onInsert={tableCommands.insertAtSelection}
          onDelete={tableCommands.deleteSelection}
          onMerge={tableCommands.mergeSelection}
          onSplit={tableCommands.splitSelection}
          onApplyBorderPreset={tableCommands.applyBorderPreset}
          canMerge={tableCommands.canMerge}
          canSplit={tableCommands.canSplit}
        />
      )}
    </div>
  );
}

export function ImageBlockView({ block, session, selected, onSelectObject }: { block: DocumentBlock; session: BlockSessionApi; selected: boolean; onSelectObject?: () => void }) {
  const [failed, setFailed] = useState(false);
  const rootRef = useRef<HTMLElement>(null);
  const frameRef = useRef<HTMLDivElement>(null);
  const [intrinsicSize, setIntrinsicSize] = useState<{ width: number; height: number } | null>(null);
  const [resizePreview, setResizePreview] = useState<{ width: number; height: number; offsetX: number; offsetY: number } | null>(null);
  const [movePreview, setMovePreview] = useState<{ offsetX: number; offsetY: number } | null>(null);
  const [cropDraft, setCropDraft] = useState<Extract<DocumentBlock["data"], { type: "image" }>['data']["transform"] | null>(null);
  const [cropEditing, setCropEditing] = useState(false);
  const imageData = block.data.type === "image" ? block.data.data : null;
  const alt = imageData?.alt ?? "";
  const assetUrl = imageData ? session.assetUrl(imageData.assetId) : "";
  const imageTransform = cropEditing ? cropDraft ?? imageData?.transform : imageData?.transform;
  const appliedCrop = imageData?.transform.crop ?? { top: 0, right: 0, bottom: 0, left: 0 };
  const cropWidthRatio = Math.max(0.08, 1 - appliedCrop.left - appliedCrop.right);
  const cropHeightRatio = Math.max(0.08, 1 - appliedCrop.top - appliedCrop.bottom);
  const displaySize = resizePreview ?? imageData?.size ?? { width: null, height: null, lockAspectRatio: true };
  const placement = resizePreview
    ? { offsetX: resizePreview.offsetX, offsetY: resizePreview.offsetY }
    : movePreview ?? imageData?.placement ?? { offsetX: 0, offsetY: 0 };
  // An intrinsic image size describes the source asset, not an editor frame.
  // Treating it as a fixed frame makes CSS clamp the width while retaining the
  // original height, which leaves a large letterbox above and below the image.
  // A frame becomes explicit only after a resize has been persisted or is
  // currently being previewed.
  const hasStoredFrameSize = resizePreview !== null
    || imageData?.size.width !== null
    || imageData?.size.height !== null;
  const sourceAspectRatio = intrinsicSize && intrinsicSize.height > 0
    ? intrinsicSize.width / intrinsicSize.height
    : null;
  const frameWidth = displaySize.width
    ?? (hasStoredFrameSize && displaySize.height !== null && sourceAspectRatio
      ? displaySize.height * sourceAspectRatio
      : null);
  const frameHeight = displaySize.height
    ?? (hasStoredFrameSize && displaySize.width !== null && sourceAspectRatio
      ? displaySize.width / sourceAspectRatio
      : null);
  const hasExplicitFrameSize = hasStoredFrameSize && frameWidth !== null && frameHeight !== null;
  // `ImageSize` is the visible object size. Once an image has been cropped,
  // reconstruct the source canvas behind that visible window so crop data
  // never becomes a merely cosmetic clip-path.
  const sourceFrameWidth = hasExplicitFrameSize ? frameWidth / cropWidthRatio : null;
  const sourceFrameHeight = hasExplicitFrameSize ? frameHeight / cropHeightRatio : null;
  const editorFrameWidth = cropEditing ? sourceFrameWidth ?? frameWidth : frameWidth;
  const editorFrameHeight = cropEditing ? sourceFrameHeight ?? frameHeight : frameHeight;
  const rendersCroppedWindow = !cropEditing
    && hasExplicitFrameSize
    && (appliedCrop.left > 0 || appliedCrop.right > 0 || appliedCrop.top > 0 || appliedCrop.bottom > 0);
  const imageStyle: CSSProperties = {
    // Legacy imported crops without an explicit ImageSize remain compatible.
    clipPath: !hasExplicitFrameSize && !cropEditing
      ? `inset(${appliedCrop.top * 100}% ${appliedCrop.right * 100}% ${appliedCrop.bottom * 100}% ${appliedCrop.left * 100}%)`
      : undefined,
    transform: `scaleX(${imageData?.transform.flipHorizontal ? -1 : 1}) scaleY(${imageData?.transform.flipVertical ? -1 : 1})`,
    position: rendersCroppedWindow ? "absolute" : undefined,
    left: rendersCroppedWindow && sourceFrameWidth !== null ? `${-appliedCrop.left * sourceFrameWidth}px` : undefined,
    top: rendersCroppedWindow && sourceFrameHeight !== null ? `${-appliedCrop.top * sourceFrameHeight}px` : undefined,
    width: rendersCroppedWindow && sourceFrameWidth !== null
      ? `${sourceFrameWidth}px`
      : hasExplicitFrameSize ? "100%" : undefined,
    height: rendersCroppedWindow && sourceFrameHeight !== null
      ? `${sourceFrameHeight}px`
      : hasExplicitFrameSize ? "100%" : undefined,
    maxWidth: rendersCroppedWindow ? "none" : undefined,
    objectFit: hasExplicitFrameSize ? "fill" : undefined,
  };
  const imageMove = useImageMove({
    frameRef,
    placement: imageData?.placement ?? { offsetX: 0, offsetY: 0 },
    onPreview: setMovePreview,
    onCancel: () => setMovePreview(null),
    onCommit: (nextPlacement) => {
      setMovePreview(null);
      if (
        nextPlacement.offsetX === imageData?.placement.offsetX
        && nextPlacement.offsetY === imageData?.placement.offsetY
      ) return;
      session.setImageConfig(block.id, { placement: nextPlacement });
    },
  });

  if (!imageData) return null;

  return (
    <figure
      ref={rootRef}
      className={`block-image${selected ? " is-selected" : ""}${failed ? " is-load-failed" : ""}`}
      style={{
        marginLeft: placement.offsetX || undefined,
        marginTop: placement.offsetY ? `${10 + placement.offsetY}px` : undefined,
      }}
      aria-label={alt || "装饰性图片"}
      aria-selected={selected}
      role="group"
      tabIndex={0}
      onPointerDown={(event) => {
        // Re-selecting an object after its neighbouring paragraph has taken
        // focus must also move the browser focus back to the object. Without
        // this, root keyboard routing would keep receiving keys on the old
        // paragraph and Enter/Arrow navigation appeared to do nothing.
        event.currentTarget.focus({ preventScroll: true });
        onSelectObject?.() ?? session.setActiveBlock(block.id);
      }}
      onClick={(event) => {
        // The draggable frame deliberately prevents the pointer default while
        // preparing a move. Reassert focus after that gesture has settled so
        // a click (as opposed to a drag) always leaves keyboard ownership on
        // the image object.
        event.currentTarget.focus({ preventScroll: true });
        onSelectObject?.() ?? session.setActiveBlock(block.id);
      }}
      onFocus={() => onSelectObject?.() ?? session.setActiveBlock(block.id)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelectObject?.() ?? session.setActiveBlock(block.id);
          return;
        }
      }}
    >
      {selected && (
        <ImageBlockToolbar
          blockId={block.id}
          image={imageData}
          assetUrl={assetUrl}
          session={session}
          cropDraft={cropDraft}
          onCropDraftChange={setCropDraft}
          onCropEditingChange={setCropEditing}
          onCropCommit={(transform) => {
            const frame = frameRef.current?.getBoundingClientRect();
            const visibleWidth = Math.max(0.08, 1 - transform.crop.left - transform.crop.right);
            const visibleHeight = Math.max(0.08, 1 - transform.crop.top - transform.crop.bottom);
            session.setImageConfig(block.id, {
              transform,
              size: frame
                ? {
                  width: Math.max(1, Math.round(frame.width * visibleWidth)),
                  height: Math.max(1, Math.round(frame.height * visibleHeight)),
                  lockAspectRatio: imageData.size.lockAspectRatio,
                }
                : undefined,
            });
          }}
        />
      )}
      <div
        ref={frameRef}
        className="block-image__frame"
        style={hasExplicitFrameSize && editorFrameWidth !== null && editorFrameHeight !== null
          ? { width: `${editorFrameWidth}px`, height: `${editorFrameHeight}px` }
          : undefined}
        {...(selected && !cropEditing ? imageMove : {})}
      >
        {failed ? (
          <div className="block-image__placeholder">图片加载失败</div>
        ) : (
          <img
            className="block-image__content"
            src={assetUrl}
            alt={alt}
            draggable={false}
            style={imageStyle}
            onLoad={(event) => {
              const image = event.currentTarget;
              setIntrinsicSize({ width: image.naturalWidth, height: image.naturalHeight });
            }}
            onError={() => setFailed(true)}
          />
        )}
        {selected && cropEditing && imageTransform && (
          <ImageCropOverlay
            crop={imageTransform.crop}
            onChange={(nextCrop) => setCropDraft({ ...imageTransform, crop: nextCrop })}
          />
        )}
        {selected && !cropEditing && (
          <ImageResizeHandles
            frameRef={frameRef}
            placement={placement}
            lockAspectRatio={imageData.size.lockAspectRatio}
            onPreview={setResizePreview}
            onCancel={() => setResizePreview(null)}
            onCommit={(geometry) => {
              setResizePreview(null);
              if (
                geometry.width === imageData.size.width
                && geometry.height === imageData.size.height
                && geometry.offsetX === imageData.placement.offsetX
                && geometry.offsetY === imageData.placement.offsetY
              ) return;
              session.setImageConfig(block.id, {
                size: {
                  width: geometry.width,
                  height: geometry.height,
                  lockAspectRatio: imageData.size.lockAspectRatio,
                },
                placement: { offsetX: geometry.offsetX, offsetY: geometry.offsetY },
              });
            }}
          />
        )}
      </div>
      {imageData.caption && <figcaption className="block-image__caption">{imageData.caption}</figcaption>}
    </figure>
  );
}
