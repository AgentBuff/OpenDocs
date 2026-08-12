import type { SnapshotEnvelope } from "@open-office/schema/artifact";

export interface DocumentHistoryState {
  canUndo: boolean;
  canRedo: boolean;
}

export interface LoadedDocument {
  snapshot: SnapshotEnvelope;
  history: DocumentHistoryState;
}

export interface DocumentLoaderDependencies {
  getArtifact: (documentId: string) => Promise<SnapshotEnvelope>;
  getHistoryState: (documentId: string) => Promise<DocumentHistoryState>;
}

/** Reads the authoritative snapshot and history state as one session bootstrap operation. */
export class DocumentLoader {
  constructor(private readonly dependencies: DocumentLoaderDependencies) {}

  async load(documentId: string): Promise<LoadedDocument> {
    const [snapshot, history] = await Promise.all([
      this.dependencies.getArtifact(documentId),
      this.dependencies.getHistoryState(documentId),
    ]);
    if (snapshot.artifact.kind !== "document") {
      throw new Error("当前文档不是 Document Artifact");
    }
    return { snapshot, history };
  }
}
