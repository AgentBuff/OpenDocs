import type { DocumentEngineAdapter } from "@open-office/document-engine";
import type { Invalidation, SnapshotEnvelope } from "@open-office/schema/artifact";

import type { AutosaveTransaction } from "./autosaveOutbox.js";

export interface CommittedTransactionInput {
  committedSnapshot: SnapshotEnvelope;
  transaction: AutosaveTransaction;
  revision: number;
  /** Server invalidation is authoritative for the committed transaction. */
  invalidation?: Invalidation;
}

export interface ReplayChange {
  snapshot: SnapshotEnvelope;
  changedBlockIds: string[];
  changedContainerIds: string[];
  structureChanged: boolean;
}

export interface CommittedTransactionResult {
  committedSnapshot: SnapshotEnvelope;
  localSnapshot: SnapshotEnvelope;
  /** IDs that may have new values in the local projection after acknowledgement. */
  localChangedBlockIds: string[];
  localChangedContainerIds: string[];
  localStructureChanged: boolean;
}

/** Applies server acknowledgement to the committed projection and replays remaining local work. */
export class CommitApplier {
  constructor(private readonly adapter: DocumentEngineAdapter) {}

  replayWithChange(base: SnapshotEnvelope, transactions: readonly AutosaveTransaction[]): ReplayChange {
    const replay = this.adapter.loadSnapshot(base);
    const changedBlockIds = new Set<string>();
    const changedContainerIds = new Set<string>();
    let structureChanged = false;
    try {
      for (const transaction of transactions) {
        const change = replay.dispatch({ baseRevision: replay.revision(), commands: transaction.commands });
        change.changedBlocks.forEach((id) => changedBlockIds.add(id));
        change.changedContainers.forEach((id) => changedContainerIds.add(id));
        structureChanged ||= change.structureChanged;
      }
      return {
        snapshot: replay.readSnapshot(),
        changedBlockIds: [...changedBlockIds],
        changedContainerIds: [...changedContainerIds],
        structureChanged,
      };
    } finally {
      replay.dispose();
    }
  }

  applyAcknowledgement({ committedSnapshot, transaction, revision, invalidation }: CommittedTransactionInput, remaining: readonly AutosaveTransaction[]): CommittedTransactionResult {
    const committed = this.replayWithChange(committedSnapshot, [transaction]);
    const serverChangedBlockIds = invalidation?.changedEntities
      .filter((entity) => entity.entityType === "document.block")
      .map((entity) => entity.entityId) ?? [];
    const serverChangedContainerIds = invalidation?.changedContainers
      .filter((entity) => entity.entityType === "document.container")
      .map((entity) => entity.entityId) ?? [];
    const normalizedCommitted: SnapshotEnvelope = {
      ...committed.snapshot,
      artifact: { ...committed.snapshot.artifact, revision },
    };
    const local = this.replayWithChange(normalizedCommitted, remaining);
    const localChangedBlockIds = unique([
      ...committed.changedBlockIds,
      ...serverChangedBlockIds,
      ...local.changedBlockIds,
    ]);
    const localChangedContainerIds = unique([
      ...committed.changedContainerIds,
      ...serverChangedContainerIds,
      ...local.changedContainerIds,
    ]);
    return {
      committedSnapshot: normalizedCommitted,
      localSnapshot: {
        ...local.snapshot,
        artifact: { ...local.snapshot.artifact, revision: revision + remaining.length },
      },
      localChangedBlockIds,
      localChangedContainerIds,
      localStructureChanged: Boolean(invalidation?.structureChanged)
        || committed.structureChanged
        || local.structureChanged,
    };
  }
}

function unique(values: readonly string[]): string[] {
  return [...new Set(values)];
}
