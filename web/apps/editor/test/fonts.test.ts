// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import {
  FONT_GROUP_CHINESE,
  FONT_GROUP_LATIN,
  FONT_GROUP_MONO,
  FONT_OPTIONS,
} from "../src/typography/fonts.js";

const selectable = FONT_OPTIONS.filter((option) => !option.disabled && option.value !== "");

describe("editor font catalog", () => {
  it("offers a curated set across Chinese, Latin and monospace groups", () => {
    const groupHeaders = FONT_OPTIONS.filter((option) => option.disabled);
    expect(groupHeaders.map((option) => option.value)).toEqual([
      FONT_GROUP_CHINESE,
      FONT_GROUP_LATIN,
      FONT_GROUP_MONO,
    ]);
    // 1 placeholder + 3 group headers + selectable fonts.
    expect(FONT_OPTIONS.length).toBe(1 + 3 + selectable.length);
    expect(selectable.length).toBeGreaterThanOrEqual(15);
  });

  it("keeps every selectable value unique and labelled", () => {
    const values = new Set(selectable.map((option) => option.value));
    expect(values.size).toBe(selectable.length);
    for (const option of selectable) {
      expect(option.label).toBeTruthy();
      expect(option.value.trim().length).toBeGreaterThan(0);
    }
  });

  it("persists as a full CSS font stack that round-trips through element.style", () => {
    // The editor writes InlineStyle.fontFamily straight into
    // element.style.fontFamily and reads it back verbatim for reconciliation;
    // a value that CSS normalizes differently would cause perpetual rebuilds.
    const probe = document.createElement("span");
    for (const option of selectable) {
      probe.style.fontFamily = option.value;
      expect(probe.style.fontFamily, option.label).toBe(option.value);
    }
  });

  it("every stack terminates in a generic family", () => {
    for (const option of selectable) {
      const families = option.value.split(",").map((part) => part.trim());
      const last = families[families.length - 1].toLowerCase();
      expect(["serif", "sans-serif", "monospace"], option.label).toContain(last);
      expect(families.length, option.label).toBeGreaterThanOrEqual(2);
    }
  });
});
