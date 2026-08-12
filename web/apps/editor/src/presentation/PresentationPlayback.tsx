import { useCallback, useEffect, useMemo, useState } from "react";

import { OpenOfficeSdk } from "@open-office/sdk";
import type { ColorRef, PresentationV5Node, PresentationV5Transform } from "@open-office/schema";
import type { PresentationDeckProjection, PresentationSlideOutlineItem, PresentationSlideProjection } from "@open-office/schema/api";

import { api } from "../api.js";
import { initialPlaybackState, nextPlaybackState, previousPlaybackState, visibleNodeIds, type PlaybackState } from "./playback-state.js";

const sdk = new OpenOfficeSdk();

type PlaybackProjection = {
  readonly deck: PresentationDeckProjection;
  readonly outline: readonly PresentationSlideOutlineItem[];
  readonly slides: readonly PresentationSlideProjection[];
};

/**
 * Read-only slideshow renderer. It loads canonical projection endpoints and
 * stores only playback cursor/visibility state; no presentation mutation is
 * possible from this surface.
 */
export function PresentationPlayback({ artifactId, title, onExit }: { artifactId: string; title: string; onExit: () => void }) {
  const [projection, setProjection] = useState<PlaybackProjection | null>(null);
  const [state, setState] = useState<PlaybackState>(initialPlaybackState);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const [deck, outline] = await Promise.all([
        sdk.presentation(artifactId),
        sdk.presentationOutline(artifactId, { limit: 500, maxBytes: 512_000 }),
      ]);
      const items = outline.data.items;
      const slides = await Promise.all(items.map(async (item) => (await sdk.presentationSlide(artifactId, item.slideId, {
        include: ["nodes", "timeline"], maxBytes: 512_000,
      })).data));
      if (!cancelled) {
        setProjection({ deck: deck.data, outline: items, slides });
        setState(initialPlaybackState());
      }
    })().catch((reason: unknown) => !cancelled && setError(reason instanceof Error ? reason.message : String(reason)));
    return () => { cancelled = true; };
  }, [artifactId]);

  const current = projection?.slides[state.slideIndex] ?? null;
  const next = useCallback(() => projection && setState((currentState) => nextPlaybackState(projection.slides as never, currentState)), [projection]);
  const previous = useCallback(() => projection && setState((currentState) => previousPlaybackState(projection.slides as never, currentState)), [projection]);

  useEffect(() => {
    const keyboard = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
      if (["ArrowRight", "ArrowDown", "PageDown", " ", "Enter"].includes(event.key)) { event.preventDefault(); next(); }
      if (["ArrowLeft", "ArrowUp", "PageUp", "Backspace"].includes(event.key)) { event.preventDefault(); previous(); }
      if (event.key === "Escape") onExit();
    };
    window.addEventListener("keydown", keyboard);
    return () => window.removeEventListener("keydown", keyboard);
  }, [next, onExit, previous]);

  const visible = useMemo(() => current ? visibleNodeIds(current as never, state.step) : new Set<string>(), [current, state.step]);
  if (error) return <main className="presentation-playback presentation-playback--error" role="alert"><p>{error}</p><button type="button" onClick={onExit}>返回编辑</button></main>;
  if (!projection || !current) return <main className="presentation-playback" aria-live="polite">正在加载演示文稿…</main>;
  const ratio = projection.deck.pageSpec.width / projection.deck.pageSpec.height;
  const transition = transitionClass(current);
  return <main className="presentation-playback" aria-label={`${title || "演示文稿"}播放视图`}>
    <header className="presentation-playback__header">
      <strong>{title || "未命名演示文稿"}</strong>
      <span aria-live="polite">第 {state.slideIndex + 1} / {projection.slides.length} 张</span>
      <button type="button" onClick={onExit}>退出播放 Esc</button>
    </header>
    <section className="presentation-playback__viewport">
      <div className={`presentation-playback__slide ${transition}`} style={{ aspectRatio: ratio }} onClick={next} role="group" aria-label="当前幻灯片，点击播放下一步">
        {(current.nodes ?? []).filter((node) => node.visible && visible.has(node.id)).map((node) => <PlaybackNode key={node.id} node={node} artifactId={artifactId} pageWidth={projection.deck.pageSpec.width} pageHeight={projection.deck.pageSpec.height} />)}
      </div>
    </section>
    <footer className="presentation-playback__controls" aria-label="播放控制">
      <button type="button" onClick={previous} disabled={state.slideIndex === 0 && state.step === 0}>上一页</button>
      <button type="button" onClick={next} disabled={state.slideIndex === projection.slides.length - 1 && state.step === 0 && !current.timeline?.entries.some((entry) => entry.trigger === "onClick")}>下一页</button>
    </footer>
  </main>;
}

