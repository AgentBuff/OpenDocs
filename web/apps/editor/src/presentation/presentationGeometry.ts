import type { CSSProperties } from "react";
import type {
  ColorRef,
  ConnectorEndpoint,
  PresentationV5Node,
  PresentationV5Transform,
  ThemeColorToken,
} from "@open-office/schema";

export type ConnectorPreview = Record<string, {
  start: ConnectorEndpoint;
  end: ConnectorEndpoint;
}>;

export function endpointPoint(
  endpoint: ConnectorEndpoint,
  nodes: readonly PresentationV5Node[],
  preview: Readonly<Record<string, PresentationV5Transform>>,
): { x: number; y: number } {
  if (endpoint.type === "free") return endpoint.value;
  const target = nodes.find((node) => node.id === endpoint.value.nodeId);
  const transform = target ? (preview[target.id] ?? target.transform) : null;
  if (!transform) return { x: 0, y: 0 };
  const centerX = transform.x + transform.width / 2;
  const centerY = transform.y + transform.height / 2;
  switch (endpoint.value.anchor) {
    case "top": return { x: centerX, y: transform.y };
    case "right": return { x: transform.x + transform.width, y: centerY };
    case "bottom": return { x: centerX, y: transform.y + transform.height };
    case "left": return { x: transform.x, y: centerY };
    case "center": return { x: centerX, y: centerY };
  }
}

export function snapConnectorEndpoint(
  endpoint: Extract<ConnectorEndpoint, { type: "free" }>,
  connectorId: string,
  nodes: readonly PresentationV5Node[],
  preview: Readonly<Record<string, PresentationV5Transform>>,
): ConnectorEndpoint {
  const target = [...nodes].reverse().find((candidate) => {
    if (candidate.id === connectorId || !candidate.visible) return false;
    const transform = preview[candidate.id] ?? candidate.transform;
    return endpoint.value.x >= transform.x
      && endpoint.value.x <= transform.x + transform.width
      && endpoint.value.y >= transform.y
      && endpoint.value.y <= transform.y + transform.height;
  });
  return target
    ? { type: "node", value: { nodeId: target.id, anchor: "center" } }
    : endpoint;
}

export function sameConnectorEndpoints(
  left: { start: ConnectorEndpoint; end: ConnectorEndpoint },
  right: { start: ConnectorEndpoint; end: ConnectorEndpoint },
) {
  return JSON.stringify(left) === JSON.stringify(right);
}

export function withoutConnectorPreview(
  preview: ConnectorPreview,
  nodeId: string,
): ConnectorPreview {
  const { [nodeId]: _removed, ...remaining } = preview;
  return remaining;
}

export function nodesWithConnectorPreview(
  nodes: readonly PresentationV5Node[],
  preview: ConnectorPreview,
): readonly PresentationV5Node[] {
  return nodes.map((node) => node.kind.type === "connector" && preview[node.id]
    ? { ...node, kind: { type: "connector", data: preview[node.id] } }
    : node);
}

export function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function nodeStyle(
  transform: PresentationV5Transform,
  scale: number,
  opacity: number,
): CSSProperties {
  return {
    left: transform.x * scale,
    top: transform.y * scale,
    width: transform.width * scale,
    height: transform.height * scale,
    opacity,
    transform: `rotate(${transform.rotation}deg)`,
  };
}

export function paintColor(paint: unknown): string | null {
  if (!paint || typeof paint !== "object") return null;
  const candidate = paint as { type?: string; value?: ColorRef };
  return candidate.type === "solid" ? colorCss(candidate.value) : null;
}

export function colorCss(color: ColorRef | undefined | null): string | null {
  if (!color) return null;
  if (color.type === "rgba") {
    return `rgba(${color.value.r}, ${color.value.g}, ${color.value.b}, ${color.value.a / 255})`;
  }
  const themes: Record<ThemeColorToken, string> = {
    background: "#ffffff",
    text: "#192033",
    accent1: "#2458d3",
    accent2: "#17a88b",
    accent3: "#ef9f28",
    accent4: "#8b5cf6",
    accent5: "#ef5e8d",
    accent6: "#40a9ff",
    hyperlink: "#2458d3",
    followedHyperlink: "#7c4ec2",
  };
  return themes[color.value];
}
