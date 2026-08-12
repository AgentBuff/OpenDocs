import { describe, expect, it } from "vitest";

import { initialPlaybackState, nextPlaybackState, previousPlaybackState, visibleNodeIds } from "./playback-state.js";

const base = {
  nodes: [
    { id: "static" }, { id: "one" }, { id: "two" },
  ],
  timeline: { entries: [
    { id: "a", targetNodeId: "one", trigger: "onClick", orderKey: "a" },
    { id: "b", targetNodeId: "two", trigger: "afterPrevious", orderKey: "b" },
  ] },
} as never;

describe("presentation playback state", () => {
  it("reveals only semantic timeline entries without changing the deck", () => {
    expect([...visibleNodeIds(base, 0)]).toEqual(["static"]);
    expect([...visibleNodeIds(base, 1)]).toEqual(["static", "one", "two"]);
  });

  it("advances click steps before it changes the slide", () => {
    const slides = [base, { nodes: [], timeline: { entries: [] } }] as never;
    expect(nextPlaybackState(slides, initialPlaybackState())).toEqual({ slideIndex: 0, step: 1 });
    expect(nextPlaybackState(slides, { slideIndex: 0, step: 1 })).toEqual({ slideIndex: 1, step: 0 });
    expect(previousPlaybackState(slides, { slideIndex: 1, step: 0 })).toEqual({ slideIndex: 0, step: 1 });
  });
});
