import { describe, expect, it } from "vitest";

import {
  ArtifactApiClient,
  parseApiError,
  parseArtifactAsset,
  parseArtifactTransactionHistoryState,
  parseArtifactTransactionResult,
  parseCapabilityCatalog,
  parseEventPage,
  parseDocumentPrintProjection,
  parseProjectionEnvelope,
  parseProjectionItem,
  parseProjectionItems,
  parsePresentationDeckProjection,
  parsePresentationNodeProjection,
  parsePresentationSlideProjection,
  parseMindmapProjection,
  parseDocumentPresencePage,
  parseDocumentReviewPage,
} from "../src/api.js";

const capabilities = {
  protocolVersion: 1,
  contractVersion: 2,
  transport: {
    snapshotEndpoint: "/api/artifacts/{artifactId}/snapshot",
    transactionEndpoint: "/api/artifacts/{artifactId}/transactions",
    revisionHeader: "If-Match",
    idempotencyHeader: "x-transaction-id",
  },
  artifacts: [{
    kind: "document",
    namespace: "document",
    features: {
      edit: "stable",
      history: "stable",
      projection: "stable",
      import: "stable",
      export: "preview",
      assets: "stable",
      presence: "preview",
    },
    commands: [],
  }],
};

describe("framework-free Artifact API boundary", () => {
  it("parses capability, projection and event contracts without UI dependencies", () => {
    expect(parseCapabilityCatalog(capabilities).artifacts[0]?.kind).toBe("document");
    expect(parseCapabilityCatalog(capabilities).artifacts[0]?.features.export).toBe("preview");
    expect(parseProjectionEnvelope({
      protocolVersion: 1,
      contractVersion: 1,
      artifactId: "a-1",
      revision: 4,
      projection: "block",
      data: { items: [] },
      truncated: false,
    }, "block").data).toEqual({ items: [] });
    expect(parseProjectionEnvelope({
      protocolVersion: 1,
      contractVersion: 1,
      artifactId: "a-1",
      revision: 4,
      projection: "tableOfContents",
      data: { items: [] },
      truncated: false,
    }, "tableOfContents").projection).toBe("tableOfContents");
    expect(parseDocumentPrintProjection({
      revision: 4,
      sections: [{ sectionId: "s-1", rootBlockIds: ["b-1"], pageSetup: null, header: null, footer: null, pageNumbering: null }],
      footnotes: [],
      endnotes: [],
    }).sections[0]?.rootBlockIds).toEqual(["b-1"]);
    expect(parseProjectionItems({ items: [{ blockId: "b-1", kind: "paragraph", parentId: null, order: 0, children: [] }] }).items[0]?.blockId).toBe("b-1");
    expect(parseProjectionItem({ blockId: "b-1", kind: "paragraph", parentId: null, order: 0, children: [], refs: [{
      artifactId: "a-1", blockId: "b-1", revision: 4, textRange: { start: 0, end: 3 }, headingPath: [],
    }] }).refs?.[0]?.textRange.end).toBe(3);
    expect(parseEventPage({ artifactId: "a-1", revision: 4, events: [] }).events).toEqual([]);
  });

  it("rejects malformed machine contracts", () => {
    expect(() => parseCapabilityCatalog({ ...capabilities, contractVersion: 0 })).toThrow("正整数");
    expect(() => parseCapabilityCatalog({ ...capabilities, artifacts: [{ ...capabilities.artifacts[0], features: { ...capabilities.artifacts[0].features, import: "maybe" } }] })).toThrow("features.import");
    expect(() => parseProjectionEnvelope({ projection: "unknown" })).toThrow("无效");
    expect(() => parseProjectionItems({ items: [{ blockId: "b-1", kind: "paragraph", parentId: null, order: -1, children: [] }] })).toThrow("非负整数");
    expect(() => parseProjectionItem({ blockId: "b-1", kind: "paragraph", parentId: null, order: 0, children: [], refs: [{
      artifactId: "a-1", blockId: "b-1", revision: 1, textRange: { start: 3, end: 1 }, headingPath: [],
    }] })).toThrow("不小于");
    expect(() => parseApiError({ error: "bad", code: "bad_request" })).toThrow("requestId");
    expect(() => parseApiError({ error: "bad", code: "bad_request", requestId: "err-1" })).toThrow("retryable");
  });

  it("parses Document review and ephemeral selection references strictly", () => {
    expect(parseDocumentPresencePage({
      artifactId: "doc-1",
      ttlMs: 30_000,
      participants: [{
        sessionId: "browser-1", actorId: "user-1", displayName: "User", revision: 4,
        blockId: "b-1", selectedNodeIds: [],
        selection: { anchor: { blockId: "b-1", offset: 1 }, focus: { blockId: "b-1", offset: 3 } },
      }],
    }).participants[0]?.selection?.focus.offset).toBe(3);
    const reviews = parseDocumentReviewPage({
      artifactId: "doc-1",
      revision: 4,
      threads: [{
        threadId: "thread-1", artifactId: "doc-1", kind: "suggestion", state: "open",
        authorId: "user-1", anchorState: "stale", baseRevision: 3,
        anchor: { blockId: "b-1", start: 1, end: 3, revision: 3 },
        suggestion: { originalText: "中🙂", replacement: "text" },
        messages: [{ messageId: "m-1", authorId: "user-1", body: "change", mentions: ["reviewer"], createdAt: "2026-09-10T00:00:00Z" }],
        createdAt: "2026-09-10T00:00:00Z", updatedAt: "2026-09-10T00:00:00Z",
      }],
    });
    expect(reviews.threads[0]?.suggestion?.replacement).toBe("text");
    expect(() => parseDocumentReviewPage({ ...reviews, threads: [{ ...reviews.threads[0], anchorState: "live" }] })).toThrow("anchorState");
  });

  it("parses the derived Mindmap layout without treating it as editable graph state", () => {
    const projection = parseMindmapProjection({
      theme: "light",
      layout: { width: 400, height: 136, nodes: [{ id: "root", depth: 0, x: 0, y: 0, width: 160, height: 40 }] },
      edges: { routes: [{ edgeId: null, parentId: "root", childId: "child", points: [{ x: 1, y: 2 }, { x: 3, y: 4 }] }] },
      advanced: {
        summaries: [{ summaryId: "summary-1", nodeIds: ["a", "b"], points: [{ x: 10, y: 20 }], labelAnchor: { x: 30, y: 40 } }],
        boundaries: [{ boundaryId: "boundary-1", nodeIds: ["a"], rect: { x: 1, y: 2, width: 100, height: 80 }, labelAnchor: { x: 4, y: 5 } }],
        formulas: [{ formulaId: "formula-1", nodeId: "a", anchor: { x: 6, y: 7 } }],
      },
    });
    expect(projection.layout.nodes[0]?.id).toBe("root");
    expect(projection.edges.routes[0]?.edgeId).toBeNull();
    expect(projection.advanced.summaries[0]?.nodeIds).toEqual(["a", "b"]);
    expect(projection.advanced.boundaries[0]?.rect.width).toBe(100);
    expect(projection.advanced.formulas[0]?.anchor).toEqual({ x: 6, y: 7 });
    expect(() => parseMindmapProjection({ ...projection, theme: "sepia" })).toThrow("theme");
  });

  it("uploads binary assets through the typed Artifact boundary", async () => {
    const requests: Array<{ url: string; init?: RequestInit }> = [];
    const client = new ArtifactApiClient({
      baseUrl: "http://api.test",
      fetcher: async (input, init) => {
        requests.push({ url: String(input), init });
        return new Response(JSON.stringify({
          artifactId: "a-1",
          assetId: "asset-1",
          objectKey: "a-1/assets/asset-1",
          contentType: "image/png",
          fileName: "shot.png",
          checksum: "abc123",
          size: 4,
          refCount: 0,
          createdAt: "2026-08-11T00:00:00Z",
          updatedAt: "2026-08-11T00:00:00Z",
        }), { status: 201, headers: { "content-type": "application/json" } });
      },
    });
    const asset = await client.uploadAsset("a-1", new Blob(["shot"], { type: "image/png" }), "shot.png");
    expect(asset.assetId).toBe("asset-1");
    expect(requests[0]?.url).toBe("http://api.test/api/artifacts/a-1/assets");
    expect(requests[0]?.init?.body).toBeInstanceOf(FormData);
    expect(client.assetUrl("a-1", "asset/1")).toBe("http://api.test/api/artifacts/a-1/assets/asset%2F1");
    expect(() => parseArtifactAsset({ ...asset, size: -1 })).toThrow("非负整数");
  });

  it("routes projections through one typed client and preserves structured errors", async () => {
    const calls: Array<{ url: string; init?: RequestInit }> = [];
    const client = new ArtifactApiClient({
      baseUrl: "http://api.test",
      fetcher: async (input, init) => {
        calls.push({ url: String(input), init });
        return new Response(JSON.stringify({
          protocolVersion: 1,
          contractVersion: 1,
          artifactId: "a-1",
          revision: 2,
          projection: "block",
          data: { items: [] },
          truncated: false,
        }), { status: 200, headers: { "content-type": "application/json" } });
      },
    });
    await client.blocks("a-1", { parentId: "root", limit: 20, include: ["headingPath"] });
    expect(calls[0]?.url).toContain("/api/artifacts/a-1/blocks?");
    expect(calls[0]?.url).toContain("parentId=root");
    expect(calls[0]?.init?.cache).toBe("no-store");

    const failing = new ArtifactApiClient({
      fetcher: async () => new Response(JSON.stringify({ error: "冲突", code: "version_conflict", requestId: "err-1", retryable: true }), { status: 409 }),
    });
    await expect(failing.capabilities()).rejects.toMatchObject({
      name: "ArtifactApiError",
      status: 409,
      envelope: { code: "version_conflict", requestId: "err-1", retryable: true },
    });
  });

  it("keeps history availability attached to accepted transaction responses", () => {
    const result = parseArtifactTransactionResult({
      protocolVersion: 1,
      artifactId: "a-1",
      transactionId: "tx-1",
      revision: 2,
      invalidation: { changedEntities: [], changedContainers: [], structureChanged: false },
      mutations: [],
      events: [],
      canUndo: true,
      canRedo: false,
    });
    expect(result).toMatchObject({ revision: 2, canUndo: true, canRedo: false });
    expect(() => parseArtifactTransactionResult({ ...result, canUndo: "yes" })).toThrow("布尔值");
  });

  it("reads canonical artifact history after a Presentation reload", async () => {
    const calls: string[] = [];
    const client = new ArtifactApiClient({
      baseUrl: "http://api.test",
      fetcher: async (input) => {
        calls.push(String(input));
        return new Response(JSON.stringify({ canUndo: true, canRedo: false }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      },
    });
    await expect(client.history("presentation-1")).resolves.toEqual({ canUndo: true, canRedo: false });
    expect(calls).toEqual(["http://api.test/api/artifacts/presentation-1/history"]);
    expect(() => parseArtifactTransactionHistoryState({ canUndo: true, canRedo: "no" })).toThrow("布尔值");
  });

  it("validates slide-scoped Presentation nodes without accepting a second deck model", () => {
    const node = {
      id: "title-1",
      parentId: null,
      orderKey: "a",
      name: null,
      altText: null,
      layoutPlaceholderId: null,
      transform: { x: 0, y: 0, width: 100, height: 40, rotation: 0 },
      visible: true,
      locked: false,
      opacity: 1,
      kind: { type: "group", data: {} },
    };
    const slide = parsePresentationSlideProjection({
      slideId: "slide-1",
      orderKey: "a",
      name: "Intro",
      layoutId: null,
      background: { type: "solid", color: "#ffffff" },
      nodes: [node],
      assetIds: [],
    });
    expect(slide.nodes?.[0]?.id).toBe("title-1");
    expect(parsePresentationNodeProjection({
      slideId: "slide-1",
      node,
      childNodeIds: [],
      assetIds: [],
      slideNodeIds: ["title-1"],
    }).node.id).toBe("title-1");
    expect(() => parsePresentationNodeProjection({
      slideId: "slide-1",
      node,
      childNodeIds: ["missing"],
      assetIds: [],
      slideNodeIds: ["title-1"],
    })).toThrow("childNodeIds");
  });

  it("reads layout picker metadata without exposing a mutable master model", () => {
    expect(parsePresentationDeckProjection({
      pageSpec: { width: 12192000, height: 6858000, unit: "emu" },
      themeId: "theme-1", themeName: "Default", slideCount: 1, masterCount: 1, layoutCount: 1, assetCount: 0,
      masters: [{
        id: "master-1", name: "Default", placeholderCount: 2,
        master: { id: "master-1", name: "Default", background: { type: "none" }, placeholders: [] },
      }],
      layouts: [{
        id: "layout-title", masterId: "master-1", name: "Title", placeholderCount: 1,
        layout: { id: "layout-title", masterId: "master-1", name: "Title", placeholders: [] },
      }],
    })).toMatchObject({ layouts: [{ id: "layout-title", masterId: "master-1" }] });
    expect(() => parsePresentationDeckProjection({
      pageSpec: { width: 1, height: 1, unit: "emu" }, themeId: "theme-1", themeName: "Default", slideCount: 0,
      masterCount: 0, layoutCount: 1, assetCount: 0, masters: [], layouts: [{
        id: "layout-1", masterId: "missing", name: "Title", placeholderCount: 0,
        layout: { id: "layout-1", masterId: "missing", name: "Title", placeholders: [] },
      }],
    })).toThrow("引用不存在 master");
  });
});
