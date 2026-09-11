import { PresentationRichText, PresentationTextFrame } from "./PresentationRichText.js";
import { useEffect, useMemo, useRef, useState } from "react";

import type { PresentationDeckProjection, PresentationSlideOutlineItem, PresentationSlideProjection } from "@open-office/schema/api";
import type { ColorRef, Paint, PresentationV5Node } from "@open-office/schema";
import { OpenOfficeSdk } from "@open-office/sdk";

/**
 * Read-only thumbnail cache. It stores projection DTOs keyed by slide and
 * revision, never a Deck or a writable scene graph. Invalidations are emitted
 * by the canonical Presentation engine as `presentation.thumbnailInvalidated`.
 */
export class PresentationThumbnailProjectionStore {
  private readonly slides = new Map<string, { revision: number; slide: PresentationSlideProjection }>();
  private readonly dirty = new Set<string>();

  markDirty(slideIds: Iterable<string>) {
    for (const slideId of slideIds) this.dirty.add(slideId);
  }

  read(slideId: string): PresentationSlideProjection | null {
    return this.slides.get(slideId)?.slide ?? null;
  }

  needsRead(slideId: string): boolean {
    return this.dirty.has(slideId) || !this.slides.has(slideId);
  }

  accept(envelope: { revision: number; data: PresentationSlideProjection }) {
    this.slides.set(envelope.data.slideId, { revision: envelope.revision, slide: envelope.data });
    this.dirty.delete(envelope.data.slideId);
  }

  removeAbsent(slideIds: Iterable<string>) {
    const live = new Set(slideIds);
    for (const id of this.slides.keys()) if (!live.has(id)) this.slides.delete(id);
    for (const id of this.dirty) if (!live.has(id)) this.dirty.delete(id);
  }
}

export function thumbnailInvalidationIds(events: readonly { typeId: string; payload: unknown }[]): string[] {
  return events.flatMap((event) => {
    if (event.typeId !== "presentation.thumbnailInvalidated" || !event.payload || typeof event.payload !== "object") return [];
    const slideId = (event.payload as Record<string, unknown>).slideId;
    return typeof slideId === "string" && slideId ? [slideId] : [];
  });
}

export function PresentationThumbnailNavigator({
  artifactId,
  deck,
  slides,
  activeSlideId,
  dirtySlideIds,
  onOpen,
}: {
  artifactId: string;
  deck: PresentationDeckProjection;
  slides: readonly PresentationSlideOutlineItem[];
  activeSlideId: string | null;
  dirtySlideIds: readonly string[];
  onOpen(slideId: string): void;
}) {
  const sdk = useMemo(() => new OpenOfficeSdk(), []);
  const store = useRef(new PresentationThumbnailProjectionStore()).current;
  const [version, setVersion] = useState(0);

  useEffect(() => {
    const slideIds = slides.map((slide) => slide.slideId);
    store.removeAbsent(slideIds);
    store.markDirty(dirtySlideIds);
    let cancelled = false;
    const pending = slideIds.filter((slideId) => store.needsRead(slideId));
    // Bounded parallelism keeps navigator rendering independent from the main
    // stage and prevents a large deck from becoming a request burst.
    const read = async () => {
      for (let start = 0; start < pending.length; start += 4) {
        await Promise.all(pending.slice(start, start + 4).map(async (slideId) => {
          const envelope = await sdk.presentationSlide(artifactId, slideId, { include: ["nodes"], maxBytes: 256_000 });
          if (!cancelled) store.accept(envelope);
        }));
        if (cancelled) return;
        setVersion((current) => current + 1);
      }
    };
    void read();
    return () => { cancelled = true; };
  }, [artifactId, dirtySlideIds, sdk, slides, store]);

  return <>
    {slides.map((item, index) => (
      <button
        type="button"
        className={`presentation-studio__thumbnail ${item.slideId === activeSlideId ? "is-active" : ""}`}
        key={item.slideId}
        onClick={() => onOpen(item.slideId)}
      >
        <span className="presentation-studio__thumbnail-number">{index + 1}</span>
        <span className="presentation-studio__thumbnail-page">
          <ThumbnailSurface key={`${item.slideId}:${version}`} slide={store.read(item.slideId)} deck={deck} label={item.name || `幻灯片 ${index + 1}`} />
        </span>
      </button>
    ))}
  </>;
}

