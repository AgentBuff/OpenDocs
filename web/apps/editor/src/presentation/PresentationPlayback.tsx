import { PresentationRichText, PresentationTextFrame } from "./PresentationRichText.js";
import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";

import { OpenOfficeSdk } from "@open-office/sdk";
import type { ColorRef, PresentationV5Node, PresentationV5Transform } from "@open-office/schema";
import type { PresentationDeckProjection, PresentationSlideOutlineItem, PresentationSlideProjection } from "@open-office/schema/api";

import { api } from "../api.js";
import {
  advancePlaybackClock,
  initialPlaybackState,
  maxPlaybackStep,
  nextPlaybackState,
  nodePlaybackFrame,
  normalizePlaybackCursor,
  playbackStepDuration,
  playbackSlideIndex,
  playbackStepForCursor,
  previousPlaybackState,
  restartPlaybackState,
  seekPlaybackState,
  setPlaybackStatus,
  type NodePlaybackFrame,
  type PlaybackState,
} from "./playback-state.js";
import { parsePresenterMessage, presenterChannelName, type PresenterCursorMessage } from "./presenter-session.js";
import type { PresentationLaunch } from "./usePresentationLauncher.js";

const sdk = new OpenOfficeSdk();

type PlaybackProjection = {
  readonly revision: number;
  readonly deck: PresentationDeckProjection;
  readonly outline: readonly PresentationSlideOutlineItem[];
  readonly slides: readonly PresentationSlideProjection[];
};

type PlaybackLaunch = PresentationLaunch | { readonly mode: "audience"; readonly sessionId: string };

/**
 * Read-only slideshow renderer. It loads canonical projection endpoints and
 * stores only playback cursor/visibility state; no presentation mutation is
 * possible from this surface.
 */
