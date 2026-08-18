import { describe, expect, it } from "vitest";

import type { DocumentBlock, SnapshotEnvelope } from "@open-office/schema/artifact";

import {
  createDocumentEngine,
  DocumentEngineAdapter,
  type DocumentEngineBinding,
  type DocumentSessionBinding,
} from "../src/index.js";

const block: DocumentBlock = {
  id: "block-1",
  kind: { type: "paragraph" },
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
  content: { text: "hello", runs: [] },
  children: [],
  data: { type: "none" },
};

const snapshot: SnapshotEnvelope = {
  protocolVersion: 1,
  artifact: {
    format: "open-office-artifact",
    schemaVersion: 5,
    artifactId: "doc-1",
    revision: 4,
    kind: "document",
    payload: {
      kind: "document",
      data: { root: [block.id], blocks: [block], pageSetup: null },
    },
  },
};

function fakeBinding(): DocumentEngineBinding {
  return {
    loadSnapshot: () => {
      const change = {
        revision: 5,
        changedBlocks: [block.id],
        changedContainers: [],
        structureChanged: false,
        mutations: [],
      };
      const session: DocumentSessionBinding = {
        dispatch: () => JSON.stringify(change),
        undo: () => JSON.stringify(change),
        redo: () => JSON.stringify(change),
        canUndo: () => true,
        canRedo: () => false,
        readBlock: () => JSON.stringify(block),
        readBlocks: (idsJson) => JSON.stringify(JSON.parse(idsJson).map(() => block)),
        readChangeSet: () => JSON.stringify(change),
        readSnapshot: () => JSON.stringify(snapshot),
        revision: () => 5,
      };
      return session;
    },
  };
}

describe("DocumentEngineAdapter", () => {
  it("keeps typed commands at the adapter boundary and reads incremental changes", () => {
    const session = new DocumentEngineAdapter(fakeBinding()).loadSnapshot(snapshot);
    const change = session.dispatch({ baseRevision: 4, commands: [] });

    expect(change.revision).toBe(5);
    expect(session.readChangeSet()?.changedBlocks).toEqual(["block-1"]);
    expect(session.readBlock("block-1").content?.text).toBe("hello");
    expect(session.readBlocks(["block-1"])[0].id).toBe("block-1");
    expect(session.readSnapshot().artifact.revision).toBe(4);
    expect(session.revision()).toBe(5);
    expect(session.canUndo()).toBe(true);
    expect(session.canRedo()).toBe(false);
    expect(session.undo().revision).toBe(5);
    expect(session.redo().changedBlocks).toEqual(["block-1"]);
  });

  it("keeps wasm loading injectable so the editor can lazy-load the binary", async () => {
    const engine = await createDocumentEngine(async () => fakeBinding());
    const session = engine.loadSnapshot(snapshot);
    expect(session.revision()).toBe(5);
  });
});
