import type { PresentationV5Slide, PresentationV5TimelineEntry } from "@open-office/schema";

export type PlaybackStatus = "playing" | "paused";
export type PlaybackSlide = Pick<PresentationV5Slide, "timeline"> & Partial<Pick<PresentationV5Slide, "transition" | "id">> & { readonly slideId?: string };

/** Stable, view-only cursor. Array positions are derived from slideId at render time. */
export interface PlaybackState {
  readonly slideId: string | null;
  /** The stable onClick entry that starts the active step; null is the automatic step. */
  readonly cueId: string | null;
  readonly elapsedMs: number;
  readonly status: PlaybackStatus;
}

export interface PlaybackCue {
  readonly entryId: string;
  readonly targetNodeId: string;
  readonly preset: PresentationV5TimelineEntry["preset"];
  readonly step: number;
  readonly startMs: number;
  readonly endMs: number;
}

export interface NodePlaybackFrame {
  readonly visible: boolean;
  readonly preset: PresentationV5TimelineEntry["preset"] | null;
  readonly progress: number;
}

export function initialPlaybackState(slides: readonly PlaybackSlide[] = []): PlaybackState {
  return { slideId: slideIdentity(slides[0]), cueId: null, elapsedMs: 0, status: "playing" };
}

export function orderedTimelineEntries(slide: Pick<PresentationV5Slide, "timeline">): readonly PresentationV5TimelineEntry[] {
  return [...slide.timeline.entries].sort((left, right) => left.orderKey.localeCompare(right.orderKey));
}

/** Compile the three trigger forms into deterministic, step-local cues. */
export function compilePlaybackCues(slide: Pick<PresentationV5Slide, "timeline">): readonly PlaybackCue[] {
  const cues: PlaybackCue[] = [];
  let step = 0;
  let previous: PlaybackCue | null = null;
  for (const entry of orderedTimelineEntries(slide)) {
    if (entry.trigger === "onClick") {
      step += 1;
      previous = null;
    }
    const anchor = previous
      ? entry.trigger === "afterPrevious" ? previous.endMs : previous.startMs
      : 0;
    const startMs = safeMilliseconds(anchor + entry.delayMs);
    const cue: PlaybackCue = {
      entryId: entry.id,
      targetNodeId: entry.targetNodeId,
      preset: entry.preset,
      step,
      startMs,
      endMs: safeMilliseconds(startMs + entry.durationMs),
    };
    cues.push(cue);
    previous = cue;
  }
  return cues;
}

export function playbackStepForCursor(slide: Pick<PresentationV5Slide, "timeline">, state: Pick<PlaybackState, "cueId">): number {
  if (state.cueId === null) return 0;
  return compilePlaybackCues(slide).find((cue) => cue.entryId === state.cueId)?.step ?? 0;
}

export function playbackStepDuration(slide: PlaybackSlide, state: Pick<PlaybackState, "cueId"> | number): number {
  const step = typeof state === "number" ? state : playbackStepForCursor(slide, state);
  const timelineDuration = compilePlaybackCues(slide)
    .filter((cue) => cue.step === step)
    .reduce((duration, cue) => Math.max(duration, cue.endMs), 0);
  return step === 0 ? Math.max(timelineDuration, slide.transition?.durationMs ?? 0) : timelineDuration;
}

export function maxPlaybackStep(slide: Pick<PresentationV5Slide, "timeline">): number {
  return compilePlaybackCues(slide).reduce((maximum, cue) => Math.max(maximum, cue.step), 0);
}

export function playbackSlideIndex(slides: readonly PlaybackSlide[], state: Pick<PlaybackState, "slideId">): number {
  const index = slides.findIndex((slide) => slideIdentity(slide) === state.slideId);
  return index >= 0 ? index : 0;
}

export function normalizePlaybackCursor(slides: readonly PlaybackSlide[], state: PlaybackState): PlaybackState | null {
  const slideIndex = slides.findIndex((slide) => slideIdentity(slide) === state.slideId);
  const slide = slides[slideIndex];
  if (!slide) return null;
  if (state.cueId !== null && !compilePlaybackCues(slide).some((cue) => cue.entryId === state.cueId && cue.step > 0)) return null;
  return { ...state, elapsedMs: Math.min(state.elapsedMs, playbackStepDuration(slide, state)) };
}