export function PresentationPlayback({ artifactId, title, launch, onExit }: { artifactId: string; title: string; launch: PlaybackLaunch; onExit: () => void }) {
  const [projection, setProjection] = useState<PlaybackProjection | null>(null);
  const [state, setState] = useState<PlaybackState>(initialPlaybackState);
  const [error, setError] = useState<string | null>(null);
  const [audienceConnected, setAudienceConnected] = useState(false);
  const [sessionElapsedMs, setSessionElapsedMs] = useState(0);
  const stateRef = useRef(state);
  const presenterChannelRef = useRef<BroadcastChannel | null>(null);
  stateRef.current = state;

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const [deck, outline] = await Promise.all([
        sdk.presentation(artifactId),
        sdk.presentationOutline(artifactId, { limit: 500, maxBytes: 512_000 }),
      ]);
      const items = outline.data.items;
      const slideEnvelopes = await Promise.all(items.map(async (item) => sdk.presentationSlide(artifactId, item.slideId, {
        include: ["nodes", "notes", "timeline"], maxBytes: 512_000,
      })));
      const revisions = new Set([deck.revision, outline.revision, ...slideEnvelopes.map((envelope) => envelope.revision)]);
      if (revisions.size !== 1) throw new Error("演示文稿在加载播放投影时发生更新，请重新进入播放。");
      const slides = slideEnvelopes.map((envelope) => envelope.data);
      if (!cancelled) {
        setProjection({ revision: deck.revision, deck: deck.data, outline: items, slides });
        setState(initialPlaybackState(slides as never));
      }
    })().catch((reason: unknown) => !cancelled && setError(reason instanceof Error ? reason.message : String(reason)));
    return () => { cancelled = true; };
  }, [artifactId]);

  const currentIndex = projection ? playbackSlideIndex(projection.slides as never, state) : 0;
  const current = projection?.slides[currentIndex] ?? null;
  const reducedMotion = useReducedMotion();
  const next = useCallback(() => projection && setState((currentState) => nextPlaybackState(projection.slides as never, currentState)), [projection]);
  const previous = useCallback(() => projection && setState((currentState) => previousPlaybackState(projection.slides as never, currentState)), [projection]);

  useEffect(() => {
    if (!current || state.status !== "playing" || launch.mode === "audience") return;
    let frameId = 0;
    let previousTime = performance.now();
    const tick = (time: number) => {
      const delta = time - previousTime;
      previousTime = time;
      setState((cursor) => advancePlaybackClock(current as never, cursor, reducedMotion ? Number.MAX_SAFE_INTEGER : delta));
      frameId = window.requestAnimationFrame(tick);
    };
    frameId = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(frameId);
  }, [current, launch.mode, reducedMotion, state.status]);

  useEffect(() => {
    if (launch.mode !== "presenter") return;
    const startedAt = performance.now();
    const update = () => setSessionElapsedMs(Math.max(0, Math.round(performance.now() - startedAt)));
    update();
    const timer = window.setInterval(update, 1_000);
    return () => window.clearInterval(timer);
  }, [launch.mode]);

  useEffect(() => {
    if (launch.mode === "single" || !projection || typeof BroadcastChannel === "undefined") return;
    const channel = new BroadcastChannel(presenterChannelName(launch.sessionId));
    presenterChannelRef.current = channel;
    const publish = () => channel.postMessage(cursorMessage(launch.sessionId, artifactId, projection.revision, stateRef.current));
    channel.onmessage = (event) => {
      const message = parsePresenterMessage(event.data);
      if (!message || message.sessionId !== launch.sessionId || message.artifactId !== artifactId) return;
      if (launch.mode === "presenter" && message.type === "hello") {
        setAudienceConnected(true);
        publish();
      } else if (launch.mode === "audience" && message.type === "cursor" && message.revision === projection.revision) {
        const cursor = normalizePlaybackCursor(projection.slides as never, message.cursor);
        if (cursor) setState(cursor);
      }
    };
    if (launch.mode === "audience") channel.postMessage({ version: 1, type: "hello", sessionId: launch.sessionId, artifactId });
    return () => {
      presenterChannelRef.current = null;
      channel.close();
    };
  }, [artifactId, launch, projection]);

  useEffect(() => {
    if (launch.mode !== "presenter" || !projection || typeof BroadcastChannel === "undefined") return;
    presenterChannelRef.current?.postMessage(cursorMessage(launch.sessionId, artifactId, projection.revision, state));
  }, [artifactId, launch, projection, state]);

  useEffect(() => {
    const keyboard = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
      if (launch.mode !== "audience" && ["ArrowRight", "ArrowDown", "PageDown", " ", "Enter"].includes(event.key)) { event.preventDefault(); next(); }
      if (launch.mode !== "audience" && ["ArrowLeft", "ArrowUp", "PageUp", "Backspace"].includes(event.key)) { event.preventDefault(); previous(); }
      if (event.key === "Escape") onExit();
    };
    window.addEventListener("keydown", keyboard);
    return () => window.removeEventListener("keydown", keyboard);
  }, [launch.mode, next, onExit, previous]);

  if (error) return <main className="presentation-playback presentation-playback--error" role="alert"><p>{error}</p><button type="button" onClick={onExit}>返回编辑</button></main>;
  if (!projection || !current) return <main className="presentation-playback" aria-live="polite">正在加载演示文稿…</main>;
  const ratio = projection.deck.pageSpec.width / projection.deck.pageSpec.height;
  const stepDuration = playbackStepDuration(current as never, state);
  const transitionStyle = playbackTransitionStyle(current, state, reducedMotion);
  const nextSlide = projection.slides[currentIndex + 1] ?? null;
  return <main className={`presentation-playback presentation-playback--${launch.mode}`} aria-label={`${title || "演示文稿"}播放视图`}>
    {launch.mode !== "audience" && <header className="presentation-playback__header">
      <strong>{title || "未命名演示文稿"}</strong>
      <span aria-live="polite">第 {currentIndex + 1} / {projection.slides.length} 张</span>
      <button type="button" onClick={onExit}>退出播放 Esc</button>
    </header>}
    {launch.mode === "single" && launch.popupBlocked && <p className="presentation-playback__popup-warning" role="alert">浏览器阻止了观众窗口，已切换为单窗口播放。</p>}
    <div className="presentation-playback__body">
      <section className="presentation-playback__viewport">
      <div className="presentation-playback__slide" style={{ aspectRatio: ratio, ...transitionStyle }} onClick={launch.mode === "audience" ? undefined : next} role="group" aria-label={launch.mode === "audience" ? "观众幻灯片" : "当前幻灯片，点击播放下一步"}>
        {(current.nodes ?? []).filter((node) => node.visible).map((node) => <PlaybackNode key={node.id} node={node} frame={nodePlaybackFrame(current as never, state, node.id)} reducedMotion={reducedMotion} artifactId={artifactId} pageWidth={projection.deck.pageSpec.width} pageHeight={projection.deck.pageSpec.height} />)}
      </div>
      </section>
      {launch.mode === "presenter" && <aside className="presentation-playback__presenter" aria-label="演讲者控制台">
        <div><span>放映计时</span><strong>{formatClock(sessionElapsedMs)}</strong></div>
        <div><span>观众窗口</span><strong>{audienceConnected && !launch.audienceWindow.closed ? "已连接" : "等待连接"}</strong></div>
        <section><h2>演讲者备注</h2><p>{current.notes || "此页没有备注"}</p></section>
        <section><h2>下一张</h2>{nextSlide ? <>
          <div className="presentation-playback__next-slide" style={{ aspectRatio: ratio }} aria-label="下一张幻灯片预览">
            {(nextSlide.nodes ?? []).filter((node) => node.visible).map((node) => <PlaybackNode key={node.id} node={node} frame={nodePlaybackFrame(nextSlide as never, { cueId: null, elapsedMs: playbackStepDuration(nextSlide as never, 0) }, node.id)} reducedMotion artifactId={artifactId} pageWidth={projection.deck.pageSpec.width} pageHeight={projection.deck.pageSpec.height} />)}
          </div>
          <p>{projection.outline[currentIndex + 1]?.name || "未命名幻灯片"}</p>
        </> : <p>演示结束</p>}</section>
      </aside>}
    </div>
    {launch.mode !== "audience" && <footer className="presentation-playback__controls" aria-label="播放控制">
      <button type="button" onClick={previous} disabled={currentIndex === 0 && state.cueId === null}>上一页</button>
      <button type="button" onClick={() => setState(restartPlaybackState(projection.slides as never))}>从头播放</button>
      <button type="button" onClick={() => setState((cursor) => setPlaybackStatus(cursor, cursor.status === "playing" ? "paused" : "playing"))}>{state.status === "playing" ? "暂停" : "继续"}</button>
      <label className="presentation-playback__seek">当前动画
        <input aria-label="当前动画进度" type="range" min="0" max={Math.max(1, stepDuration)} value={Math.min(state.elapsedMs, Math.max(1, stepDuration))} onChange={(event) => setState((cursor) => setPlaybackStatus(seekPlaybackState(cursor, Number(event.target.value)), "paused"))} />
      </label>
      <button type="button" onClick={next} disabled={currentIndex === projection.slides.length - 1 && playbackStepForCursor(current as never, state) >= maxPlaybackStep(current as never)}>下一页</button>
    </footer>}
  </main>;
}

