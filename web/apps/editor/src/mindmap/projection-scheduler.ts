import { projectMindmapSnapshot, type MindmapViewTheme } from "@open-office/mindmap-engine";
import type { MindmapProjection } from "@open-office/schema/api";
import type { SnapshotEnvelope } from "@open-office/schema/artifact";

interface WorkerMessage {
  type: "projected" | "error";
  requestId: number;
  artifactId: string;
  revision: number;
  projection?: MindmapProjection;
  message?: string;
}

interface ProjectionWorker {
  postMessage(value: unknown): void;
  terminate(): void;
  onmessage: ((event: MessageEvent<WorkerMessage>) => void) | null;
  onerror: ((event: ErrorEvent) => void) | null;
}

interface PendingProjection {
  revision: number;
  artifactId: string;
  snapshot: SnapshotEnvelope;
  theme: MindmapViewTheme;
  measurements?: Record<string, { width: number; height: number }>;
  reject: (reason: Error) => void;
  resolve: (projection: MindmapProjection) => void;
}

type ProjectionFunction = typeof projectMindmapSnapshot;

export class ProjectionCancelledError extends Error {
  constructor() { super("Mindmap projection was superseded by a newer revision"); }
}

export class MindmapProjectionScheduler {
  private worker: ProjectionWorker | null = null;
  private nextRequestId = 1;
  private pending = new Map<number, PendingProjection>();

  constructor(
    private readonly workerFactory: () => ProjectionWorker | null = defaultWorkerFactory,
    private readonly fallbackProject: ProjectionFunction = projectMindmapSnapshot,
  ) {}

  project(snapshot: SnapshotEnvelope, theme: MindmapViewTheme, measurements?: Record<string, { width: number; height: number }>): Promise<MindmapProjection> {
    const revision = snapshot.artifact.revision;
    const artifactId = snapshot.artifact.artifactId;
    for (const [requestId, pending] of this.pending) {
      this.worker?.postMessage({ type: "cancel", requestId });
      pending.reject(new ProjectionCancelledError());
      this.pending.delete(requestId);
    }
    const worker = this.ensureWorker();
    if (!worker) return this.fallbackProject(snapshot, theme, measurements);
    const requestId = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      this.pending.set(requestId, { revision, artifactId, snapshot, theme, measurements, resolve, reject });
      worker.postMessage({ type: "project", requestId, artifactId, revision, snapshot, theme, measurements });
    });
  }

  dispose() {
    this.worker?.terminate();
    this.worker = null;
    for (const pending of this.pending.values()) pending.reject(new ProjectionCancelledError());
    this.pending.clear();
  }

  private ensureWorker(): ProjectionWorker | null {
    if (this.worker) return this.worker;
    this.worker = this.workerFactory();
    if (!this.worker) return null;
    this.worker.onmessage = (event) => {
      const pending = this.pending.get(event.data.requestId);
      if (!pending) return;
      this.pending.delete(event.data.requestId);
      if (pending.revision !== event.data.revision || pending.artifactId !== event.data.artifactId) {
        pending.reject(new ProjectionCancelledError());
      } else if (event.data.type === "projected" && event.data.projection) {
        pending.resolve(event.data.projection);
      } else {
        pending.reject(new Error(event.data.message ?? "Mindmap projection worker failed"));
      }
    };
    this.worker.onerror = () => {
      const pending = [...this.pending.values()];
      this.pending.clear();
      this.worker?.terminate();
      this.worker = null;
      for (const request of pending) {
        void this.fallbackProject(request.snapshot, request.theme, request.measurements)
          .then(request.resolve, request.reject);
      }
    };
    return this.worker;
  }
}

function defaultWorkerFactory(): ProjectionWorker | null {
  if (typeof Worker === "undefined") return null;
  return new Worker(new URL("./projection-worker.ts", import.meta.url), { type: "module", name: "mindmap-projection" });
}
