import type { PointerEvent as ReactPointerEvent } from "react";

import type {
  PresentationV5Node,
  PresentationV5NodeKind,
  PresentationV5Transform,
} from "@open-office/schema";

import { endpointPoint, type ConnectorPreview } from "./presentationGeometry.js";

type PresentationConnectorNode = PresentationV5Node & {
  kind: Extract<PresentationV5NodeKind, { type: "connector" }>;
};

export interface PresentationConnectorOverlayProps {
  nodes: readonly PresentationV5Node[];
  preview: Readonly<Record<string, PresentationV5Transform>>;
  connectorPreview: ConnectorPreview;
  selectedNodeId: string | null;
  scale: number;
  width: number;
  height: number;
  canEdit: boolean;
  onSelect: (nodeId: string, extend?: boolean) => void;
  onEndpointPointerDown: (
    event: ReactPointerEvent<SVGCircleElement>,
    nodeId: string,
    endpoint: "start" | "end",
  ) => void;
}

export function PresentationConnectorOverlay({
  nodes,
  preview,
  connectorPreview,
  selectedNodeId,
  scale,
  width,
  height,
  canEdit,
  onSelect,
  onEndpointPointerDown,
}: PresentationConnectorOverlayProps) {
  const connectors = nodes.filter(
    (node): node is PresentationConnectorNode => node.visible && node.kind.type === "connector",
  );
  if (connectors.length === 0) return null;

  return (
    <svg
      className="presentation-studio__connector-overlay"
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      aria-label="连接线编辑层"
    >
      {connectors.map((node) => {
        const endpoints = connectorPreview[node.id] ?? node.kind.data;
        const start = endpointPoint(endpoints.start, nodes, preview);
        const end = endpointPoint(endpoints.end, nodes, preview);
        const selected = selectedNodeId === node.id;
        return (
          <g key={node.id} className={selected ? "is-selected" : undefined}>
            <line
              className="presentation-studio__connector-hit-target"
              x1={start.x * scale}
              y1={start.y * scale}
              x2={end.x * scale}
              y2={end.y * scale}
              role="button"
              tabIndex={0}
              aria-label={`${node.name || "连接线"}${selected ? "，已选择" : ""}`}
              aria-pressed={selected}
              onPointerDown={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onSelect(node.id, event.shiftKey || event.metaKey || event.ctrlKey);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  onSelect(node.id, event.shiftKey || event.metaKey || event.ctrlKey);
                }
              }}
            />
            {selected && canEdit ? (
              <>
                <circle
                  className="presentation-studio__connector-handle"
                  cx={start.x * scale}
                  cy={start.y * scale}
                  r="5"
                  role="button"
                  tabIndex={0}
                  aria-label="拖动连接线起点"
                  onPointerDown={(event) => onEndpointPointerDown(event, node.id, "start")}
                />
                <circle
                  className="presentation-studio__connector-handle"
                  cx={end.x * scale}
                  cy={end.y * scale}
                  r="5"
                  role="button"
                  tabIndex={0}
                  aria-label="拖动连接线终点"
                  onPointerDown={(event) => onEndpointPointerDown(event, node.id, "end")}
                />
              </>
            ) : null}
          </g>
        );
      })}
    </svg>
  );
}