export function nodePlaybackFrame(
  slide: Pick<PresentationV5Slide, "timeline">,
  state: Pick<PlaybackState, "cueId" | "elapsedMs">,
  nodeId: string,
): NodePlaybackFrame {
  const step = playbackStepForCursor(slide, state);
  const nodeCues = compilePlaybackCues(slide).filter((cue) => cue.targetNodeId === nodeId);
  if (nodeCues.length === 0) return { visible: true, preset: null, progress: 1 };
  let completed = false;
  for (const cue of nodeCues) {
    if (cue.step < step) {
      completed = true;
      continue;
    }
    if (cue.step > step) break;
    if (state.elapsedMs < cue.startMs) {
      return completed ? { visible: true, preset: null, progress: 1 } : { visible: false, preset: cue.preset, progress: 0 };
    }
    const duration = cue.endMs - cue.startMs;
    if (duration > 0 && state.elapsedMs < cue.endMs) {
      return { visible: true, preset: cue.preset, progress: clamp01((state.elapsedMs - cue.startMs) / duration) };
    }
    completed = true;
  }
  return completed ? { visible: true, preset: null, progress: 1 } : { visible: false, preset: null, progress: 0 };
}

export function advancePlaybackClock(slide: PlaybackSlide, state: PlaybackState, deltaMs: number): PlaybackState {
  if (state.status !== "playing" || deltaMs <= 0) return state;
  const duration = playbackStepDuration(slide, state);
  const elapsedMs = Math.min(duration, safeMilliseconds(state.elapsedMs + deltaMs));
  return elapsedMs === state.elapsedMs ? state : { ...state, elapsedMs };
}

export function seekPlaybackState(state: PlaybackState, elapsedMs: number): PlaybackState {
  return { ...state, elapsedMs: safeMilliseconds(elapsedMs) };
}

export function setPlaybackStatus(state: PlaybackState, status: PlaybackStatus): PlaybackState {
  return state.status === status ? state : { ...state, status };
}

export function restartPlaybackState(slides: readonly PlaybackSlide[]): PlaybackState {
  return initialPlaybackState(slides);
}

export function nextPlaybackState(slides: readonly PlaybackSlide[], state: PlaybackState): PlaybackState {
  const slideIndex = playbackSlideIndex(slides, state);
  const current = slides[slideIndex];
  if (!current) return state;
  const step = playbackStepForCursor(current, state);
  if (step < maxPlaybackStep(current)) {
    const cueId = compilePlaybackCues(current).find((cue) => cue.step === step + 1)?.entryId ?? null;
    return { ...state, cueId, elapsedMs: 0, status: "playing" };
  }
  const nextSlide = slides[slideIndex + 1];
  return nextSlide
    ? { slideId: slideIdentity(nextSlide), cueId: null, elapsedMs: 0, status: "playing" }
    : state;
}

export function previousPlaybackState(slides: readonly PlaybackSlide[], state: PlaybackState): PlaybackState {
  const slideIndex = playbackSlideIndex(slides, state);
  const current = slides[slideIndex];
  if (!current) return state;
  const step = playbackStepForCursor(current, state);
  if (step > 0) {
    const previousStep = step - 1;
    const cueId = previousStep === 0 ? null : compilePlaybackCues(current).find((cue) => cue.step === previousStep)?.entryId ?? null;
    const previous = { ...state, cueId, status: "paused" as const };
    return { ...previous, elapsedMs: playbackStepDuration(current, previous) };
  }
  const previousSlide = slides[slideIndex - 1];
  if (!previousSlide) return state;
  const lastStep = maxPlaybackStep(previousSlide);
  const cueId = lastStep === 0 ? null : compilePlaybackCues(previousSlide).find((cue) => cue.step === lastStep)?.entryId ?? null;
  const previous = { slideId: slideIdentity(previousSlide), cueId, elapsedMs: 0, status: "paused" as const };
  return { ...previous, elapsedMs: playbackStepDuration(previousSlide, previous) };
}

function slideIdentity(slide: PlaybackSlide | undefined): string | null {
  return slide?.slideId ?? slide?.id ?? null;
}

function safeMilliseconds(value: number): number {
  return Number.isFinite(value) ? Math.max(0, Math.min(Number.MAX_SAFE_INTEGER, Math.round(value))) : 0;
}

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}
