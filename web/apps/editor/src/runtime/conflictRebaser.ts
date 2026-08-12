import type { SnapshotEnvelope } from "@open-office/schema/artifact";

import type { AutosaveTransaction } from "./autosaveOutbox.js";
import { CommitApplier } from "./commitApplier.js";

export interface RebaseResult {
  committedSnapshot: SnapshotEnvelope;
  localSnapshot: SnapshotEnvelope;
  /** A conflict response has no server invalidation; remote blocks are therefore unknown. */
  requiresFullProjectionRefresh: true;
}

/** Rebuilds the local projection from the latest server snapshot without mutating queued intent. */
export class ConflictRebaser {
  constructor(private readonly applier: CommitApplier) {}

  rebase(latest: SnapshotEnvelope, pending: readonly AutosaveTransaction[]): RebaseResult {
    const local = this.applier.replayWithChange(latest, pending);
    return {
      committedSnapshot: latest,
      localSnapshot: {
        ...local.snapshot,
        artifact: {
          ...local.snapshot.artifact,
          revision: latest.artifact.revision + pending.length,
        },
      },
      requiresFullProjectionRefresh: true,
    };
  }
}
