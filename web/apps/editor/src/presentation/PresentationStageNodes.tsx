import type { PointerEvent as ReactPointerEvent } from "react";
import type { PresentationNodeAdornment } from "@open-office/presentation-ui";
import type { PresentationV5Node, PresentationV5NodeKind, PresentationV5Transform } from "@open-office/schema";

import { api } from "../api.js";
import { PresentationRichText, PresentationTextFrame } from "./PresentationRichText.js";
import { PresentationTextEditor } from "./PresentationTextEditor.js";
import { nodeStyle, paintColor } from "./presentationGeometry.js";
import type { PresentationDragMode, PresentationResizeHandle } from "./interactions.js";
import { tableRange, type PresentationTableNode, type TableCellAddress, type TableSelection } from "./presentationTableSelection.js";

type PresentationImageNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "image" }> };

export function SlideNode({
  artifactId,
  node,
  transform,
  scale,
  editing,
  adornments,
  unsupportedReason,
  onSelect,
  onEdit,
  onPointerDown,
  tableSelection,
  onTableCellSelect,
  onTextSave,
}: {
  artifactId: string;
  node: PresentationV5Node;
  transform: PresentationV5Transform;
  scale: number;
  editing: boolean;
  adornments: readonly PresentationNodeAdornment[];
  unsupportedReason: string | null;
  onSelect: (extend?: boolean) => void;
  onEdit: () => void;
  onPointerDown: (event: ReactPointerEvent<HTMLElement>, node: PresentationV5Node, mode: PresentationDragMode) => void;
  tableSelection?: TableSelection | null;
  /** Grid selection is renderer-local and never mutates the TableNode directly. */
  onTableCellSelect?: (address: TableCellAddress, extend: boolean) => void;
  onTextSave: (node: PresentationV5Node, body: import("@open-office/schema").PresentationV5RichText) => void;
}) {
  const style = nodeStyle(transform, scale, node.opacity);
  const outline = adornments.find((adornment): adornment is Extract<PresentationNodeAdornment, { kind: "outline" }> => adornment.kind === "outline");
  const selected = Boolean(outline);
  if (node.kind.type !== "text") {
    return (
      <div
        className={`presentation-studio__node presentation-studio__node--hit-target ${selected ? "is-selected" : ""}`}
        style={style}
        data-node-id={node.id}
        role="button"
        tabIndex={0}
        aria-label={`${node.name || node.kind.type}${selected ? "，已选择" : ""}`}
        aria-pressed={selected}
        onPointerDown={(event) => onPointerDown(event, node, "move")}
        // Pointer selection happens on down so drag starts from the same
        // stable target.  Do not toggle a second time on click.
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onSelect(event.shiftKey || event.metaKey || event.ctrlKey);
          }
        }}
      >
        {selected && outline && <NodeTransformHandles outline={outline} node={node} onPointerDown={onPointerDown} />}
        {node.kind.type === "image" && <PresentationImage assetId={node.kind.data.assetId} crop={node.kind.data.crop} flipH={node.kind.data.flipH} flipV={node.kind.data.flipV} artifactId={artifactId} caption={node.kind.data.caption} />}
        {node.kind.type === "table" && <PresentationTable pointScale={12700 * scale} node={node as PresentationTableNode} selection={tableSelection ?? null} onCellSelect={onTableCellSelect} />}
        {(node.kind.type === "video" || node.kind.type === "audio") && <PresentationMedia artifactId={artifactId} mediaType={node.kind.type} assetId={node.kind.data.assetId} posterAssetId={node.kind.data.posterAssetId} />}
        {unsupportedReason && <span className="presentation-studio__unsupported-node" title={unsupportedReason}>暂不支持</span>}
      </div>
    );
  }
  return (
    <div
      className={`presentation-studio__node ${selected ? "is-selected" : ""}`}
      style={style}
      data-node-id={node.id}
      role={editing ? undefined : "button"}
      tabIndex={editing ? undefined : 0}
      aria-label={`${node.name || "文本对象"}${selected ? "，已选择" : ""}`}
      aria-pressed={editing ? undefined : selected}
      onPointerDown={(event) => onPointerDown(event, node, "move")}
      onDoubleClick={(event) => { event.stopPropagation(); onEdit(); }}
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (editing) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect(event.shiftKey || event.metaKey || event.ctrlKey);
        }
        if (event.key === "F2") {
          event.preventDefault();
          onEdit();
        }
      }}
    >
      {editing ? (
        <PresentationTextEditor node={node} body={node.kind.data.frame.body} pointScale={12700 * scale} onSave={onTextSave} />
      ) : (
        <PresentationTextFrame
          body={node.kind.data.frame.body}
          pointScale={12700 * scale}
          autoFit={node.kind.data.frame.autoFit}
          verticalAlign={node.kind.data.frame.verticalAlign}
          padding={`${node.kind.data.frame.padding.top * scale}px ${node.kind.data.frame.padding.right * scale}px ${node.kind.data.frame.padding.bottom * scale}px ${node.kind.data.frame.padding.left * scale}px`}
          style={{ fontSize: 12 * 12700 * scale }}
        />
      )}
      {selected && !editing && outline && <NodeTransformHandles outline={outline} node={node} onPointerDown={onPointerDown} />}
    </div>
  );
}

