import { readBlockTextSelection } from "../utils/blockSelection.js";

export interface ToolbarSelectionMarks {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
}

export interface ToolbarSelectionState {
  /** A non-empty native selection that can be addressed by patchInlineRange. */
  hasTextSelection: boolean;
  marks: ToolbarSelectionMarks;
  fontFamily: string | null;
  fontSize: number | null;
  color: string | null;
  highlight: string | null;
}

export const EMPTY_TOOLBAR_SELECTION: ToolbarSelectionState = {
  hasTextSelection: false,
  marks: { bold: false, italic: false, underline: false, strikethrough: false },
  fontFamily: null,
  fontSize: null,
  color: null,
  highlight: null,
};

/**
 * Read the browser selection as presentation state for toolbar chrome.
 *
 * The document engine remains the source of truth for writes.  This helper only
 * inspects the DOM ranges that are already owned by the Block renderer, so the
 * toolbar can expose the same active/disabled affordances as an office editor
 * without introducing a second formatting model.  Mixed ranges intentionally
 * return null for scalar values and only report a mark active when every
 * selected fragment carries that mark.
 */
export function readToolbarSelectionState(
  page: HTMLElement | null = document.querySelector<HTMLElement>(".block-editor__page"),
): ToolbarSelectionState {
  if (!page || typeof window === "undefined") return EMPTY_TOOLBAR_SELECTION;
  const ranges = readBlockTextSelection(page);
  if (ranges.length === 0) return EMPTY_TOOLBAR_SELECTION;
  const nativeSelection = window.getSelection();
  if (!nativeSelection || nativeSelection.rangeCount === 0 || nativeSelection.isCollapsed) {
    return EMPTY_TOOLBAR_SELECTION;
  }
  const nativeRange = nativeSelection.getRangeAt(0);
  const styles = collectSelectedStyles(page, nativeRange);
  if (styles.length === 0) return EMPTY_TOOLBAR_SELECTION;

  return {
    hasTextSelection: true,
    marks: {
      bold: styles.every((style) => style.bold),
      italic: styles.every((style) => style.italic),
      underline: styles.every((style) => style.underline),
      strikethrough: styles.every((style) => style.strikethrough),
    },
    fontFamily: commonValue(styles.map((style) => style.fontFamily)),
    fontSize: commonValue(styles.map((style) => style.fontSize)),
    color: commonValue(styles.map((style) => style.color)),
    highlight: commonValue(styles.map((style) => style.highlight)),
  };
}

interface SelectedStyle {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
  fontFamily: string | null;
  fontSize: number | null;
  color: string | null;
  highlight: string | null;
}

function collectSelectedStyles(page: HTMLElement, range: Range): SelectedStyle[] {
  const styles: SelectedStyle[] = [];
  for (const content of page.querySelectorAll<HTMLElement>(".block-row__content[contenteditable]") ) {
    if (!intersects(range, content)) continue;
    const walker = document.createTreeWalker(content, NodeFilter.SHOW_TEXT);
    let node: Node | null;
    while ((node = walker.nextNode())) {
      if (!node.textContent || !intersects(range, node)) continue;
      const element = node.parentElement;
      if (element) styles.push(readDomStyle(element));
    }
  }
  return styles;
}

function readDomStyle(element: HTMLElement): SelectedStyle {
  const computed = window.getComputedStyle(element);
  const decoration = computed.textDecorationLine;
  const weight = Number.parseInt(computed.fontWeight, 10);
  const background = normaliseColor(computed.backgroundColor);
  return {
    bold: weight >= 600 || element.closest("strong,b") !== null,
    italic: computed.fontStyle === "italic" || element.closest("em,i") !== null,
    underline: decoration.includes("underline") || element.closest("u") !== null,
    strikethrough: decoration.includes("line-through") || element.closest("s,strike") !== null,
    fontFamily: computed.fontFamily || null,
    fontSize: Number.isFinite(Number.parseFloat(computed.fontSize)) ? Number.parseFloat(computed.fontSize) : null,
    color: normaliseColor(computed.color),
    highlight: background === "transparent" ? null : background,
  };
}

function commonValue<T>(values: readonly T[]): T | null {
  const first = values[0];
  return values.every((value) => Object.is(value, first)) ? first ?? null : null;
}

function intersects(range: Range, node: Node): boolean {
  try {
    return range.intersectsNode(node);
  } catch {
    return false;
  }
}

function normaliseColor(value: string): string | null {
  if (!value || value === "transparent" || value === "rgba(0, 0, 0, 0)") return null;
  const match = value.match(/^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)(?:\s*,\s*[\d.]+)?\s*\)$/);
  if (!match) return value;
  return `#${[match[1], match[2], match[3]].map((part) => Number(part).toString(16).padStart(2, "0")).join("")}`;
}
