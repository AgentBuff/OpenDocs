import { EMPTY_EDITOR_SELECTION, type EditorSelection, type SelectionEvent } from "./types.js";

/**
 * Pure transition boundary for editor selection. Invalid external payloads are
 * rejected here rather than letting a renderer infer a second selection model.
 */
export function selectionReducer(current: EditorSelection, event: SelectionEvent): EditorSelection {
  switch (event.type) {
    case "clear":
      return EMPTY_EDITOR_SELECTION;
    case "escape":
      return event.overlayHandled ? current : EMPTY_EDITOR_SELECTION;
    case "select":
      return assertEditorSelection(event.selection);
  }
}

export function assertEditorSelection(selection: EditorSelection): EditorSelection {
  switch (selection.kind) {
    case "none":
      return EMPTY_EDITOR_SELECTION;
    case "text":
      if (!isId(selection.blockId) || !isRange(selection.range)) throw new Error("无效的文本选区");
      return selection;
    case "blocks":
      if (!isId(selection.anchorId) || !isId(selection.focusId) || selection.blockIds.length === 0 || selection.blockIds.some((id) => !isId(id))) {
        throw new Error("无效的块选区");
      }
      if (!selection.blockIds.includes(selection.anchorId) || !selection.blockIds.includes(selection.focusId)) {
        throw new Error("块选区必须包含锚点和焦点");
      }
      return selection;
    case "object":
      if (!isId(selection.blockId) || (selection.objectType !== "image" && selection.objectType !== "code")) {
        throw new Error("无效的对象选区");
      }
      return selection;
    case "table":
      if (!isId(selection.blockId) || selection.mode !== selection.selection.kind) throw new Error("无效的表格选区");
      return selection;
  }
}

function isId(value: string): boolean {
  return typeof value === "string" && value.trim().length > 0;
}

function isRange(range: { start: number; end: number }): boolean {
  return Number.isInteger(range.start) && Number.isInteger(range.end) && range.start >= 0 && range.end >= range.start;
}
