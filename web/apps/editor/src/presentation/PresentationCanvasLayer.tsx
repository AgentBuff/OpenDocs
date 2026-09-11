import { useEffect, useRef } from "react";
import type { PresentationV5ChartSpec, PresentationV5Node, PresentationV5Transform } from "@open-office/schema";

import { deriveCanvasRenderPlan, type CanvasRenderSnapshot } from "./canvas-render-plan.js";
import {
  colorCss,
  endpointPoint,
  nodesWithConnectorPreview,
  paintColor,
  type ConnectorPreview,
} from "./presentationGeometry.js";

export function PresentationCanvasLayer({ nodes, preview, connectorPreview, scale, width, height }: { nodes: readonly PresentationV5Node[]; preview: Readonly<Record<string, PresentationV5Transform>>; connectorPreview: ConnectorPreview; scale: number; width: number; height: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const lastSnapshot = useRef<CanvasRenderSnapshot | null>(null);
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const ratio = window.devicePixelRatio || 1;
    // Feed the retained canvas a projection containing the transient endpoint
    // coordinates. This keeps the line repaint incremental while the Deck
    // itself remains untouched until pointerup.
    const renderNodes = nodesWithConnectorPreview(nodes, connectorPreview);
    // Endpoint geometry is not represented by a connector's legacy transform
    // bounds. Until the render-plan has segment-aware dirty rectangles, a
    // slide containing connectors must repaint fully to avoid stale pixels.
    const plan = deriveCanvasRenderPlan({
      previous: renderNodes.some((node) => node.kind.type === "connector") ? null : lastSnapshot.current,
      nodes: renderNodes,
      preview,
      width,
      height,
      scale,
    });
    if (element.width !== Math.floor(width * ratio)) element.width = Math.floor(width * ratio);
    if (element.height !== Math.floor(height * ratio)) element.height = Math.floor(height * ratio);
    element.style.width = `${width}px`;
    element.style.height = `${height}px`;
    const context = element.getContext("2d");
    if (!context) return;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    if (plan.kind === "noop") return;
    if (plan.kind === "full") {
      context.clearRect(0, 0, width, height);
      for (const node of plan.nodes) drawNode(context, node, preview[node.id] ?? node.transform, scale, renderNodes, preview, connectorPreview);
    } else if (plan.dirtyRect) {
      context.clearRect(plan.dirtyRect.x, plan.dirtyRect.y, plan.dirtyRect.width, plan.dirtyRect.height);
      context.save();
      context.beginPath();
      context.rect(plan.dirtyRect.x, plan.dirtyRect.y, plan.dirtyRect.width, plan.dirtyRect.height);
      context.clip();
      for (const node of plan.nodes) drawNode(context, node, preview[node.id] ?? node.transform, scale, renderNodes, preview, connectorPreview);
      context.restore();
    }
    lastSnapshot.current = plan.snapshot;
  }, [connectorPreview, height, nodes, preview, scale, width]);
  return <canvas className="presentation-studio__canvas" ref={canvas} aria-hidden="true" />;
}