function cursorMessage(sessionId: string, artifactId: string, revision: number, cursor: PlaybackState): PresenterCursorMessage {
  return { version: 1, type: "cursor", sessionId, artifactId, revision, cursor };
}

function formatClock(milliseconds: number): string {
  const totalSeconds = Math.floor(milliseconds / 1_000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function PlaybackNode({ node, frame, reducedMotion, artifactId, pageWidth, pageHeight }: { node: PresentationV5Node; frame: NodePlaybackFrame; reducedMotion: boolean; artifactId: string; pageWidth: number; pageHeight: number }) {
  const style = transformStyle(node.transform, node.opacity, pageWidth, pageHeight, playbackNodeEffectStyle(frame, reducedMotion));
  if (node.kind.type === "text") {
    const frame = node.kind.data.frame;
    return <PresentationTextFrame className="presentation-playback__node presentation-playback__text" body={frame.body} pointScale={`${12700 / pageWidth * 100}cqw`} autoFit={frame.autoFit} verticalAlign={frame.verticalAlign} padding={`${frame.padding.top / pageWidth * 100}cqw ${frame.padding.right / pageWidth * 100}cqw ${frame.padding.bottom / pageWidth * 100}cqw ${frame.padding.left / pageWidth * 100}cqw`} style={{ ...style, fontSize: `${12 * 12700 / pageWidth * 100}cqw` }} />;
  }
  if (node.kind.type === "image") return <div className="presentation-playback__node presentation-playback__image" style={style}><img src={api.assetUrl(artifactId, node.kind.data.assetId)} alt={node.kind.data.caption ?? node.altText ?? ""} /></div>;
  if (node.kind.type === "shape") return <div className={`presentation-playback__node presentation-playback__shape presentation-playback__shape--${node.kind.data.geometry}`} style={{ ...style, background: colorCss(node.kind.data.style.fill.type === "solid" ? node.kind.data.style.fill.value : null), borderColor: colorCss(node.kind.data.style.stroke?.color ?? null), borderWidth: node.kind.data.style.stroke?.width ?? 0 }} />;
  if (node.kind.type === "table") return <div className="presentation-playback__node presentation-playback__table" style={{ ...style, gridTemplateColumns: `repeat(${node.kind.data.columns}, minmax(0, 1fr))`, gridTemplateRows: `repeat(${node.kind.data.rows}, minmax(0, 1fr))` }}>{node.kind.data.cells.map((cell) => <div key={`${cell.row}:${cell.column}`} style={{ gridColumn: `${cell.column + 1} / span ${cell.columnSpan}`, gridRow: `${cell.row + 1} / span ${cell.rowSpan}`, fontSize: `${12 * 12700 / pageWidth * 100}cqw`, textAlign: cell.style.horizontalAlign, alignContent: cell.style.verticalAlign }}><PresentationRichText body={cell.content} pointScale={`${12700 / pageWidth * 100}cqw`} /></div>)}</div>;
  return <div className="presentation-playback__node presentation-playback__unsupported" style={style} aria-label={`${node.kind.type}对象`} />;
}

function transformStyle(transform: PresentationV5Transform, opacity: number, pageWidth: number, pageHeight: number, effect: PlaybackEffectStyle): CSSProperties {
  return { left: `${transform.x / pageWidth * 100}%`, top: `${transform.y / pageHeight * 100}%`, width: `${transform.width / pageWidth * 100}%`, height: `${transform.height / pageHeight * 100}%`, opacity: opacity * effect.opacity, visibility: effect.visible ? "visible" : "hidden", clipPath: effect.clipPath, transform: `translateX(${effect.translateXPercent}%) rotate(${transform.rotation}deg)` };
}

function colorCss(color: ColorRef | null): string {
  if (!color) return "transparent";
  if (color.type === "rgba") return `rgba(${color.value.r}, ${color.value.g}, ${color.value.b}, ${color.value.a / 255})`;
  return ({ background: "#fff", text: "#192033", accent1: "#2458d3", accent2: "#17a88b", accent3: "#ef9f28", accent4: "#8b5cf6", accent5: "#ef5e8d", accent6: "#40a9ff", hyperlink: "#2458d3", followedHyperlink: "#7c4ec2" } as const)[color.value];
}

type PlaybackEffectStyle = {
  readonly visible: boolean;
  readonly opacity: number;
  readonly translateXPercent: number;
  readonly clipPath?: string;
};

function playbackNodeEffectStyle(frame: NodePlaybackFrame, reducedMotion: boolean): PlaybackEffectStyle {
  if (!frame.visible) return { visible: false, opacity: 0, translateXPercent: 0 };
  const progress = reducedMotion ? 1 : frame.progress;
  if (frame.preset === "fade") return { visible: true, opacity: progress, translateXPercent: 0 };
  if (frame.preset === "flyIn") return { visible: true, opacity: progress, translateXPercent: (1 - progress) * 12 };
  if (frame.preset === "wipe") return { visible: true, opacity: 1, translateXPercent: 0, clipPath: `inset(0 ${(1 - progress) * 100}% 0 0)` };
  return { visible: true, opacity: 1, translateXPercent: 0 };
}

function playbackTransitionStyle(slide: PresentationSlideProjection, state: PlaybackState, reducedMotion: boolean): CSSProperties {
  const transition = slide.transition;
  if (!transition || transition.kind === "none" || reducedMotion || state.cueId !== null) return {};
  const progress = transition.durationMs === 0 ? 1 : Math.max(0, Math.min(1, state.elapsedMs / transition.durationMs));
  if (transition.kind === "fade") return { opacity: 0.1 + progress * 0.9 };
  if (transition.kind === "push") return { opacity: 0.5 + progress * 0.5, transform: `translateX(${(1 - progress) * 4}%)` };
  return { clipPath: `inset(0 ${(1 - progress) * 100}% 0 0)` };
}

function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(() => typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches);
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const query = matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReduced(query.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return reduced;
}
