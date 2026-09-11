import type { ComponentProps } from "react";
import type { PresentationV5Node } from "@open-office/schema";
import type { PresentationPresenceParticipant, PresentationSlideProjection } from "@open-office/schema/api";

import { SlideNode } from "./PresentationStageNodes.js";
import { createNodeContext, presentationNodeUiRegistry } from "./presentationNodeContext.js";
import type { TableSelection } from "./presentationTableSelection.js";
import type { PresentationSnapGuide } from "./interactions.js";

export function PresentationSnapGuides({ guides, scale }: { guides: readonly PresentationSnapGuide[]; scale: number }) {
  if (guides.length === 0) return null;
  return <div className="presentation-studio__snap-guides" aria-hidden="true">
    {guides.map((guide) => <span
      key={`${guide.axis}:${guide.position}`}
      className={`presentation-studio__snap-guide presentation-studio__snap-guide--${guide.axis}`}
      style={guide.axis === "x" ? { left: guide.position * scale } : { top: guide.position * scale }}
    />)}
  </div>;
}

export function PresenceOverlay({
  participants,
  nodes,
  scale,
}: {
  participants: readonly PresentationPresenceParticipant[];
  nodes: readonly PresentationV5Node[];
  scale: number;
}) {
  const nodeById = new Map(nodes.map((node) => [node.id, node]));
  return <div className="presentation-studio__presence-layer" aria-live="polite" aria-label="协作者状态">
    {participants.flatMap((participant) => participant.selectedNodeIds.map((nodeId) => {
      const node = nodeById.get(nodeId);
      if (!node) return null;
      const transform = node.transform;
      return <div
        key={`${participant.sessionId}:${nodeId}`}
        className="presentation-studio__remote-selection"
        style={{ left: transform.x * scale, top: transform.y * scale, width: transform.width * scale, height: transform.height * scale }}
        title={`${participant.displayName} 正在选择此对象`}
      />;
    }))}
    {participants.map((participant) => participant.cursor && (
      <div
        key={`${participant.sessionId}:cursor`}
        className="presentation-studio__remote-cursor"
        style={{ left: participant.cursor.x * scale, top: participant.cursor.y * scale }}
      >
        <span>{participant.displayName}</span>
      </div>
    ))}
  </div>;
}

export function SlideNodeWithUi({
  artifactId,
  revision,
  slide,
  node,
  selectedNodeIds,
  tableSelection,
  editing,
  availableCapabilities,
  ...props
}: Omit<ComponentProps<typeof SlideNode>, "artifactId" | "adornments" | "unsupportedReason"> & {
  artifactId: string;
  revision: number;
  slide: PresentationSlideProjection;
  selectedNodeIds: readonly string[];
  tableSelection: TableSelection | null;
  editing: boolean;
  availableCapabilities: ReadonlySet<string>;
}) {
  const ui = presentationNodeUiRegistry.resolve(createNodeContext({
    artifactId,
    revision,
    slide,
    node,
    selectedNodeIds,
    mode: editing ? "text" : "node",
    availableCapabilities,
  }));
  const unsupportedReason = ui.renderModel.kind === "unsupported" ? ui.renderModel.reason : null;
  return <SlideNode {...props} artifactId={artifactId} node={node} editing={editing} adornments={ui.adornments} unsupportedReason={unsupportedReason} tableSelection={tableSelection} />;
}
