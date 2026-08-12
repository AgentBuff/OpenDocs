import type { PresentationV5Slide, PresentationV5TimelineEntry } from "@open-office/schema";

/**
 * View-only playback state. It deliberately contains ids and cursors only:
 * the deck remains the canonical immutable source for slides and timelines.
 */
export interface PlaybackState {
  readonly slideIndex: number;
  readonly step: number;
}

export function initialPlaybackState(): PlaybackState {
  return { slideIndex: 0, step: 0 };
}

export function orderedTimelineEntries(slide: Pick<PresentationV5Slide, "timeline">): readonly PresentationV5TimelineEntry[] {
  return [...slide.timeline.entries].sort((left, right) => left.orderKey.localeCompare(right.orderKey));
}

/** A click reveals exactly one click-triggered entry and its dependent chain. */
export function revealCountForStep(slide: Pick<PresentationV5Slide, "timeline">, step: number): number {
  const entries = orderedTimelineEntries(slide);
  if (entries.length === 0 || step <= 0) return 0;
  let clickSteps = 0;
  let visible = 0;
  for (const entry of entries) {
    if (entry.trigger === "onClick") clickSteps += 1;
    if (clickSteps > step) break;
    visible += 1;
  }
  return visible;
}

export function visibleNodeIds(slide: Pick<PresentationV5Slide, "nodes" | "timeline">, step: number): ReadonlySet<string> {
  const entries = orderedTimelineEntries(slide);
  const animated = new Set(entries.map((entry) => entry.targetNodeId));
  const visible = new Set((slide.nodes ?? []).filter((node) => !animated.has(node.id)).map((node) => node.id));
  for (const entry of entries.slice(0, revealCountForStep(slide, step))) visible.add(entry.targetNodeId);
  return visible;
}

export function maxPlaybackStep(slide: Pick<PresentationV5Slide, "timeline">): number {
  return orderedTimelineEntries(slide).filter((entry) => entry.trigger === "onClick").length;
}

export function nextPlaybackState(slides: readonly Pick<PresentationV5Slide, "timeline">[], state: PlaybackState): PlaybackState {
  const current = slides[state.slideIndex];
  if (!current) return state;
  if (state.step < maxPlaybackStep(current)) return { ...state, step: state.step + 1 };
  return state.slideIndex < slides.length - 1 ? { slideIndex: state.slideIndex + 1, step: 0 } : state;
}

export function previousPlaybackState(slides: readonly Pick<PresentationV5Slide, "timeline">[], state: PlaybackState): PlaybackState {
  if (state.step > 0) return { ...state, step: state.step - 1 };
  if (state.slideIndex === 0) return state;
  const previousIndex = state.slideIndex - 1;
  return { slideIndex: previousIndex, step: maxPlaybackStep(slides[previousIndex]!) };
}