function drawNode(
  context: CanvasRenderingContext2D,
  node: PresentationV5Node,
  transform: PresentationV5Transform,
  scale: number,
  nodes: readonly PresentationV5Node[],
  preview: Readonly<Record<string, PresentationV5Transform>>,
  connectorPreview: ConnectorPreview,
) {
  // Images are rendered by the DOM image layer, which can load the immutable
  // Asset endpoint without turning Canvas into a second asset cache.
  if (!node.visible || node.kind.type === "text" || node.kind.type === "image") return;
  if (node.kind.type === "connector") {
    const endpoints = connectorPreview[node.id] ?? node.kind.data;
    const start = endpointPoint(endpoints.start, nodes, preview);
    const end = endpointPoint(endpoints.end, nodes, preview);
    context.save();
    context.globalAlpha = node.opacity;
    context.strokeStyle = "#405a7d";
    context.lineWidth = Math.max(1.5, 1.5 * scale);
    context.beginPath();
    context.moveTo(start.x * scale, start.y * scale);
    context.lineTo(end.x * scale, end.y * scale);
    context.stroke();
    context.restore();
    return;
  }
  const { x, y, width, height, rotation } = transform;
  context.save();
  context.globalAlpha = node.opacity;
  context.translate((x + width / 2) * scale, (y + height / 2) * scale);
  context.rotate(rotation * Math.PI / 180);
  context.translate(-width * scale / 2, -height * scale / 2);
  if (node.kind.type === "shape") {
    context.fillStyle = paintColor(node.kind.data.style.fill) ?? "transparent";
    context.strokeStyle = colorCss(node.kind.data.style.stroke?.color) ?? "#5d6b82";
    context.lineWidth = Math.max(1, (node.kind.data.style.stroke?.width ?? 1) * scale);
    if (node.kind.data.geometry === "ellipse") {
      context.beginPath();
      context.ellipse(width * scale / 2, height * scale / 2, width * scale / 2, height * scale / 2, 0, 0, Math.PI * 2);
      context.fill();
      context.stroke();
    } else if (node.kind.data.geometry === "line" || node.kind.data.geometry === "arrow") {
      const midY = height * scale / 2;
      const endX = width * scale;
      context.beginPath();
      context.moveTo(0, midY);
      context.lineTo(endX, midY);
      context.stroke();
      if (node.kind.data.geometry === "arrow") {
        const head = Math.min(18 * scale, Math.max(6 * scale, endX / 5));
        context.beginPath();
        context.moveTo(endX, midY);
        context.lineTo(endX - head, midY - head * 0.65);
        context.moveTo(endX, midY);
        context.lineTo(endX - head, midY + head * 0.65);
        context.stroke();
      }
    } else {
      context.fillRect(0, 0, width * scale, height * scale);
      context.strokeRect(0, 0, width * scale, height * scale);
    }
  } else if (node.kind.type === "table") {
    context.fillStyle = "rgba(255,255,255,.9)";
    context.fillRect(0, 0, width * scale, height * scale);
    context.strokeStyle = "#9eacc0";
    for (let row = 0; row <= node.kind.data.rows; row += 1) {
      const line = height * scale * row / node.kind.data.rows;
      context.beginPath(); context.moveTo(0, line); context.lineTo(width * scale, line); context.stroke();
    }
    for (let column = 0; column <= node.kind.data.columns; column += 1) {
      const line = width * scale * column / node.kind.data.columns;
      context.beginPath(); context.moveTo(line, 0); context.lineTo(line, height * scale); context.stroke();
    }
  } else if (node.kind.type === "chart") {
    drawChart(context, node.kind.data.spec, width * scale, height * scale);
  } else {
    context.fillStyle = "#dce4ef";
    context.fillRect(0, 0, width * scale, height * scale);
    context.strokeStyle = "#9eacc0";
    context.strokeRect(0, 0, width * scale, height * scale);
  }
  context.restore();
}

/** Canvas is only a renderer here. All values come from the immutable chart
 * node projection and the inspector commits an entire validated ChartSpec. */
