import { describe, expect, it } from "vitest";

import type {
  DocumentEngineAdapter,
  DocumentEngineSession,
  DocumentChangeSet,
  DocumentCommandBatch,
} from "@open-office/document-engine";
import { CURRENT_SCHEMA_VERSION, type SnapshotEnvelope } from "@open-office/schema/artifact";

import { AutosaveOutbox } from "../src/runtime/autosaveOutbox.js";
import { CommitApplier } from "../src/runtime/commitApplier.js";
import { ConflictRebaser } from "../src/runtime/conflictRebaser.js";
import { HistoryAdapter } from "../src/runtime/historyAdapter.js";
import { DocumentLoader } from "../src/runtime/loader.js";

describe("document session runtime", () => {
  it("loads the authoritative snapshot and history together", async () => {
    const snapshot = createSnapshot(3);
    const loader = new DocumentLoader({
      getArtifact: async () => snapshot,
      getHistoryState: async () => ({ canUndo: true, canRedo: false }),
    });

    await expect(loader.load("doc-1")).resolves.toEqual({
      snapshot,
      history: { canUndo: true, canRedo: false },
    });
  });

  it("returns typed history CommitResult without writing a client snapshot", async () => {
    const adapter = new HistoryAdapter({
      getState: async () => ({ canUndo: true, canRedo: false }),
      submit: async (documentId, action, baseRevision) => ({
        protocolVersion: 1,
        artifactId: documentId,
        transactionId: `history-${action}`,
        revision: baseRevision + 1,
        invalidation: { changedEntities: [], changedContainers: [], structureChanged: true },
        mutations: [],
        events: [{ eventId: "event-1", typeId: "document.historyApplied", payload: { action } }],
        canUndo: action === "redo",
        canRedo: action === "undo",
      }),
    });

    await expect(adapter.readState("doc-1")).resolves.toEqual({ canUndo: true, canRedo: false });
    await expect(adapter.submit("doc-1", "undo", 7)).resolves.toMatchObject({
      artifactId: "doc-1",
      revision: 8,
      events: [{ typeId: "document.historyApplied" }],
      canUndo: false,
      canRedo: true,
    });
  });

  it("rejects a history result for another artifact", async () => {
    const adapter = new HistoryAdapter({
      getState: async () => ({ canUndo: false, canRedo: false }),
      submit: async () => ({
        protocolVersion: 1,
        artifactId: "other-doc",
        transactionId: "history-1",
        revision: 1,
        invalidation: { changedEntities: [], changedContainers: [], structureChanged: false },
        mutations: [],
        events: [],
        canUndo: false,
        canRedo: false,
      }),
    });

    await expect(adapter.submit("doc-1", "undo", 0)).rejects.toThrow("artifactId");
  });

  it("reuses one history transaction id after an uncertain network failure", async () => {
    const transactionIds: string[] = [];
    let attempts = 0;
    const adapter = new HistoryAdapter({
      getState: async () => ({ canUndo: true, canRedo: false }),
      submit: async (documentId, _action, baseRevision, transactionId) => {
        transactionIds.push(transactionId);
        attempts += 1;
        if (attempts === 1) throw new Error("network disconnected");
        return {
          protocolVersion: 1,
          artifactId: documentId,
          transactionId,
          revision: baseRevision + 1,
          invalidation: { changedEntities: [], changedContainers: [], structureChanged: true },
          mutations: [],
          events: [],
          canUndo: false,
          canRedo: true,
        };
      },
    });

    await expect(adapter.submit("doc-1", "undo", 7)).rejects.toThrow("network");
    await expect(adapter.submit("doc-1", "undo", 7)).resolves.toMatchObject({ revision: 8 });
    expect(transactionIds[0]).toBe(transactionIds[1]);
  });

  it("keeps transaction ordering stable while retargeting revisions", () => {
    const outbox = new AutosaveOutbox();
    const command = {
      type: "setPageSetup" as const,
      pageSetup: null,
    };
    const first = outbox.enqueue({ artifactId: "doc-1", baseRevision: 4, sequence: 1, commands: [command] });
    const second = outbox.enqueue({ artifactId: "doc-1", baseRevision: 4, sequence: 2, commands: [command] });

    outbox.retarget(20);

    expect(outbox.entries().map((entry) => [entry.sequence, entry.envelope.baseRevision]))
      .toEqual([[1, 20], [2, 21]]);
    expect(outbox.beginAttempt(first.envelope.transactionId, 100)).not.toBeNull();
    expect(outbox.acknowledge(first.envelope.transactionId)?.sequence).toBe(1);
    expect(outbox.peek()?.envelope.transactionId).toBe(second.envelope.transactionId);
  });

  it("keeps the same transaction id across bounded exponential retries", () => {
    const outbox = new AutosaveOutbox({ maxAttempts: 3, retryBaseDelayMs: 10, retryMaxDelayMs: 100 });
    const transaction = outbox.enqueue({
      artifactId: "doc-1",
      baseRevision: 4,
      sequence: 1,
      commands: [{ type: "setPageSetup", pageSetup: null }],
    });
    const transactionId = transaction.envelope.transactionId;

    expect(outbox.beginAttempt(transactionId, 0)?.attempts).toBe(1);
    expect(outbox.markRetry(transactionId, new Error("offline"), 0)).toEqual({
      state: "retrying",
      attempts: 1,
      nextAttemptAt: 10,
    });
    expect(outbox.peekReady(9)).toBeNull();
    expect(outbox.beginAttempt(transactionId, 10)?.envelope.transactionId).toBe(transactionId);
    expect(outbox.markRetry(transactionId, new Error("offline"), 10)).toEqual({
      state: "retrying",
      attempts: 2,
      nextAttemptAt: 30,
    });
    expect(outbox.beginAttempt(transactionId, 30)?.attempts).toBe(3);
    expect(outbox.markRetry(transactionId, new Error("offline"), 30)?.state).toBe("failed");
    expect(outbox.blocked).toBe(true);
    expect(outbox.retry(transactionId, 40)).toBe(true);
    expect(outbox.peek()?.state).toBe("pending");
  });

  it("applies an acknowledgement and replays the remaining local queue", () => {
    const adapter = fakeAdapter();
    const applier = new CommitApplier(adapter);
    const outbox = new AutosaveOutbox();
    const command = { type: "setPageSetup" as const, pageSetup: null };
    const first = outbox.enqueue({ artifactId: "doc-1", baseRevision: 4, sequence: 1, commands: [command] });
    const second = outbox.enqueue({ artifactId: "doc-1", baseRevision: 4, sequence: 2, commands: [command] });

    const result = applier.applyAcknowledgement({
      committedSnapshot: createSnapshot(4),
      transaction: first,
      revision: 5,
      invalidation: {
        changedEntities: [{ entityType: "document.block", entityId: "p-1" }],
        changedContainers: [],
        structureChanged: false,
      },
    }, [second]);

    expect(result.committedSnapshot.artifact.revision).toBe(5);
    expect(result.localSnapshot.artifact.revision).toBe(6);
    expect(result.localChangedBlockIds).toContain("p-1");
    expect(result.localStructureChanged).toBe(false);
  });

  it("rebases queued commands on the latest committed snapshot", () => {
    const adapter = fakeAdapter();
    const outbox = new AutosaveOutbox();
    outbox.enqueue({
      artifactId: "doc-1",
      baseRevision: 4,
      sequence: 1,
      commands: [{ type: "setPageSetup", pageSetup: null }],
    });

    const result = new ConflictRebaser(new CommitApplier(adapter)).rebase(createSnapshot(9), outbox.entries());

    expect(result.committedSnapshot.artifact.revision).toBe(9);
    expect(result.localSnapshot.artifact.revision).toBe(10);
  });
});

