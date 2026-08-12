import { describe, expect, it } from "vitest";
import { CURRENT_SCHEMA_VERSION, parseSnapshot } from "../src/artifact.js";
import { parsePresentationV5Deck } from "../src/presentation-v5.js";
import sharedFixture from "../../../../fixtures/presentation/v5/minimal-deck.json";

const frame = {
  body: { text: "Hello", runs: [] },
  verticalAlign: "middle",
  padding: { top: 0, right: 0, bottom: 0, left: 0 },
  autoFit: "none",
};
const deck = {
  pageSpec: { width: 12192000, height: 6858000, unit: "emu" },
  slides: [{
    id: "slide-1", orderKey: "a", name: "Intro", layoutId: "layout-1",
    nodes: [{ id: "text-1", parentId: null, orderKey: "a", layoutPlaceholderId: "title", transform: { x: 0, y: 0, width: 10, height: 10, rotation: 0 }, visible: true, locked: false, opacity: 1, kind: { type: "text", data: { frame } } }],
    timeline: { entries: [] },
  }],
  masters: [{ id: "master-1", name: "Default", placeholders: [{ id: "master-title", kind: "title", transform: { x: 0, y: 0, width: 10, height: 10, rotation: 0 }, defaultText: null }] }],
  layouts: [{ id: "layout-1", masterId: "master-1", name: "Title", placeholders: [{ id: "title", kind: "title", masterPlaceholderId: "master-title", transform: { x: 0, y: 0, width: 10, height: 10, rotation: 0 }, defaultText: null }] }],
  theme: { id: "theme-1", name: "Default", colors: {}, fonts: {} }, assets: [],
};
const copy = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

describe("presentation v5 target contract", () => {
  it("accepts the shared Rust/browser target fixture", () => {
    const parsed = parsePresentationV5Deck(sharedFixture);
    expect(parsed.slides[0]?.nodes[0]?.id).toBe("title-1");
  });

  it("accepts v5 Deck only through the online Artifact boundary", () => {
    const snapshot = {
      protocolVersion: 1,
      artifact: {
        format: "open-office-artifact", schemaVersion: CURRENT_SCHEMA_VERSION,
        artifactId: "presentation-1", revision: 0, kind: "presentation",
        payload: { kind: "presentation", data: sharedFixture },
      },
    };
    expect(parseSnapshot(snapshot).artifact.payload.kind).toBe("presentation");
    const v4 = structuredClone(snapshot) as { artifact: { schemaVersion: number; payload: { data: Record<string, unknown> } } };
    v4.artifact.schemaVersion = 4;
    v4.artifact.payload.data = { slides: [{ id: "old", elements: [] }], theme: null };
    expect(() => parseSnapshot(v4)).toThrow();
  });

  it("accepts typed text frame and master/layout placeholder references", () => {
    expect(parsePresentationV5Deck(deck)).toMatchObject({ slides: [{ nodes: [{ layoutPlaceholderId: "title" }] }] });
  });

  it("rejects generic v4 node data and invalid typed payloads", () => {
    const v4 = copy(deck); v4.slides[0].nodes[0] = { ...v4.slides[0].nodes[0], typeId: "text", attrs: {} } as never;
    expect(() => parsePresentationV5Deck(v4)).toThrow(/不受支持/);
    const invalidText = copy(deck); invalidText.slides[0].nodes[0].kind.data.frame.body.runs = [{ start: 1, end: 2, style: {} }] as never;
    expect(() => parsePresentationV5Deck(invalidText)).toThrow(/区间无效/);
    const invalidPlaceholder = copy(deck); invalidPlaceholder.slides[0].nodes[0].layoutPlaceholderId = "lost";
    expect(() => parsePresentationV5Deck(invalidPlaceholder)).toThrow(/placeholder/);
  });

  it("validates image crop, table coverage, connector and timeline targets", () => {
    const invalidImage = copy(deck); invalidImage.slides[0].nodes[0].kind = { type: "image", data: { assetId: "lost", originalAssetId: null, crop: { top: 0, right: 0, bottom: 0, left: 0 }, flipH: false, flipV: false, caption: null } } as never;
    expect(() => parsePresentationV5Deck(invalidImage)).toThrow(/asset/);
    const invalidTimeline = copy(deck); invalidTimeline.slides[0].timeline.entries = [{ id: "a", targetNodeId: "lost", trigger: "onClick", preset: "appear", durationMs: 0, delayMs: 0, orderKey: "a" }] as never;
    expect(() => parsePresentationV5Deck(invalidTimeline)).toThrow(/timeline/);
    const invalidTable = copy(deck); invalidTable.slides[0].nodes[0].kind = { type: "table", data: { rows: 1, columns: 2, cells: [{ row: 0, column: 0, rowSpan: 1, columnSpan: 1, content: { text: "", runs: [] }, style: { fill: { type: "none" }, horizontalAlign: "left", verticalAlign: "middle" } }] } } as never;
    expect(() => parsePresentationV5Deck(invalidTable)).toThrow(/未覆盖/);
  });

  it("requires a strict, object-shaped extension payload", () => {
    const extension = copy(deck);
    extension.slides[0].nodes[0].kind = {
      type: "extension",
      data: { namespace: "com.example.widget", version: "1", typeId: "widget", data: { answer: 42 } },
    } as never;
    expect(parsePresentationV5Deck(extension)).toMatchObject({ slides: [{ nodes: [{ kind: { data: { typeId: "widget" } } }] }] });
    const invalid = copy(extension);
    (invalid.slides[0].nodes[0] as unknown as { kind: { type: "extension"; data: { data: unknown } } }).kind.data.data = [];
    expect(() => parsePresentationV5Deck(invalid)).toThrow(/extension\.data/);
  });
});
