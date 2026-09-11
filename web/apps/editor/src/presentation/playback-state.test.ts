import { describe, expect, it } from "vitest";

import {
  advancePlaybackClock,
  compilePlaybackCues,
  initialPlaybackState,
  nextPlaybackState,
  nodePlaybackFrame,
  normalizePlaybackCursor,
  previousPlaybackState,
  restartPlaybackState,
  seekPlaybackState,
  setPlaybackStatus,
} from "./playback-state.js";

const base = {
  slideId: "slide-1",
  nodes: [{ id: "static" }, { id: "one" }, { id: "two" }, { id: "auto" }],
  timeline: { entries: [
    { id: "auto", targetNodeId: "auto", trigger: "afterPrevious", preset: "appear", durationMs: 0, delayMs: 50, orderKey: "0" },
    { id: "a", targetNodeId: "one", trigger: "onClick", preset: "fade", durationMs: 300, delayMs: 100, orderKey: "a" },
    { id: "b", targetNodeId: "two", trigger: "afterPrevious", preset: "flyIn", durationMs: 200, delayMs: 50, orderKey: "b" },
  ] },
} as never;

describe("presentation playback state", () => {
  it("compiles triggers and timing into deterministic step-local cues", () => {
    expect(compilePlaybackCues(base)).toEqual([
      { entryId: "auto", targetNodeId: "auto", preset: "appear", step: 0, startMs: 50, endMs: 50 },
      { entryId: "a", targetNodeId: "one", preset: "fade", step: 1, startMs: 100, endMs: 400 },
      { entryId: "b", targetNodeId: "two", preset: "flyIn", step: 1, startMs: 450, endMs: 650 },
    ]);
  });

  it("continues through multiple cues for the same node in one step", () => {
    const repeated = { timeline: { entries: [
      { id: "one", targetNodeId: "target", trigger: "onClick", preset: "fade", durationMs: 100, delayMs: 0, orderKey: "a" },
      { id: "two", targetNodeId: "target", trigger: "withPrevious", preset: "wipe", durationMs: 200, delayMs: 100, orderKey: "b" },
    ] } } as never;
    expect(nodePlaybackFrame(repeated, { cueId: "one", elapsedMs: 50 }, "target")).toEqual({ visible: true, preset: "fade", progress: 0.5 });
    expect(nodePlaybackFrame(repeated, { cueId: "one", elapsedMs: 150 }, "target")).toEqual({ visible: true, preset: "wipe", progress: 0.25 });
    expect(nodePlaybackFrame(repeated, { cueId: "one", elapsedMs: 300 }, "target")).toEqual({ visible: true, preset: null, progress: 1 });
  });

  it("reconstructs the same node frame from a cursor without mutating the deck", () => {
    expect(nodePlaybackFrame(base, { cueId: null, elapsedMs: 0 }, "static")).toEqual({ visible: true, preset: null, progress: 1 });
    expect(nodePlaybackFrame(base, { cueId: null, elapsedMs: 49 }, "auto").visible).toBe(false);
    expect(nodePlaybackFrame(base, { cueId: null, elapsedMs: 50 }, "auto").visible).toBe(true);
    expect(nodePlaybackFrame(base, { cueId: "a", elapsedMs: 250 }, "one")).toEqual({ visible: true, preset: "fade", progress: 0.5 });
    expect(nodePlaybackFrame(base, { cueId: "a", elapsedMs: 449 }, "two").visible).toBe(false);
    expect(nodePlaybackFrame(base, { cueId: "a", elapsedMs: 650 }, "two").progress).toBe(1);
  });

  it("ticks, pauses and seeks a bounded view-only clock", () => {
    const started = nextPlaybackState([base], initialPlaybackState([base]));
    expect(advancePlaybackClock(base, started, 250).elapsedMs).toBe(250);
    expect(advancePlaybackClock(base, started, 9999).elapsedMs).toBe(650);
    const paused = setPlaybackStatus(started, "paused");
    expect(advancePlaybackClock(base, paused, 100)).toBe(paused);
    expect(seekPlaybackState(paused, 400).elapsedMs).toBe(400);
  });

  it("advances click steps and restores completed previous states", () => {
    const slides = [base, { slideId: "slide-2", nodes: [], timeline: { entries: [] } }] as never;
    const initial = initialPlaybackState(slides);
    expect(nextPlaybackState(slides, initial)).toEqual({ slideId: "slide-1", cueId: "a", elapsedMs: 0, status: "playing" });
    expect(nextPlaybackState(slides, { slideId: "slide-1", cueId: "a", elapsedMs: 650, status: "paused" })).toEqual({ slideId: "slide-2", cueId: null, elapsedMs: 0, status: "playing" });
    expect(previousPlaybackState(slides, { slideId: "slide-2", cueId: null, elapsedMs: 0, status: "playing" })).toEqual({ slideId: "slide-1", cueId: "a", elapsedMs: 650, status: "paused" });
    expect(restartPlaybackState(slides)).toEqual({ slideId: "slide-1", cueId: null, elapsedMs: 0, status: "playing" });
    expect(normalizePlaybackCursor(slides, { slideId: "slide-1", cueId: "a", elapsedMs: 99_999, status: "paused" })?.elapsedMs).toBe(650);
    expect(normalizePlaybackCursor(slides, { slideId: "missing", cueId: null, elapsedMs: 0, status: "paused" })).toBeNull();
    expect(normalizePlaybackCursor(slides, { slideId: "slide-1", cueId: "missing", elapsedMs: 0, status: "paused" })).toBeNull();
  });
});
