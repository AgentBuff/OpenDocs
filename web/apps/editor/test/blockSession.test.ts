import { describe, expect, it } from "vitest";

import type { DocumentBlock, RichText } from "@open-office/schema/artifact";

import { adjustRichTextFontSize, buildPastedImageInsertCommands, setRichTextAttrs, toggleRichTextMark } from "../src/hooks/useBlockSession.js";
import { concatRichText, sliceRichText } from "../src/blocks/richText.js";
import { AutosaveOutbox } from "../src/runtime/autosaveOutbox.js";
import { autosaveDelayMs } from "../src/runtime/autosaveTiming.js";

describe("block text transforms", () => {
  it("slices and joins rich text at Unicode code-point boundaries", () => {
    const content: RichText = {
      text: "A😀BC",
      runs: [{ start: 0, end: 2, style: { ...plainStyle(), bold: true } }, { start: 2, end: 4, style: plainStyle() }],
    };
    expect(sliceRichText(content, 1, 3)).toEqual({
      text: "😀B",
      runs: [
        { start: 0, end: 1, style: { ...plainStyle(), bold: true } },
        { start: 1, end: 2, style: plainStyle() },
      ],
    });
    expect(concatRichText(sliceRichText(content, 0, 1), sliceRichText(content, 3, 4))).toEqual({
      text: "AC",
      runs: [
        { start: 0, end: 1, style: { ...plainStyle(), bold: true } },
        { start: 1, end: 2, style: plainStyle() },
      ],
    });
  });

  it("toggles a selected rich text range without changing text", () => {
    const content: RichText = { text: "hello", runs: [] };
    const bold = toggleRichTextMark(content, 1, 4, "bold");
    expect(bold.text).toBe("hello");
    expect(bold.runs).toEqual([
      { start: 0, end: 1, style: plainStyle() },
      { start: 1, end: 4, style: { ...plainStyle(), bold: true } },
      { start: 4, end: 5, style: plainStyle() },
    ]);
    expect(toggleRichTextMark(bold, 1, 4, "bold")).toEqual(contentWithRuns(content));
  });

  it("can force one mark state across mixed block selections", () => {
    const content: RichText = { text: "hello", runs: [{ start: 1, end: 4, style: { ...plainStyle(), bold: true } }] };
    expect(toggleRichTextMark(content, 0, 5, "bold", true).runs).toEqual([
      { start: 0, end: 5, style: { ...plainStyle(), bold: true } },
    ]);
  });

  it("writes and clears inline font attributes only inside the selected range", () => {
    const content: RichText = { text: "hello", runs: [] };
    const styled = setRichTextAttrs(content, 1, 4, { fontFamily: "Arial", fontSize: 18, color: "#d54941" });
    expect(styled.runs).toEqual([
      { start: 0, end: 1, style: plainStyle() },
      { start: 1, end: 4, style: { ...plainStyle(), fontFamily: "Arial", fontSize: 18, color: "#d54941" } },
      { start: 4, end: 5, style: plainStyle() },
    ]);
    expect(setRichTextAttrs(styled, 1, 4, { color: null })).toEqual({
      text: "hello",
      runs: [
        { start: 0, end: 1, style: plainStyle() },
        { start: 1, end: 4, style: { ...plainStyle(), fontFamily: "Arial", fontSize: 18 } },
        { start: 4, end: 5, style: plainStyle() },
      ],
    });
  });

  it("adjusts the selected font size through the rich-text transform", () => {
    const content: RichText = { text: "hello", runs: [{ start: 0, end: 5, style: { ...plainStyle(), fontSize: 18 } }] };
    expect(adjustRichTextFontSize(content, 1, 4, 1).runs).toEqual([
      { start: 0, end: 1, style: { ...plainStyle(), fontSize: 18 } },
      { start: 1, end: 4, style: { ...plainStyle(), fontSize: 19 } },
      { start: 4, end: 5, style: { ...plainStyle(), fontSize: 18 } },
    ]);
  });

  it("keeps pending commands inside an idempotent transaction envelope", () => {
    const commands = [{
      type: "replaceBlockText" as const,
      blockId: "p-1",
      content: { text: "next", runs: [] },
    }];
    const outbox = new AutosaveOutbox();
    const first = outbox.enqueue({
      artifactId: "doc-1",
      baseRevision: 4,
      sequence: 7,
      commands,
    });
    const second = outbox.enqueue({
      artifactId: "doc-1",
      baseRevision: 4,
      sequence: 8,
      commands,
    });
    expect(first.envelope.baseRevision).toBe(4);
    expect(second.envelope.baseRevision).toBe(5);
    expect(first.envelope.commands[0].typeId).toBe("document.replaceBlockText");
    expect(first.commands).toEqual(commands);
    outbox.retarget(10);
    expect(outbox.entries().map((item) => item.envelope.baseRevision))
      .toEqual([10, 11]);
    expect(outbox.beginAttempt(first.envelope.transactionId)).not.toBeNull();
    expect(outbox.acknowledge(first.envelope.transactionId)?.sequence).toBe(7);
    expect(outbox.peek()?.envelope.transactionId).toBe(second.envelope.transactionId);
  });

  it("coalesces only consecutive unsent text drafts for the same target", () => {
    const outbox = new AutosaveOutbox();
    const first = outbox.enqueue({
      artifactId: "doc-1",
      baseRevision: 4,
      sequence: 1,
      coalesceKey: "block-text:p-1",
      commands: [{ type: "replaceBlockText", blockId: "p-1", content: { text: "你", runs: [] } }],
    });
    const latest = outbox.enqueue({
      artifactId: "doc-1",
      baseRevision: 4,
      sequence: 2,
      coalesceKey: "block-text:p-1",
      commands: [{ type: "replaceBlockText", blockId: "p-1", content: { text: "你好", runs: [] } }],
    });
    expect(outbox.size).toBe(1);
    expect(latest.envelope.transactionId).toBe(first.envelope.transactionId);
    expect(latest.envelope.baseRevision).toBe(4);
    expect(latest.commands[0]).toMatchObject({ type: "replaceBlockText", content: { text: "你好" } });

    outbox.enqueue({
      artifactId: "doc-1",
      baseRevision: 4,
      sequence: 3,
      commands: [{ type: "insertDivider", blockId: "divider-1", parentId: null, index: 1 }],
    });
    expect(outbox.size).toBe(2);
  });

  it("uses trailing idle time with a bounded maximum autosave wait", () => {
    expect(autosaveDelayMs({ now: 1_000, firstPendingAt: 1_000, lastEditAt: 1_000 })).toBe(800);
    expect(autosaveDelayMs({ now: 2_800, firstPendingAt: 0, lastEditAt: 2_800 })).toBe(200);
    expect(autosaveDelayMs({ now: 3_100, firstPendingAt: 0, lastEditAt: 3_100 })).toBe(0);
  });

  it("replaces an empty list item with an asset-backed image without losing list presentation", () => {
    const source = textBlock("source", "", "ordered");
    const commands = buildPastedImageInsertCommands({
      source,
      parentId: null,
      index: 3,
      imageBlockId: "image-1",
      assetId: "asset-1",
      alt: "shot.png",
    });
    expect(commands).toHaveLength(2);
    expect(commands[0]).toMatchObject({
      type: "insertBlock",
      index: 3,
      block: {
        id: "image-1",
        kind: { type: "image" },
        presentation: { list: { kind: "ordered", level: 0 } },
        data: {
          type: "image",
          data: {
            assetId: "asset-1",
            alt: "shot.png",
            originalAssetId: null,
            transform: { crop: { top: 0, right: 0, bottom: 0, left: 0 }, flipHorizontal: false, flipVertical: false },
            caption: "",
          },
        },
      },
    });
    expect(commands[1]).toEqual({ type: "deleteBlock", blockId: "source" });
  });

  it("inserts an image after a non-empty list item", () => {
    const source = textBlock("source", "正文", "bullet");
    const commands = buildPastedImageInsertCommands({
      source,
      parentId: "parent",
      index: 2,
      imageBlockId: "image-2",
      assetId: "asset-2",
      alt: "clip.png",
    });
    expect(commands).toHaveLength(1);
    expect(commands[0]).toMatchObject({
      type: "insertBlock",
      parentId: "parent",
      index: 3,
      block: { presentation: { list: { kind: "bullet", level: 0 } } },
    });
  });
});

function textBlock(id: string, text: string, list: "ordered" | "bullet"): DocumentBlock {
  return {
    id,
    kind: { type: "paragraph" },
    presentation: {
      align: "left",
      list: { kind: list, level: 0 },
      indentStart: 0,
      indentEnd: 0,
      spacingBefore: 0,
      spacingAfter: 0,
      lineHeight: 1,
      namedStyle: null,
    },
    content: { text, runs: [] },
    children: [],
    data: { type: "none" },
  };
}

function contentWithRuns(content: RichText): RichText {
  return { text: content.text, runs: [{ start: 0, end: content.text.length, style: plainStyle() }] };
}

function plainStyle() {
  return { bold: false, italic: false, underline: false, strikethrough: false, fontFamily: null, fontSize: null, color: null, highlight: null, verticalAlign: null };
}
