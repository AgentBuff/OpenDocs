import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { plainPresentationRichText } from "@open-office/schema";

import { patchPresentationParagraphRange, patchPresentationTextRange, PresentationRichText, presentationRangeStyle, preservePresentationText } from "./PresentationRichText.js";

describe("presentation rich text primitives", () => {
  it("patches Unicode-scalar ranges without splitting CJK or emoji", () => {
    const body = plainPresentationRichText("甲🚀乙");
    const next = patchPresentationTextRange(body, 1, 2, { bold: true });
    expect(next.runs).toHaveLength(3);
    expect(next.runs.map((run) => [run.start, run.end, run.style.bold])).toEqual([
      [0, 1, false], [1, 2, true], [2, 3, false],
    ]);
    expect(presentationRangeStyle(next, 1, 2).bold).toBe(true);
  });

  it("preserves paragraph semantics by paragraph index while text ranges change", () => {
    const body = plainPresentationRichText("一\n二");
    body.paragraphs[1] = { ...body.paragraphs[1]!, alignment: "right", list: { type: "bullet" }, indentLevel: 2 };
    const next = preservePresentationText(body, "甲\n乙丙\n丁");
    expect(next.paragraphs).toEqual([
      { start: 0, end: 2, alignment: "left", list: null, indentLevel: 0 },
      { start: 2, end: 5, alignment: "right", list: { type: "bullet" }, indentLevel: 2 },
      { start: 5, end: 6, alignment: "left", list: null, indentLevel: 0 },
    ]);
  });

  it("applies paragraph formatting only to intersecting paragraphs", () => {
    const body = plainPresentationRichText("one\ntwo\nthree");
    const next = patchPresentationParagraphRange(body, 4, 7, { alignment: "center", list: { type: "ordered", startAt: 3 }, indentLevel: 1 });
    expect(next.paragraphs.map((paragraph) => paragraph.alignment)).toEqual(["left", "center", "left"]);
    expect(next.paragraphs[1]).toMatchObject({ list: { type: "ordered", startAt: 3 }, indentLevel: 1 });
  });

  it("renders consecutive ordered paragraphs with advancing markers", () => {
    const body = plainPresentationRichText("one\ntwo");
    body.paragraphs = body.paragraphs.map((paragraph) => ({ ...paragraph, list: { type: "ordered", startAt: 3 } }));
    const html = renderToStaticMarkup(<PresentationRichText body={body} />);
    expect(html).toContain(">3.</span>");
    expect(html).toContain(">4.</span>");
  });
});