function PlaybackNode({ node, artifactId, pageWidth, pageHeight }: { node: PresentationV5Node; artifactId: string; pageWidth: number; pageHeight: number }) {
  const style = transformStyle(node.transform, node.opacity, pageWidth, pageHeight);
  if (node.kind.type === "text") return <div className="presentation-playback__node presentation-playback__text" style={style}>{node.kind.data.frame.body.text}</div>;
  if (node.kind.type === "image") return <div className="presentation-playback__node presentation-playback__image" style={style}><img src={api.assetUrl(artifactId, node.kind.data.assetId)} alt={node.kind.data.caption ?? node.altText ?? ""} /></div>;
  if (node.kind.type === "shape") return <div className={`presentation-playback__node presentation-playback__shape presentation-playback__shape--${node.kind.data.geometry}`} style={{ ...style, background: colorCss(node.kind.data.style.fill.type === "solid" ? node.kind.data.style.fill.value : null), borderColor: colorCss(node.kind.data.style.stroke?.color ?? null), borderWidth: node.kind.data.style.stroke?.width ?? 0 }} />;
  if (node.kind.type === "table") return <div className="presentation-playback__node presentation-playback__table" style={{ ...style, gridTemplateColumns: `repeat(${node.kind.data.columns}, minmax(0, 1fr))`, gridTemplateRows: `repeat(${node.kind.data.rows}, minmax(0, 1fr))` }}>{node.kind.data.cells.map((cell) => <div key={`${cell.row}:${cell.column}`} style={{ gridColumn: `${cell.column + 1} / span ${cell.columnSpan}`, gridRow: `${cell.row + 1} / span ${cell.rowSpan}`, textAlign: cell.style.horizontalAlign, alignContent: cell.style.verticalAlign }}>{cell.content.text}</div>)}</div>;
  return <div className="presentation-playback__node presentation-playback__unsupported" style={style} aria-label={`${node.kind.type}对象`} />;
}

function transformStyle(transform: PresentationV5Transform, opacity: number, pageWidth: number, pageHeight: number) {
  return { left: `${transform.x / pageWidth * 100}%`, top: `${transform.y / pageHeight * 100}%`, width: `${transform.width / pageWidth * 100}%`, height: `${transform.height / pageHeight * 100}%`, opacity, transform: `rotate(${transform.rotation}deg)` };
}

function colorCss(color: ColorRef | null): string {
  if (!color) return "transparent";
  if (color.type === "rgba") return `rgba(${color.value.r}, ${color.value.g}, ${color.value.b}, ${color.value.a / 255})`;
  return ({ background: "#fff", text: "#192033", accent1: "#2458d3", accent2: "#17a88b", accent3: "#ef9f28", accent4: "#8b5cf6", accent5: "#ef5e8d", accent6: "#40a9ff", hyperlink: "#2458d3", followedHyperlink: "#7c4ec2" } as const)[color.value];
}

function transitionClass(slide: PresentationSlideProjection): string {
  return `is-transition-${slide.transition?.kind ?? "none"}`;
}