function drawChart(context: CanvasRenderingContext2D, spec: PresentationV5ChartSpec, width: number, height: number) {
  const palette = ["#165dff", "#00b42a", "#ff7d00", "#722ed1", "#f53f3f", "#14c9c9"];
  const colorFor = (index: number) => colorCss(spec.series[index]?.color) ?? palette[index % palette.length] ?? "#165dff";
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, width, height);
  const titleHeight = spec.title ? Math.min(28, height * .16) : 0;
  if (spec.title) {
    context.fillStyle = "#1d2129";
    context.font = `${Math.max(11, Math.min(16, width / 25))}px system-ui, sans-serif`;
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText(spec.title, width / 2, titleHeight / 2 + 5);
  }
  const pad = { left: Math.max(30, width * .11), right: Math.max(16, width * .05), top: titleHeight + Math.max(12, height * .06), bottom: Math.max(26, height * .16) };
  const plotWidth = Math.max(1, width - pad.left - pad.right);
  const plotHeight = Math.max(1, height - pad.top - pad.bottom);
  const allValues = spec.series.flatMap((series) => series.values);
  const max = Math.max(1, ...allValues);
  const min = Math.min(0, ...allValues);
  const range = Math.max(1, max - min);
  const yFor = (value: number) => pad.top + (max - value) / range * plotHeight;
  const xFor = (value: number) => pad.left + (value - min) / range * plotWidth;
  const baseY = yFor(0);
  context.strokeStyle = "#e5e6eb";
  context.lineWidth = 1;
  for (let step = 0; step <= 4; step += 1) {
    const y = pad.top + plotHeight * step / 4;
    context.beginPath(); context.moveTo(pad.left, y); context.lineTo(width - pad.right, y); context.stroke();
  }
  context.strokeStyle = "#86909c";
  context.beginPath(); context.moveTo(pad.left, pad.top); context.lineTo(pad.left, pad.top + plotHeight); context.lineTo(width - pad.right, pad.top + plotHeight); context.stroke();
  const count = spec.categories.length;
  if (spec.chartType === "pie") {
    const series = spec.series[0];
    if (!series) return;
    const total = series.values.reduce((sum, value) => sum + Math.max(0, value), 0) || 1;
    const radius = Math.max(8, Math.min(plotWidth, plotHeight) * .36);
    const centerX = pad.left + plotWidth / 2;
    const centerY = pad.top + plotHeight / 2;
    let start = -Math.PI / 2;
    series.values.forEach((value, index) => {
      const end = start + Math.max(0, value) / total * Math.PI * 2;
      context.fillStyle = palette[index % palette.length] ?? "#165dff";
      context.beginPath(); context.moveTo(centerX, centerY); context.arc(centerX, centerY, radius, start, end); context.closePath(); context.fill();
      start = end;
    });
  } else if (spec.chartType === "line") {
    spec.series.forEach((series, seriesIndex) => {
      context.strokeStyle = colorFor(seriesIndex);
      context.lineWidth = Math.max(1.5, Math.min(3, width / 240));
      series.values.forEach((value, index) => {
        const x = pad.left + (index + .5) * plotWidth / count;
        const y = yFor(value);
        if (index === 0) context.beginPath(), context.moveTo(x, y); else context.lineTo(x, y);
      });
      context.stroke();
      series.values.forEach((value, index) => {
        context.fillStyle = colorFor(seriesIndex);
        context.beginPath(); context.arc(pad.left + (index + .5) * plotWidth / count, yFor(value), 2.5, 0, Math.PI * 2); context.fill();
      });
    });
  } else {
    const groupCount = spec.series.length;
    const band = plotWidth / count;
    const gap = Math.max(2, band * .08);
    const barWidth = Math.max(2, (band - gap * 2) / groupCount);
    spec.series.forEach((series, seriesIndex) => {
      context.fillStyle = colorFor(seriesIndex);
      series.values.forEach((value, index) => {
        const length = Math.abs(yFor(value) - baseY);
        if (spec.chartType === "bar") {
          const slot = plotHeight / count;
          const x = xFor(0);
          const y = pad.top + index * slot + gap + seriesIndex * Math.max(2, (slot - gap * 2) / groupCount);
          context.fillRect(Math.min(x, xFor(value)), y, Math.abs(xFor(value) - x), Math.max(2, (slot - gap * 2) / groupCount));
        } else {
          const x = pad.left + index * band + gap + seriesIndex * barWidth;
          context.fillRect(x, Math.min(baseY, yFor(value)), barWidth, length);
        }
      });
    });
  }
  context.fillStyle = "#4e5969";
  context.font = `${Math.max(9, Math.min(12, width / 34))}px system-ui, sans-serif`;
  context.textAlign = "center";
  context.textBaseline = "top";
  spec.categories.forEach((category, index) => {
    if (spec.chartType !== "pie") context.fillText(category, pad.left + (index + .5) * plotWidth / count, height - pad.bottom + 7, Math.max(12, plotWidth / count - 4));
  });
}


