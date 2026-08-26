import { describe, expect, it } from "vitest";

import { InteractionStore } from "../../src/interaction/interactionStore.js";
import { assertEditorSelection, selectionReducer } from "../../src/interaction/selectionReducer.js";
import { EMPTY_EDITOR_SELECTION, type EditorSelection } from "../../src/interaction/types.js";

describe("interaction selection reducer", () => {
  it("keeps exactly one primary selection", () => {
    const text: EditorSelection = { kind: "text", blockId: "block-a", range: { start: 0, end: 4 }, affinity: "forward" };
    const object: EditorSelection = { kind: "object", blockId: "image-a", objectType: "image" };
    expect(selectionReducer(EMPTY_EDITOR_SELECTION, { type: "select", selection: text })).toBe(text);
    expect(selectionReducer(text, { type: "select", selection: object })).toBe(object);
  });

  it("keeps selection when Escape was consumed by an overlay and otherwise clears it", () => {
    const selected: EditorSelection = { kind: "object", blockId: "image-a", objectType: "image" };
    expect(selectionReducer(selected, { type: "escape", overlayHandled: true })).toBe(selected);
    expect(selectionReducer(selected, { type: "escape", overlayHandled: false })).toEqual(EMPTY_EDITOR_SELECTION);
  });

  it("rejects malformed ranges and incoherent table modes", () => {
    expect(() => assertEditorSelection({ kind: "text", blockId: "", range: { start: 2, end: 1 }, affinity: "forward" })).toThrow("无效的文本选区");
    expect(() => assertEditorSelection({
      kind: "table",
      blockId: "table-a",
      selection: { kind: "row", id: "row-a" },
      mode: "cell",
    })).toThrow("无效的表格选区");
  });

  it("notifies only for a real selection transition", () => {
    const store = new InteractionStore();
    let notifications = 0;
    const unsubscribe = store.subscribe(() => { notifications += 1; });
    store.clear();
    store.select({ kind: "none" });
    store.select({ kind: "object", blockId: "image-a", objectType: "image" });
    expect(notifications).toBe(1);
    unsubscribe();
  });
});
