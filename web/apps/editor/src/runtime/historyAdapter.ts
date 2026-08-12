import type {
  CommitResult,
  DocumentHistoryAction,
} from "@open-office/schema/artifact";

export interface HistoryState {
  canUndo: boolean;
  canRedo: boolean;
}

/** The server response for a history intent. It is deliberately a CommitResult, not a snapshot. */
export interface HistoryCommitResult extends CommitResult, HistoryState {}

export interface HistoryAdapterDependencies {
  getState: (documentId: string) => Promise<HistoryState>;
  submit: (
    documentId: string,
    action: DocumentHistoryAction,
    baseRevision: number,
    transactionId: string,
  ) => Promise<HistoryCommitResult>;
}

/**
 * Server-authoritative history boundary.
 *
 * History is a domain operation, not a second client-side snapshot stack. This adapter only
 * reads the authoritative affordances and returns the typed CommitResult produced by the server;
 * the session decides when to reload the canonical artifact snapshot.
 */
export class HistoryAdapter {
  private readonly pendingIntentIds = new Map<string, string>();

  constructor(private readonly dependencies: HistoryAdapterDependencies) {}

  readState(documentId: string): Promise<HistoryState> {
    return this.dependencies.getState(documentId);
  }

  async submit(
    documentId: string,
    action: DocumentHistoryAction,
    baseRevision: number,
  ): Promise<HistoryCommitResult> {
    if (!Number.isSafeInteger(baseRevision) || baseRevision < 0) {
      throw new Error("history baseRevision 必须是非负整数");
    }
    const key = `${documentId}:${action}:${baseRevision}`;
    const transactionId = this.pendingIntentIds.get(key) ?? randomId();
    this.pendingIntentIds.set(key, transactionId);
    try {
      const result = await this.dependencies.submit(documentId, action, baseRevision, transactionId);
      if (result.artifactId !== documentId) {
        throw new Error("history CommitResult artifactId 与文档不一致");
      }
      if (!Number.isSafeInteger(result.revision) || result.revision < 0) {
        throw new Error("history CommitResult revision 无效");
      }
      this.pendingIntentIds.delete(key);
      return result;
    } catch (error) {
      // Keep the same transaction id after an uncertain network result. The server can then
      // resolve a retry idempotently instead of applying a second undo/redo intent.
      this.pendingIntentIds.set(key, transactionId);
      throw error;
    }
  }
}

function randomId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