function createSnapshot(revision: number): SnapshotEnvelope {
  return {
    protocolVersion: 1,
    artifact: {
      format: "open-office-artifact",
      schemaVersion: CURRENT_SCHEMA_VERSION,
      artifactId: "doc-1",
      revision,
      kind: "document",
      payload: {
        kind: "document",
        data: {
          root: [],
          blocks: [],
          pageSetup: null,
          pageSemantics: { sections: [], footnotes: [], endnotes: [] },
        },
      },
    },
  };
}

function fakeAdapter(): DocumentEngineAdapter {
  return {
    loadSnapshot(snapshot: SnapshotEnvelope): DocumentEngineSession {
      let revision = snapshot.artifact.revision;
      const session = {
        dispatch: (_batch: DocumentCommandBatch): DocumentChangeSet => {
          revision += 1;
          return {
            revision,
            changedBlocks: [],
            changedContainers: [],
            structureChanged: false,
            mutations: [],
          };
        },
        undo: () => { throw new Error("not implemented"); },
        redo: () => { throw new Error("not implemented"); },
        canUndo: () => false,
        canRedo: () => false,
        readBlock: () => { throw new Error("not implemented"); },
        readBlocks: () => { throw new Error("not implemented"); },
        findText: () => "[]",
        tableOfContents: () => "[]",
        readChangeSet: () => null,
        readSnapshot: () => ({ ...snapshot, artifact: { ...snapshot.artifact, revision } }),
        revision: () => revision,
        dispose: () => undefined,
      };
      return session as unknown as DocumentEngineSession;
    },
  } as DocumentEngineAdapter;
}
