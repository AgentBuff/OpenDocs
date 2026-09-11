// @vitest-environment jsdom
import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it } from "vitest";

import { DocumentFindBar, DocumentToc, revealDocumentMatch } from "../src/chrome/DocumentNavigation.js";
import type { BlockSessionApi } from "../src/hooks/useBlockSession.js";

describe("Document search and navigation", () => {
  beforeEach(() => {
    document.body.replaceChildren();
    if (!globalThis.CSS) Object.defineProperty(globalThis, "CSS", { value: {} });
    if (!CSS.escape) CSS.escape = (value: string) => value.replace(/["\\]/g, "\\$&");
    HTMLElement.prototype.scrollIntoView = () => undefined;
  });

  it("reveals exact CJK and emoji scalar ranges in blocks and stable table cells", () => {
    document.body.innerHTML = `
      <section data-block-id="heading"><div class="block-row__content" contenteditable="true">路线图 😀</div></section>
      <section data-block-id="table"><table><tbody><tr><td contenteditable="true" data-table-row-id="r1" data-table-cell-id="c1">甲😀乙</td></tr></tbody></table></section>
    `;
    const active: string[] = [];
    expect(revealDocumentMatch(
      { target: { type: "block", blockId: "heading" }, start: 4, end: 5 },
      (id) => active.push(id),
    )).toBe(true);
    expect(window.getSelection()?.toString()).toBe("😀");
    expect(revealDocumentMatch(
      { target: { type: "tableCell", blockId: "table", rowId: "r1", cellId: "c1" }, start: 1, end: 2 },
      (id) => active.push(id),
    )).toBe(true);
    expect(window.getSelection()?.toString()).toBe("😀");
    expect(active).toEqual(["heading", "table"]);
  });

  it("renders accessible find/replace controls and heading-only directory entries", () => {
    const session = {
      findText: () => [],
      tableOfContents: () => [{ blockId: "h1", level: 1, text: "开始" }],
      setActiveBlock: () => undefined,
      replaceTextMatch: () => true,
      replaceAllText: () => true,
    } as unknown as BlockSessionApi;
    const find = renderToStaticMarkup(<DocumentFindBar session={session} revision={1} onClose={() => undefined} />);
    const toc = renderToStaticMarkup(<DocumentToc session={session} revision={1} onClose={() => undefined} />);
    expect(find).toContain('aria-label="查找和替换"');
    expect(find).toContain("区分大小写");
    expect(find).toContain("全部替换");
    expect(toc).toContain('aria-label="文档目录"');
    expect(toc).toContain("开始");
  });
});
