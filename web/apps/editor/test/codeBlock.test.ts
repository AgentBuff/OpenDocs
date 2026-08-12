import { describe, expect, it } from "vitest";

import { defaultCodeBlockConfig } from "@open-office/schema/artifact";
import { clampCodeBlockHeight, codeConfig, codeLineNumbers, countCodeLines, indentText, isCurrentHighlightRequest, tokenizeCode } from "../src/blocks/code/codeConfig.js";

describe("code block view model", () => {
  it("fills defaults without changing persisted source semantics", () => {
    expect(codeConfig(defaultCodeBlockConfig())).toMatchObject({
      language: "plainText",
      theme: "light",
      showLineNumbers: true,
      indentWidth: 2,
    });
  });

  it("tokenizes common code without HTML injection", () => {
    const tokens = tokenizeCode("const value = \"<safe>\"; // note", "javascript");
    expect(tokens.some((token) => token.className === "code-token--keyword")).toBe(true);
    expect(tokens.some((token) => token.className === "code-token--string" && token.text.includes("<safe>"))).toBe(true);
    expect(tokens.some((token) => token.className === "code-token--comment")).toBe(true);
  });

  it("honors tabs and configured space indentation", () => {
    expect(indentText({ ...codeConfig(defaultCodeBlockConfig()), indentMode: "tabs" })).toBe("\t");
    expect(indentText({ ...codeConfig(defaultCodeBlockConfig()), indentMode: "spaces", indentWidth: 4 })).toBe("    ");
  });

  it("keeps the editable viewport within the persisted resize bounds", () => {
    expect(clampCodeBlockHeight(80)).toBe(160);
    expect(clampCodeBlockHeight(327.7)).toBe(328);
    expect(clampCodeBlockHeight(900)).toBe(640);
  });

  it("builds line-number text without allocating one DOM node per line", () => {
    const source = "one\ntwo\nthree";
    expect(countCodeLines(source)).toBe(3);
    expect(codeLineNumbers(source)).toBe("1\n2\n3");
    expect(countCodeLines("")).toBe(1);
  });

  it("drops highlight responses that are stale or arrive after unmount", () => {
    expect(isCurrentHighlightRequest(4, 4, false)).toBe(true);
    expect(isCurrentHighlightRequest(3, 4, false)).toBe(false);
    expect(isCurrentHighlightRequest(4, 4, true)).toBe(false);
  });
});
