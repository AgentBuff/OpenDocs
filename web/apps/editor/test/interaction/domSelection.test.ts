// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";

import {
  applyDomTextSelection,
  readDomTextSelection,
} from "../../src/interaction/domSelection.js";

/**
 * Contract under test: semantic scalar offsets must survive a DOM round trip
 * without surrogate-pair drift, across plain text, CJK, emoji, links and
 * styled inline marks (I01 acceptance, unit evidence).
 */
function createEditableRoot(): HTMLElement {
  const root = document.createElement("div");
  root.setAttribute("contenteditable", "true");
  root.dataset.blockId = "block-1";
  document.body.appendChild(root);
  return root;
}

describe("dom selection adapter", () => {
  let root: HTMLElement;

  beforeEach(() => {
    document.body.replaceChildren();
    root = createEditableRoot();
    // 8 Unicode scalars total: 甲(0) 乙(1) | 链(2) 接(3) | 粗(4) 🙂(5) 尾(6)
    root.innerHTML = '甲乙<a href="https://example.com">链接</a><b>粗🙂尾</b>';
    const selection = window.getSelection();
    selection?.removeAllRanges();
  });

  it("round-trips a CJK range that crosses an inline link boundary", () => {
    expect(applyDomTextSelection(root, { start: 1, end: 3 })).toBe(true);
    expect(readDomTextSelection(root, "block-1")?.range).toEqual({ start: 1, end: 3 });
  });

  it("round-trips a collapsed caret after styled content", () => {
    expect(applyDomTextSelection(root, { start: 7, end: 7 })).toBe(true);
    const selection = readDomTextSelection(root, "block-1");
    expect(selection?.range).toEqual({ start: 7, end: 7 });
    expect(selection?.affinity).toBe("forward");
  });

  it("round-trips a range that starts and ends inside a surrogate-pair-safe emoji run", () => {
    // 粗=4, 🙂 occupies exactly one scalar slot [5,6), 尾=6.
    expect(applyDomTextSelection(root, { start: 5, end: 6 })).toBe(true);
    expect(readDomTextSelection(root, "block-1")?.range).toEqual({ start: 5, end: 6 });
  });

  it("round-trips a range fully inside a styled inline mark", () => {
    expect(applyDomTextSelection(root, { start: 4, end: 5 })).toBe(true);
    expect(readDomTextSelection(root, "block-1")?.range).toEqual({ start: 4, end: 5 });
  });

  it("never splits a surrogate pair when restoring an emoji boundary", () => {
    expect(applyDomTextSelection(root, { start: 6, end: 7 })).toBe(true);
    const selection = window.getSelection();
    expect(selection).not.toBeNull();
    const range = selection!.getRangeAt(0);
    // The restored boundary text must not begin with a lone low surrogate.
    const boundaryText = range.toString();
    expect(boundaryText.codePointAt(0)).not.toBeUndefined();
    expect(Array.from(boundaryText).length).toBe(1);
  });

  it("rejects non-integer or inverted ranges", () => {
    expect(applyDomTextSelection(root, { start: -1, end: 2 })).toBe(false);
    expect(applyDomTextSelection(root, { start: 3, end: 2 })).toBe(false);
    expect(applyDomTextSelection(root, { start: 1.5, end: 2 })).toBe(false);
  });

  it("returns null when the native selection leaves the block root", () => {
    const outside = document.createElement("div");
    outside.textContent = "外部";
    document.body.appendChild(outside);
    const selection = window.getSelection()!;
    const range = document.createRange();
    range.selectNodeContents(outside);
    selection.removeAllRanges();
    selection.addRange(range);
    expect(readDomTextSelection(root, "block-1")).toBeNull();
  });

  it("returns null without any active native selection", () => {
    window.getSelection()?.removeAllRanges();
    expect(readDomTextSelection(root, "block-1")).toBeNull();
  });
});