function NodeTransformHandles({ outline, node, onPointerDown }: {
  outline: Extract<PresentationNodeAdornment, { kind: "outline" }>;
  node: PresentationV5Node;
  onPointerDown: (event: ReactPointerEvent<HTMLElement>, node: PresentationV5Node, mode: PresentationDragMode) => void;
}) {
  const resizeHandles = outline.handles.filter((handle): handle is PresentationResizeHandle => handle !== "rotate");
  return <>
    {resizeHandles.map((handle) => <button
      key={handle}
      className={`presentation-studio__resize-handle presentation-studio__resize-handle--${handle}`}
      type="button"
      aria-label={`从${resizeHandleLabel(handle)}调整对象大小`}
      onPointerDown={(event) => { event.stopPropagation(); onPointerDown(event, node, `resize:${handle}`); }}
    />)}
    {outline.handles.includes("rotate") && <button
      className="presentation-studio__rotate-handle"
      type="button"
      aria-label="旋转对象"
      onPointerDown={(event) => { event.stopPropagation(); onPointerDown(event, node, "rotate"); }}
    />}
  </>;
}

function resizeHandleLabel(handle: PresentationResizeHandle) {
  return ({ northWest: "左上角", north: "上方", northEast: "右上角", east: "右侧", southEast: "右下角", south: "下方", southWest: "左下角", west: "左侧" })[handle];
}

/** DOM table is a read-only projection of the canonical grid. Editing always
 * travels back through the table node registry and typed transactions. */
function PresentationTable({ node, selection, onCellSelect, pointScale }: { pointScale: number; node: PresentationTableNode; selection: TableSelection | null; onCellSelect?: (address: TableCellAddress, extend: boolean) => void }) {
  const table = node.kind.data;
  const selectionRange = selection?.nodeId === node.id ? tableRange(selection) : null;
  return <div className="presentation-studio__table" role="grid" aria-label="演示文稿表格" aria-rowcount={table.rows} aria-colcount={table.columns} style={{ gridTemplateColumns: `repeat(${table.columns}, minmax(0, 1fr))`, gridTemplateRows: `repeat(${table.rows}, minmax(0, 1fr))` }}>
    {table.cells.map((cell) => <div
      key={`${cell.row}:${cell.column}`}
      className={`presentation-studio__table-cell${selectionRange && cell.row <= selectionRange.end.row && cell.row + cell.rowSpan - 1 >= selectionRange.start.row && cell.column <= selectionRange.end.column && cell.column + cell.columnSpan - 1 >= selectionRange.start.column ? " is-grid-selected" : ""}`}
      style={{
        gridColumn: `${cell.column + 1} / span ${cell.columnSpan}`,
        gridRow: `${cell.row + 1} / span ${cell.rowSpan}`,
        fontSize: 12 * pointScale,
        background: paintColor(cell.style.fill) ?? "transparent",
        textAlign: cell.style.horizontalAlign,
        alignContent: cell.style.verticalAlign,
      }}
      role="gridcell"
      tabIndex={0}
      aria-label={`第 ${cell.row + 1} 行，第 ${cell.column + 1} 列`}
      onPointerDown={(event) => {
        event.stopPropagation();
        event.currentTarget.setPointerCapture(event.pointerId);
        onCellSelect?.({ row: cell.row, column: cell.column }, event.shiftKey || event.metaKey || event.ctrlKey);
      }}
      onPointerEnter={(event) => {
        if (event.buttons === 1) onCellSelect?.({ row: cell.row, column: cell.column }, true);
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onCellSelect?.({ row: cell.row, column: cell.column }, event.shiftKey);
        }
      }}
    ><PresentationRichText body={cell.content} pointScale={pointScale} /></div>)}
  </div>;
}

/** DOM image layer intentionally consumes immutable asset URLs only. It never owns image data or edits. */
function PresentationImage({
  artifactId,
  assetId,
  crop,
  flipH,
  flipV,
  caption,
}: {
  artifactId: string;
  assetId: string;
  crop: PresentationImageNode["kind"]["data"]["crop"];
  flipH: boolean;
  flipV: boolean;
  caption: string | null;
}) {
  const visibleWidth = 1 - crop.left - crop.right;
  const visibleHeight = 1 - crop.top - crop.bottom;
  return <div className="presentation-studio__image-frame">
    <img
      className="presentation-studio__image-content"
      draggable={false}
      src={api.assetUrl(artifactId, assetId)}
      alt={caption ?? "演示文稿图片"}
      style={{
        width: `${100 / visibleWidth}%`,
        height: `${100 / visibleHeight}%`,
        left: `${-crop.left / visibleWidth * 100}%`,
        top: `${-crop.top / visibleHeight * 100}%`,
        transform: `scale(${flipH ? -1 : 1}, ${flipV ? -1 : 1})`,
      }}
    />
    {caption && <span className="presentation-studio__image-caption">{caption}</span>}
  </div>;
}

/** Media bytes remain server-owned immutable assets. DOM media elements only render their URL. */
function PresentationMedia({ artifactId, mediaType, assetId, posterAssetId }: {
  artifactId: string;
  mediaType: "video" | "audio";
  assetId: string;
  posterAssetId: string | null;
}) {
  const source = api.assetUrl(artifactId, assetId);
  if (mediaType === "audio") {
    return <audio className="presentation-studio__media presentation-studio__media--audio" controls preload="metadata" src={source} onPointerDown={(event) => event.stopPropagation()} />;
  }
  return <video className="presentation-studio__media presentation-studio__media--video" controls preload="metadata" poster={posterAssetId ? api.assetUrl(artifactId, posterAssetId) : undefined} src={source} onPointerDown={(event) => event.stopPropagation()} />;
}