function ThumbnailSurface({ slide, deck, label }: { slide: PresentationSlideProjection | null; deck: PresentationDeckProjection; label: string }) {
  if (!slide?.nodes) return <span>{label}</span>;
  const { width, height } = deck.pageSpec;
  return <svg viewBox={`0 0 ${width} ${height}`} role="img" aria-label={label} className="presentation-studio__thumbnail-svg">
    {slide.nodes.filter((node) => node.visible).map((node) => <ThumbnailNode key={node.id} node={node} />)}
  </svg>;
}

function ThumbnailNode({ node }: { node: PresentationV5Node }) {
  const { x, y, width, height, rotation } = node.transform;
  const transform = `rotate(${rotation} ${x + width / 2} ${y + height / 2})`;
  if (node.kind.type === "text") {
    const frame = node.kind.data.frame;
    // Render text in CSS pixels inside the EMU SVG coordinate system.
    return <g transform={transform} opacity={node.opacity}><foreignObject width={width / 9525} height={height / 9525} transform={`translate(${x} ${y}) scale(9525)`}>
      <PresentationTextFrame className="presentation-studio__thumbnail-text" body={frame.body} autoFit={frame.autoFit} verticalAlign={frame.verticalAlign} padding={`${frame.padding.top / 9525}px ${frame.padding.right / 9525}px ${frame.padding.bottom / 9525}px ${frame.padding.left / 9525}px`} style={{ width: "100%", height: "100%", overflow: frame.autoFit === "resizeShape" ? "visible" : "hidden", boxSizing: "border-box", whiteSpace: "pre-wrap", color: "#26354d", fontSize: 16, lineHeight: 1.35 }} />
    </foreignObject></g>;
  }
  if (node.kind.type === "shape") {
    const fill = paintCss(node.kind.data.style.fill) ?? "transparent";
    const stroke = colorCss(node.kind.data.style.stroke?.color) ?? "#5d6b82";
    return node.kind.data.geometry === "ellipse"
      ? <ellipse cx={x + width / 2} cy={y + height / 2} rx={width / 2} ry={height / 2} transform={transform} fill={fill} stroke={stroke} />
      : <rect x={x} y={y} width={width} height={height} transform={transform} fill={fill} stroke={stroke} />;
  }
  if (node.kind.type === "table") return <g transform={transform} opacity={node.opacity}><foreignObject width={width / 9525} height={height / 9525} transform={`translate(${x} ${y}) scale(9525)`}>
    <div className="presentation-studio__thumbnail-table" style={{ display: "grid", width: "100%", height: "100%", overflow: "hidden", fontSize: 16, lineHeight: 1.35, color: "#26354d", gridTemplateColumns: `repeat(${node.kind.data.columns}, minmax(0, 1fr))`, gridTemplateRows: `repeat(${node.kind.data.rows}, minmax(0, 1fr))` }}>{node.kind.data.cells.map(cell => <div key={`${cell.row}:${cell.column}`} style={{ gridColumn: `${cell.column + 1} / span ${cell.columnSpan}`, gridRow: `${cell.row + 1} / span ${cell.rowSpan}`, overflow: "hidden", whiteSpace: "pre-wrap", border: "1px solid #9eacc0", padding: "5px 7px", textAlign: cell.style.horizontalAlign, alignContent: cell.style.verticalAlign, background: paintCss(cell.style.fill) ?? "#fff" }}><PresentationRichText body={cell.content} /></div>)}</div>
  </foreignObject></g>;
  return <rect x={x} y={y} width={width} height={height} transform={transform} fill="#dce4ef" stroke="#9eacc0" />;
}

function paintCss(paint: Paint): string | null { return paint.type === "solid" ? colorCss(paint.value) : null; }
function colorCss(color: ColorRef | null | undefined): string | null {
  if (!color) return null;
  if (color.type === "rgba") return `rgba(${color.value.r}, ${color.value.g}, ${color.value.b}, ${color.value.a / 255})`;
  return ({ background: "#fff", text: "#26354d", accent1: "#3b82f6", accent2: "#14b8a6", accent3: "#f59e0b", accent4: "#8b5cf6", accent5: "#ef4444", accent6: "#06b6d4", hyperlink: "#2563eb", followedHyperlink: "#7c3aed" } as const)[color.value];
}
