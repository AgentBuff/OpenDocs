import { describe, expect, it } from "vitest";

import { CURRENT_SCHEMA_VERSION, parseCommitResult, parseSnapshot, type DocumentCommand, type SnapshotEnvelope } from "../src/artifact.js";

function paragraphSnapshot(): SnapshotEnvelope {
  return {
    protocolVersion: 1,
    artifact: {
      format: "open-office-artifact",
      schemaVersion: CURRENT_SCHEMA_VERSION,
      artifactId: "doc-1",
      revision: 3,
      kind: "document",
      payload: {
        kind: "document",
        data: {
          root: ["p-1"],
          blocks: [
            {
              id: "p-1",
              kind: { type: "paragraph" },
              presentation: { align: "left", list: null, indentStart: 0, indentEnd: 0, spacingBefore: 0, spacingAfter: 0, lineHeight: 1, namedStyle: null },
              content: { text: "hello", runs: [] },
              children: [],
              data: { type: "none" },
            },
          ],
          pageSetup: null,
        },
      },
    },
  };
}

describe("Artifact snapshot boundary", () => {
  it("accepts a valid document tree", () => {
    expect(parseSnapshot(paragraphSnapshot())).toEqual(paragraphSnapshot());
  });

  it("rejects a kind mismatch before the editor sees it", () => {
    const snapshot = structuredClone(paragraphSnapshot());
    Object.defineProperty(snapshot.artifact, "kind", { value: "spreadsheet", writable: true });
    expect(() => parseSnapshot(snapshot)).toThrow("kind 不匹配");
  });

  it("rejects orphan blocks and cycles", () => {
    const orphan = structuredClone(paragraphSnapshot());
    if (orphan.artifact.payload.kind !== "document") throw new Error("expected document payload");
    orphan.artifact.payload.data.blocks.push({
      id: "orphan",
      kind: { type: "paragraph" },
      presentation: { align: "left", list: null, indentStart: 0, indentEnd: 0, spacingBefore: 0, spacingAfter: 0, lineHeight: 1, namedStyle: null },
      content: { text: "", runs: [] },
      children: [],
      data: { type: "none" },
    });
    expect(() => parseSnapshot(orphan)).toThrow("root 可达树");

    const cycle = structuredClone(paragraphSnapshot());
    if (cycle.artifact.payload.kind !== "document") throw new Error("expected document payload");
    cycle.artifact.payload.data.blocks[0].children = ["p-1"];
    expect(() => parseSnapshot(cycle)).toThrow("存在环");
  });

  it("preserves an unknown future block kind", () => {
    const snapshot = structuredClone(paragraphSnapshot());
    if (snapshot.artifact.payload.kind !== "document") throw new Error("expected document payload");
    const blocks = snapshot.artifact.payload.data.blocks;
    const futureKind = {
      type: "future.databaseView",
      provider: "example",
      config: { columns: ["name", "status"] },
    };
    blocks[0].kind = futureKind;
    blocks[0].data = { type: "extension", data: { typeId: "future.databaseView", raw: futureKind } };
    const parsed = parseSnapshot(snapshot);
    if (parsed.artifact.payload.kind !== "document") throw new Error("expected document payload");
    expect(parsed.artifact.payload.data.blocks[0].kind).toEqual(futureKind);
  });

  it("rejects invalid rich text ranges", () => {
    const snapshot = paragraphSnapshot();
    if (snapshot.artifact.payload.kind !== "document") throw new Error("expected document payload");
    snapshot.artifact.payload.data.blocks[0].content = {
      text: "你好",
      runs: [{ start: 1, end: 3, style: plainInlineStyle() }],
    };
    expect(() => parseSnapshot(snapshot)).toThrow("区间无效");
  });

  it("rejects retired inline attrs and unknown style fields", () => {
    const snapshot = paragraphSnapshot();
    if (snapshot.artifact.payload.kind !== "document") throw new Error("expected document payload");
    snapshot.artifact.payload.data.blocks[0].content = {
      text: "x",
      runs: [{ start: 0, end: 1, attrs: {} } as never],
    };
    expect(() => parseSnapshot(snapshot)).toThrow("旧 attrs");
    snapshot.artifact.payload.data.blocks[0].content = {
      text: "x",
      runs: [{ start: 0, end: 1, style: { ...plainInlineStyle(), unknown: true } } as never],
    };
    expect(() => parseSnapshot(snapshot)).toThrow("不是受支持");
  });

  it("rejects invalid Word format attributes at the network boundary", () => {
    const snapshot = paragraphSnapshot();
    if (snapshot.artifact.payload.kind !== "document") throw new Error("expected document payload");
    snapshot.artifact.payload.data.blocks[0].presentation = { ...snapshot.artifact.payload.data.blocks[0].presentation, align: "diagonal" as never };
    expect(() => parseSnapshot(snapshot)).toThrow("align 无效");
    snapshot.artifact.payload.data.blocks[0].content = {
      text: "hello",
      runs: [{ start: 0, end: 5, style: { ...plainInlineStyle(), fontSize: 1024 } }],
    };
    expect(() => parseSnapshot(snapshot)).toThrow("fontSize");
  });

  it("requires a typed target for link blocks", () => {
    const snapshot = paragraphSnapshot();
    if (snapshot.artifact.payload.kind !== "document") throw new Error("expected document payload");
    snapshot.artifact.payload.data.blocks[0].kind = { type: "link" };
    expect(() => parseSnapshot(snapshot)).toThrow("link 必须包含 link data");
    snapshot.artifact.payload.data.blocks[0].data = { type: "link", data: { url: "https://openoffice.example" } };
    expect(parseSnapshot(snapshot).artifact.payload.kind).toBe("document");
  });

  it("accepts structured table blocks and rejects malformed rows", () => {
    const snapshot = paragraphSnapshot();
    if (snapshot.artifact.payload.kind !== "document") throw new Error("expected document payload");
    const table = snapshot.artifact.payload.data.blocks[0];
    table.kind = { type: "table" };
    table.content = null;
    table.data = {
      type: "table",
      data: {
        columns: [{ id: "c-1", width: null }, { id: "c-2", width: 120 }],
        rows: [{
          id: "r-1",
          height: null,
          cells: [
            {
              id: "cell-1",
              content: { text: "A", runs: [] },
              style: {
                borders: {
                  top: { style: "solid", color: "#1677ff", width: 1 },
                },
              },
            },
            { id: "cell-2", content: { text: "B", runs: [] } },
          ],
        }],
        mergedRanges: [],
      },
    };
    expect(parseSnapshot(snapshot).artifact.payload.kind).toBe("document");
    if (table.data.type !== "table") throw new Error("expected table data");
    table.data.data.rows[0].cells[0].style!.borders!.top!.color = "red";
    expect(() => parseSnapshot(snapshot)).toThrow("#RRGGBB");
    table.data.data.rows[0].cells[0].style!.borders!.top!.color = "#1677ff";
    table.data.data.rows[0].cells.pop();
    expect(() => parseSnapshot(snapshot)).toThrow("单元格数量");

    table.data.data.rows[0].cells.push({ id: "cell-2", content: { text: "B", runs: [] } });
    table.data.data.rows[0].height = 0;
    expect(() => parseSnapshot(snapshot)).toThrow("height");
    table.data.data.rows[0].height = null;
    table.data.data.rows[0].cells[0].style!.borders!.top!.width = 33;
    expect(() => parseSnapshot(snapshot)).toThrow("width");
  });

  it("keeps v3 merged ranges as stable row and column references", () => {
    const snapshot = paragraphSnapshot();
    if (snapshot.artifact.payload.kind !== "document") throw new Error("expected document payload");
    const table = snapshot.artifact.payload.data.blocks[0];
    table.kind = { type: "table" };
    table.content = null;
    table.data = {
      type: "table",
      data: {
        columns: [{ id: "c-1", width: null }, { id: "c-2", width: null }],
        rows: [
          { id: "r-1", height: null, cells: [{ id: "cell-1", content: { text: "A", runs: [] } }, { id: "cell-2", content: { text: "B", runs: [] } }] },
          { id: "r-2", height: null, cells: [{ id: "cell-3", content: { text: "C", runs: [] } }, { id: "cell-4", content: { text: "D", runs: [] } }] },
        ],
        mergedRanges: [{ startRowId: "r-1", endRowId: "r-2", startColumnId: "c-1", endColumnId: "c-2" }],
      },
    };

    const parsed = parseSnapshot(snapshot);
    if (parsed.artifact.payload.kind !== "document") throw new Error("expected document payload");
    const data = parsed.artifact.payload.data.blocks[0].data;
    expect(data).toEqual({
      type: "table",
      data: {
        columns: [{ id: "c-1", width: null }, { id: "c-2", width: null }],
        rows: [
          { id: "r-1", height: null, cells: [{ id: "cell-1", content: { text: "A", runs: [] }, style: {} }, { id: "cell-2", content: { text: "B", runs: [] }, style: {} }] },
          { id: "r-2", height: null, cells: [{ id: "cell-3", content: { text: "C", runs: [] }, style: {} }, { id: "cell-4", content: { text: "D", runs: [] }, style: {} }] },
        ],
        mergedRanges: [{ startRowId: "r-1", endRowId: "r-2", startColumnId: "c-1", endColumnId: "c-2" }],
      },
    });
  });

  it("requires the strict current schema version at the browser boundary", () => {
    for (const version of [3, 4]) {
      const snapshot = structuredClone(paragraphSnapshot());
      snapshot.artifact.schemaVersion = version;
      expect(() => parseSnapshot(snapshot)).toThrow(`不支持的 schema 版本：${version}`);
    }
  });

  it("round-trips the current block presentation/data boundary", () => {
    const snapshot = paragraphSnapshot();
    if (snapshot.artifact.payload.kind !== "document") throw new Error("expected document payload");
    const block = snapshot.artifact.payload.data.blocks[0];
    block.presentation = {
      align: "center",
      list: { kind: "ordered", level: 2 },
      indentStart: 3,
      indentEnd: 1,
      spacingBefore: 4,
      spacingAfter: 6,
      lineHeight: 1.5,
      namedStyle: { name: "Heading 2" },
    };
    block.data = { type: "todo", data: { checked: true } };
    block.kind = { type: "todo" };
    const parsed = parseSnapshot(structuredClone(snapshot));
    if (parsed.artifact.payload.kind !== "document") throw new Error("expected document payload");
    expect(parsed.artifact.schemaVersion).toBe(CURRENT_SCHEMA_VERSION);
    expect(parsed.artifact.payload.data.blocks[0]).toMatchObject({
      presentation: block.presentation,
      data: block.data,
    });
  });

  it("rejects retired DocumentBlock attrs and payload fields online", () => {
    for (const retiredField of ["attrs", "payload"] as const) {
      const snapshot = structuredClone(paragraphSnapshot());
      if (snapshot.artifact.payload.kind !== "document") throw new Error("expected document payload");
      Object.assign(snapshot.artifact.payload.data.blocks[0] as unknown as Record<string, unknown>, {
        [retiredField]: retiredField === "attrs" ? {} : null,
      });
      expect(() => parseSnapshot(snapshot)).toThrow("旧 attrs/payload");
    }
  });

  it("requires mergedRanges to be present and each range endpoint to be non-empty", () => {
    const missing = structuredClone(paragraphSnapshot());
    if (missing.artifact.payload.kind !== "document") throw new Error("expected document payload");
    const missingTable = missing.artifact.payload.data.blocks[0];
    missingTable.kind = { type: "table" };
    missingTable.content = null;
    missingTable.data = {
      type: "table",
      data: {
        columns: [{ id: "c-1", width: null }],
        rows: [{ id: "r-1", height: null, cells: [{ id: "cell-1", content: { text: "", runs: [] } }] }],
      } as never,
    };
    expect(() => parseSnapshot(missing)).toThrow("mergedRanges");

    const invalid = structuredClone(paragraphSnapshot());
    if (invalid.artifact.payload.kind !== "document") throw new Error("expected document payload");
    const invalidTable = invalid.artifact.payload.data.blocks[0];
    invalidTable.kind = { type: "table" };
    invalidTable.content = null;
    invalidTable.data = {
      type: "table",
      data: {
        columns: [{ id: "c-1", width: null }],
        rows: [{ id: "r-1", height: null, cells: [{ id: "cell-1", content: { text: "", runs: [] } }] }],
        mergedRanges: [{ startRowId: "", endRowId: "r-1", startColumnId: "c-1", endColumnId: "c-1" }],
      },
    };
    expect(() => parseSnapshot(invalid)).toThrow("table merge startRowId");
  });

  it("keeps table formatting commands typed at the protocol boundary", () => {
    const commands: DocumentCommand[] = [
      {
        type: "formatTableCells",
        blockId: "table-1",
        selection: { kind: "range", startRowId: "r-1", endRowId: "r-2", startColumnId: "c-1", endColumnId: "c-2" },
        patch: { fillColor: "#fff2cc", horizontalAlign: "center", verticalAlign: "middle" },
      },
      {
        type: "setTableBorders",
        blockId: "table-1",
        selection: { kind: "all" },
        patch: { top: { style: "solid", color: "#1677ff", width: 1 }, bottom: null },
      },
      {
        type: "applyTableBorderPreset",
        blockId: "table-1",
        selection: { kind: "all" },
        preset: "innerVertical",
        border: { style: "solid", color: "#1677ff", width: 1 },
      },
      { type: "setTableRowHeight", blockId: "table-1", rowId: "r-1", height: 48 },
      {
        type: "patchInlineRange",
        blockId: "paragraph-1",
        range: { start: 0, end: 4 },
        patch: { bold: true, fontSize: 18, color: "#3366ff", highlight: null },
      },
      {
        type: "patchTableCellInlineRange",
        blockId: "table-1",
        rowId: "r-1",
        cellId: "cell-1",
        range: { start: 0, end: 4 },
        patch: { bold: true, color: "#3366ff" },
      },
    ];
    expect(commands.map((command) => command.type)).toEqual([
      "formatTableCells",
      "setTableBorders",
      "applyTableBorderPreset",
      "setTableRowHeight",
      "patchInlineRange",
      "patchTableCellInlineRange",
    ]);
  });

  it("requires code blocks to carry an explicit config and fills config defaults", () => {
    const missingPayload = structuredClone(paragraphSnapshot());
    if (missingPayload.artifact.payload.kind !== "document") throw new Error("expected document payload");
    missingPayload.artifact.payload.data.blocks[0].kind = { type: "code" };
    expect(() => parseSnapshot(missingPayload)).toThrow("必须包含 code data");

    const partial = {
      protocolVersion: 1,
      artifact: {
        format: "open-office-artifact",
        schemaVersion: CURRENT_SCHEMA_VERSION,
        artifactId: "doc-code",
        revision: 0,
        kind: "document",
        payload: {
          kind: "document",
          data: {
            root: ["code-1"],
            blocks: [{
              id: "code-1",
              kind: { type: "code" },
              presentation: defaultBlockPresentation(),
              content: { text: "fn main() {}", runs: [] },
              children: [],
              data: { type: "code", data: { language: "rust" } },
            }],
            pageSetup: null,
          },
        },
      },
    };
    const parsed = parseSnapshot(partial);
    if (parsed.artifact.payload.kind !== "document") throw new Error("expected document payload");
    const data = parsed.artifact.payload.data.blocks[0].data;
    expect(data).toEqual({
      type: "code",
      data: {
        title: "",
        language: "rust",
        theme: "light",
        height: 200,
        showLineNumbers: true,
        wrap: false,
        indentMode: "spaces",
        indentWidth: 2,
        fontSize: 14,
      },
    });
  });

  it("strictly validates code payload combinations and settings", () => {
    const wrongPayload = structuredClone(paragraphSnapshot());
    if (wrongPayload.artifact.payload.kind !== "document") throw new Error("expected document payload");
    wrongPayload.artifact.payload.data.blocks[0].kind = { type: "code" };
    wrongPayload.artifact.payload.data.blocks[0].data = {
      type: "image",
      data: {
        assetId: "asset-1",
        alt: "",
        originalAssetId: null,
        transform: { crop: { top: 0, right: 0, bottom: 0, left: 0 }, flipHorizontal: false, flipVertical: false },
        size: { width: null, height: null, lockAspectRatio: true },
        placement: { offsetX: 0, offsetY: 0 },
        caption: "",
      },
    };
    expect(() => parseSnapshot(wrongPayload)).toThrow("code 必须包含 code data");

    const invalidConfig = (data: Record<string, unknown>) => ({
      protocolVersion: 1,
      artifact: {
        format: "open-office-artifact",
        schemaVersion: CURRENT_SCHEMA_VERSION,
        artifactId: "doc-code-invalid",
        revision: 0,
        kind: "document",
        payload: {
          kind: "document",
          data: {
            root: ["code-1"],
            blocks: [{
              id: "code-1",
              kind: { type: "code" },
              presentation: defaultBlockPresentation(),
              content: { text: "", runs: [] },
              children: [],
              data: { type: "code", data },
            }],
            pageSetup: null,
          },
        },
      },
    });
    expect(() => parseSnapshot(invalidConfig({ language: "java script" }))).toThrow("language");
    expect(() => parseSnapshot(invalidConfig({ indentMode: "spaces", indentWidth: 3 }))).toThrow("indentWidth");
    expect(() => parseSnapshot(invalidConfig({ showLineNumbers: "yes" }))).toThrow("showLineNumbers");
    expect(() => parseSnapshot(invalidConfig({ fontSize: 64 }))).toThrow("fontSize");
    expect(() => parseSnapshot(invalidConfig({ height: 120 }))).toThrow("height");
  });

  it("validates non-document artifact structure before the editor sees it", () => {
    const snapshot: SnapshotEnvelope = {
      protocolVersion: 1,
      artifact: {
        format: "open-office-artifact",
        schemaVersion: CURRENT_SCHEMA_VERSION,
        artifactId: "sheet-1",
        revision: 0,
        kind: "spreadsheet",
        payload: {
          kind: "spreadsheet",
          data: {
            metadata: { activeSheetId: null, calculationMode: "automatic", dateSystem: "excel1900" },
            sheets: [{
              id: "sheet-1",
              name: "Sheet 1",
              cells: [
                { row: 1, column: 1, attrs: {} },
                { row: 1, column: 1, attrs: {} },
              ],
              metadata: { visibility: "visible", rowCount: null, columnCount: null, freeze: { rows: 0, columns: 0 }, autoFilter: null, sort: null, conditionalFormats: [], dataValidations: [], mergedRanges: [], media: [] },
            }],
          },
        },
      },
    };
    expect(() => parseSnapshot(snapshot)).toThrow("重复 cell 坐标");
  });

  it("validates the typed commit result boundary", () => {
    const result = parseCommitResult({
      protocolVersion: 1,
      artifactId: "doc-1",
      transactionId: "tx-1",
      revision: 4,
      invalidation: {
        changedEntities: [{ entityType: "document.block", entityId: "p-1" }],
        changedContainers: [],
        structureChanged: false,
      },
      mutations: [{ typeId: "document.mutation", payload: { type: "update" } }],
      events: [],
    });
    expect(result.invalidation.changedEntities[0].entityId).toBe("p-1");
    expect(() => parseCommitResult({ ...result, invalidation: null })).toThrow("invalidation");
  });

  it("rejects unsupported snapshot and commit protocol versions at the boundary", () => {
    const zero = structuredClone(paragraphSnapshot());
    zero.protocolVersion = 0;
    expect(() => parseSnapshot(zero)).toThrow("protocolVersion");

    const future = structuredClone(paragraphSnapshot());
    future.protocolVersion = 2;
    expect(() => parseSnapshot(future)).toThrow("protocol");

    expect(() => parseCommitResult({
      protocolVersion: 2,
      artifactId: "doc-1",
      transactionId: "tx-1",
      revision: 1,
      invalidation: { changedEntities: [], changedContainers: [], structureChanged: false },
      mutations: [],
      events: [],
    })).toThrow("protocol");
  });

  it("keeps opaque mutation and event payloads while validating stable identifiers", () => {
    const parsed = parseCommitResult({
      protocolVersion: 1,
      artifactId: "doc-1",
      transactionId: "tx-future",
      revision: 8,
      invalidation: {
        changedEntities: [{ entityType: "document.future", entityId: "opaque-1" }],
        changedContainers: [],
        structureChanged: false,
      },
      mutations: [{ typeId: "document.futureMutation", payload: { unknown: [1, true] } }],
      events: [{ eventId: "event-1", typeId: "document.futureEvent", payload: { opaque: true } }],
    });
    expect(parsed.mutations[0].payload).toEqual({ unknown: [1, true] });
    expect(parsed.events[0].typeId).toBe("document.futureEvent");

    expect(() => parseCommitResult({
      protocolVersion: 1,
      artifactId: "",
      transactionId: "tx-1",
      revision: 1,
      invalidation: { changedEntities: [], changedContainers: [], structureChanged: false },
      mutations: [],
      events: [],
    })).toThrow("artifactId");
  });
});

function plainInlineStyle() {
  return { bold: false, italic: false, underline: false, strikethrough: false, fontFamily: null, fontSize: null, color: null, highlight: null, verticalAlign: null };
}

function defaultBlockPresentation() {
  return {
    align: "left" as const,
    list: null,
    indentStart: 0,
    indentEnd: 0,
    spacingBefore: 0,
    spacingAfter: 0,
    lineHeight: 1,
    namedStyle: null,
  };
}
