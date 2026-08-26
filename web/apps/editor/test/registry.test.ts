import { describe, expect, it } from "vitest";

import type { DocumentBlock } from "@open-office/schema/artifact";

import {
  createBlockRegistry,
  type BlockDefinition,
  type BlockRenderer,
} from "../src/blocks/registry.js";

const renderer = (() => null) as BlockRenderer;

function block(kind: DocumentBlock["kind"]): DocumentBlock {
  return {
    id: "block-1",
    kind,
    presentation: {
      align: "left",
      list: null,
      indentStart: 0,
      indentEnd: 0,
      spacingBefore: 0,
      spacingAfter: 0,
      lineHeight: 1,
      namedStyle: null,
    },
    content: { text: "", runs: [] },
    children: [],
    data: { type: "none" },
  };
}

function definitions(): BlockDefinition[] {
  return [
    {
      key: "code",
      matches: (value) => value.kind.type === "code",
      renderer,
      behavior: { selection: "object" },
      commands: [{ id: "copy", title: "复制", execute: () => undefined }],
    },
    { key: "content", fallback: true, matches: () => true, renderer },
  ];
}

describe("block runtime registry", () => {
  it("resolves specialized definitions before the fallback", () => {
    const registry = createBlockRegistry(definitions());
    expect(registry.resolve(block({ type: "code" })).key).toBe("code");
    expect(registry.resolve(block({ type: "paragraph" })).key).toBe("content");
    expect(registry.resolve(block({ type: "code" })).commands?.[0].id).toBe("copy");
  });

  it("rejects duplicate keys and missing fallback definitions", () => {
    expect(() => createBlockRegistry([
      { key: "content", matches: () => true, renderer },
      { key: "content", matches: () => true, renderer },
    ])).toThrow("duplicate block definition");
    expect(() => createBlockRegistry([
      { key: "code", matches: () => true, renderer, behavior: { selection: "object" } },
    ])).toThrow("fallback");
  });

  it("creates isolated runtime instances instead of sharing module state", () => {
    const first = createBlockRegistry(definitions());
    const second = createBlockRegistry([
      { key: "image", matches: () => true, renderer, behavior: { selection: "object" } },
      { key: "content", fallback: true, matches: () => false, renderer },
    ]);
    expect(first).not.toBe(second);
    expect(first.resolve(block({ type: "code" })).key).toBe("code");
    expect(second.resolve(block({ type: "paragraph" })).key).toBe("image");
  });

  it("rejects atomic blocks without an explicit interaction behavior", () => {
    expect(() => createBlockRegistry([
      { key: "image", matches: () => true, renderer },
      { key: "content", fallback: true, matches: () => false, renderer },
    ])).toThrow("交互行为");
  });
});
