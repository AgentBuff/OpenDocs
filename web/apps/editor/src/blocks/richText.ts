import type { InlineStyle, RichText } from "@open-office/schema/artifact";

export function richTextToDom(content: RichText | null): DocumentFragment {
  const fragment = document.createDocumentFragment();
  if (!content?.text) return fragment;
  const runs = content.runs.length > 0 ? content.runs : [{ start: 0, end: Array.from(content.text).length, style: emptyInlineStyle() }];
  const chars = Array.from(content.text);
  for (const run of runs) {
    const span = document.createElement("span");
    span.dataset.runStart = String(run.start);
    applyRunStyles(span, run.style);
    for (const [lineIndex, line] of chars.slice(run.start, run.end).join("").split("\n").entries()) {
      if (lineIndex > 0) span.append(document.createElement("br"));
      span.append(document.createTextNode(line));
    }
    let wrapped: HTMLElement = span;
    if (run.style.bold) wrapped = wrapDom("strong", wrapped);
    if (run.style.italic) wrapped = wrapDom("em", wrapped);
    if (run.style.underline) wrapped = wrapDom("u", wrapped);
    if (run.style.strikethrough) wrapped = wrapDom("s", wrapped);
    fragment.append(wrapped);
  }
  return fragment;
}

export function richTextFromHtml(element: HTMLElement): RichText {
  const textParts: string[] = [];
  const runs: RichText["runs"] = [];
  let offset = 0;
  const append = (text: string, style: InlineStyle) => {
    if (!text) return;
    const length = Array.from(text).length;
    textParts.push(text);
    const previous = runs[runs.length - 1];
    if (previous && JSON.stringify(previous.style) === JSON.stringify(style) && previous.end === offset) {
      previous.end += length;
    } else {
      runs.push({ start: offset, end: offset + length, style });
    }
    offset += length;
  };
  const visit = (node: Node, style: InlineStyle) => {
    if (node.nodeType === Node.TEXT_NODE) {
      append(node.textContent ?? "", style);
      return;
    }
    if (!(node instanceof HTMLElement)) return;
    if (node.tagName === "BR") {
      append("\n", style);
      return;
    }
    const next = { ...style };
    if (node.tagName === "STRONG" || node.tagName === "B") next.bold = true;
    if (node.tagName === "EM" || node.tagName === "I") next.italic = true;
    if (node.tagName === "U") next.underline = true;
    if (node.tagName === "S" || node.tagName === "STRIKE") next.strikethrough = true;
    readRunStyles(node, next);
    node.childNodes.forEach((child) => visit(child, next));
  };
  element.childNodes.forEach((child) => visit(child, emptyInlineStyle()));
  return { text: textParts.join(""), runs };
}

/**
 * Return a block-local rich-text slice using Unicode code-point offsets.
 * Selection offsets are measured with Array.from throughout the editor, so
 * astral characters cannot split a surrogate pair during deletion.
 */
export function sliceRichText(content: RichText, start: number, end: number): RichText {
  const chars = Array.from(content.text);
  const from = Math.max(0, Math.min(chars.length, Math.floor(start)));
  const to = Math.max(from, Math.min(chars.length, Math.floor(end)));
  if (from === to) return { text: "", runs: [] };
  const sourceRuns = content.runs.length > 0
    ? content.runs
    : [{ start: 0, end: chars.length, style: emptyInlineStyle() }];
  const runs = sourceRuns.flatMap((run) => {
    const runStart = Math.max(from, run.start);
    const runEnd = Math.min(to, run.end);
    return runEnd > runStart
      ? [{ start: runStart - from, end: runEnd - from, style: { ...run.style } }]
      : [];
  });
  return { text: chars.slice(from, to).join(""), runs };
}

/** Concatenate rich-text fragments while retaining each fragment's styles. */
export function concatRichText(...fragments: readonly RichText[]): RichText {
  const textParts: string[] = [];
  const runs: RichText["runs"] = [];
  let offset = 0;
  for (const fragment of fragments) {
    const chars = Array.from(fragment.text);
    if (chars.length === 0) continue;
    textParts.push(fragment.text);
    const sourceRuns = fragment.runs.length > 0
      ? fragment.runs
      : [{ start: 0, end: chars.length, style: emptyInlineStyle() }];
    for (const run of sourceRuns) {
      runs.push({ start: run.start + offset, end: run.end + offset, style: { ...run.style } });
    }
    offset += chars.length;
  }
  return { text: textParts.join(""), runs };
}

function emptyInlineStyle(): InlineStyle {
  return {
    bold: false,
    italic: false,
    underline: false,
    strikethrough: false,
    fontFamily: null,
    fontSize: null,
    color: null,
    highlight: null,
    verticalAlign: null,
  };
}

function applyRunStyles(element: HTMLSpanElement, style: InlineStyle): void {
  if (style.fontFamily) element.style.fontFamily = style.fontFamily;
  if (typeof style.fontSize === "number" && Number.isFinite(style.fontSize)) {
    element.style.fontSize = `${style.fontSize}px`;
  }
  if (style.color) element.style.color = style.color;
  if (style.highlight) element.style.backgroundColor = style.highlight;
}

function readRunStyles(node: HTMLElement, style: InlineStyle): void {
  if (node.style.fontFamily) style.fontFamily = node.style.fontFamily;
  if (node.style.fontSize) {
    const size = Number.parseFloat(node.style.fontSize);
    if (Number.isFinite(size)) style.fontSize = size;
  }
  if (node.style.color) style.color = node.style.color;
  if (node.style.backgroundColor) style.highlight = node.style.backgroundColor;
}

function wrapDom(tag: "strong" | "em" | "u" | "s", child: HTMLElement): HTMLElement {
  const wrapper = document.createElement(tag);
  wrapper.append(child);
  return wrapper;
}
