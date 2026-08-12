import { describe, expect, it, vi } from "vitest";

import type { DocumentBlock, DocumentModel, SnapshotEnvelope } from "@open-office/schema/artifact";

import { BlockProjectionStore } from "../src/store/blockProjectionStore.js";

function block(id: string, text: string): DocumentBlock {
  return {
    id,
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
    content: { text, runs: [] },
    children: [],
    data: { type: "none" },
  };
}

function snapshot(model: DocumentModel, revision = 1): SnapshotEnvelope {
  return {
    protocolVersion: 1,
    artifact: {
      format: "open-office-artifact",
      schemaVersion: 4,
      artifactId: "doc-1",
      revision,
      kind: "document",
      payload: { kind: "document", data: model },
    },
  };
}

describe("BlockProjectionStore", () => {
  it("retains unchanged block references while replacing only changed blocks", () => {
    const first = block("p-1", "one");
    const second = block("p-2", "two");
    const store = new BlockProjectionStore(snapshot({ root: [first.id, second.id], blocks: [first, second], pageSetup: null }));

    const nextFirst = block("p-1", "updated");
    store.replaceSnapshot(snapshot({ root: [first.id, second.id], blocks: [nextFirst, second], pageSetup: null }, 2));

    expect(store.getBlock("p-1")).toBe(nextFirst);
    expect(store.getBlock("p-2")).toBe(second);
  });

  it("notifies the changed block without notifying structure subscribers", async () => {
    const first = block("p-1", "one");
    const second = block("p-2", "two");
    const store = new BlockProjectionStore(snapshot({ root: [first.id, second.id], blocks: [first, second], pageSetup: null }));
    const blockListener = vi.fn();
    const structureListener = vi.fn();
    store.subscribeBlock(first.id, blockListener);
    store.subscribeBlock(second.id, vi.fn());
    store.subscribeStructure(structureListener);

    store.replaceSnapshot(snapshot({
      root: [first.id, second.id],
      blocks: [block("p-1", "updated"), second],
      pageSetup: null,
    }, 2));
    await Promise.resolve();

    expect(blockListener).toHaveBeenCalledTimes(1);
    expect(structureListener).not.toHaveBeenCalled();
  });

  it("notifies structure subscribers when root order changes", async () => {
    const first = block("p-1", "one");
    const second = block("p-2", "two");
    const store = new BlockProjectionStore(snapshot({ root: [first.id, second.id], blocks: [first, second], pageSetup: null }));
    const structureListener = vi.fn();
    store.subscribeStructure(structureListener);

    store.replaceSnapshot(snapshot({ root: [second.id, first.id], blocks: [first, second], pageSetup: null }, 2));
    await Promise.resolve();

    expect(structureListener).toHaveBeenCalledTimes(1);
    expect(store.getStructureSnapshot().root).toEqual([second.id, first.id]);
  });

  it("applies a block-local ChangeSet without traversing or invalidating the root", async () => {
    const first = block("p-1", "one");
    const second = block("p-2", "two");
    const store = new BlockProjectionStore(snapshot({ root: [first.id, second.id], blocks: [first, second], pageSetup: null }));
    const blockListener = vi.fn();
    const otherBlockListener = vi.fn();
    const structureListener = vi.fn();
    store.subscribeBlock(first.id, blockListener);
    store.subscribeBlock(second.id, otherBlockListener);
    store.subscribeStructure(structureListener);

    const updated = block(first.id, "updated");
    store.applyChange({ revision: 2, blocks: [updated] });
    await Promise.resolve();

    expect(store.getBlock(first.id)).toBe(updated);
    expect(store.getBlock(second.id)).toBe(second);
    expect(store.getStructureSnapshot().root).toEqual([first.id, second.id]);
    expect(store.getStructureSnapshot().revision).toBe(2);
    expect(blockListener).toHaveBeenCalledTimes(1);
    expect(otherBlockListener).not.toHaveBeenCalled();
    expect(structureListener).not.toHaveBeenCalled();
  });

  it("keeps untouched references when an authoritative snapshot contains fresh objects", () => {
    const first = block("p-1", "one");
    const second = block("p-2", "two");
    const store = new BlockProjectionStore(snapshot({
      root: [first.id, second.id],
      blocks: [first, second],
      pageSetup: null,
    }));
    const refreshedFirst = block(first.id, "updated");
    const refreshedSecond = block(second.id, "two");

    store.applySnapshot(snapshot({
      root: [first.id, second.id],
      blocks: [refreshedFirst, refreshedSecond],
      pageSetup: null,
    }, 2), [first.id]);

    expect(store.getBlock(first.id)).toBe(refreshedFirst);
    expect(store.getBlock(second.id)).toBe(second);
  });

  it("updates structure while keeping content references outside the invalidation set", () => {
    const first = block("p-1", "one");
    const second = block("p-2", "two");
    const store = new BlockProjectionStore(snapshot({
      root: [first.id, second.id],
      blocks: [first, second],
      pageSetup: null,
    }));

    store.applySnapshot(snapshot({
      root: [second.id, first.id],
      blocks: [block(first.id, "one"), block(second.id, "two")],
      pageSetup: { width: 612, height: 792, marginTop: 72, marginRight: 72, marginBottom: 72, marginLeft: 72 },
    }, 2), []);

    expect(store.getBlock(first.id)).toBe(first);
    expect(store.getBlock(second.id)).toBe(second);
    expect(store.getStructureSnapshot().root).toEqual([second.id, first.id]);
  });

  it("replaces the structure projection and notifies removed block subscribers", async () => {
    const first = block("p-1", "one");
    const second = block("p-2", "two");
    const store = new BlockProjectionStore(snapshot({ root: [first.id, second.id], blocks: [first, second], pageSetup: null }));
    const removedListener = vi.fn();
    const structureListener = vi.fn();
    store.subscribeBlock(second.id, removedListener);
    store.subscribeStructure(structureListener);

    store.replaceSnapshot(snapshot({ root: [first.id], blocks: [first], pageSetup: null }, 2));
    await Promise.resolve();

    expect(store.getBlock(second.id)).toBeNull();
    expect(store.getStructureSnapshot().root).toEqual([first.id]);
    expect(removedListener).toHaveBeenCalledTimes(1);
    expect(structureListener).toHaveBeenCalledTimes(1);
  });

  it("keeps block-local updates bounded with a 5k-block document", () => {
    const blocks = Array.from({ length: 5000 }, (_, index) => block(`p-${index}`, `line ${index}`));
    const store = new BlockProjectionStore(snapshot({
      root: blocks.map((item) => item.id),
      blocks,
      pageSetup: null,
    }));
    const untouched = store.getBlock("p-4999");
    const samples: number[] = [];

    for (let index = 0; index < 100; index += 1) {
      const started = performance.now();
      store.applyChange({ revision: index + 2, blocks: [block("p-0", `line ${index}`)] }, false);
      samples.push(performance.now() - started);
    }

    const sorted = [...samples].sort((left, right) => left - right);
    const p95 = sorted[Math.ceil(sorted.length * 0.95) - 1];
    expect(store.getBlock("p-4999")).toBe(untouched);
    expect(p95).toBeLessThan(20);
  });
});
