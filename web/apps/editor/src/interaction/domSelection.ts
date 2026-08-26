import type { TextRange } from "@open-office/schema/artifact";

import { scalarBoundaryAtOffset, scalarOffsetWithin } from "../utils/blockSelection.js";
import type { EditorSelection } from "./types.js";

/**
 * Reads a DOM range only when both boundaries belong to the same editable
 * block. Collapsed caret positions are selections too: treating them as
 * "nothing" is what previously let every block invent its own focus state.
 * Cross-block browser ranges deliberately remain `blocks` selections until a
 * semantic cross-block text command exists.
 */
export function readDomTextSelection(root: HTMLElement, blockId: string): Extract<EditorSelection, { kind: "text" }> | null {
  const selection = window.getSelection();
  if (!selection || selection.rangeCount === 0) return null;
  const range = selection.getRangeAt(0);
  if (!containsBoundary(root, range.startContainer) || !containsBoundary(root, range.endContainer)) return null;
  const start = scalarOffsetWithin(root, range.startContainer, range.startOffset);
  const end = scalarOffsetWithin(root, range.endContainer, range.endOffset);
  if (end < start) return null;
  return {
    kind: "text",
    blockId,
    range: { start, end },
    affinity: selection.anchorNode === range.startContainer && selection.anchorOffset === range.startOffset ? "forward" : "backward",
  };
}

/** Restores a block-local selection without splitting surrogate pairs. */
export function applyDomTextSelection(root: HTMLElement, range: TextRange): boolean {
  if (!Number.isInteger(range.start) || !Number.isInteger(range.end) || range.start < 0 || range.end < range.start) return false;
  const start = scalarBoundaryAtOffset(root, range.start);
  const end = scalarBoundaryAtOffset(root, range.end);
  const selection = window.getSelection();
  if (!start || !end || !selection) return false;
  const next = document.createRange();
  next.setStart(start.node, start.offset);
  next.setEnd(end.node, end.offset);
  selection.removeAllRanges();
  selection.addRange(next);
  return true;
}

function containsBoundary(root: Node, node: Node): boolean {
  return root === node || root.contains(node);
}
