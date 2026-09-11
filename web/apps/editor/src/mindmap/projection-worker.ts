/// <reference lib="webworker" />

import { projectMindmapSnapshot } from "@open-office/mindmap-engine";
import type { SnapshotEnvelope } from "@open-office/schema/artifact";
import type { MindmapViewTheme } from "@open-office/mindmap-engine";

type ProjectRequest = {
  type: "project";
  requestId: number;
  artifactId: string;
  revision: number;
  snapshot: SnapshotEnvelope;
  theme: MindmapViewTheme;
  measurements?: Record<string, { width: number; height: number }>;
};
type CancelRequest = { type: "cancel"; requestId: number };

const cancelled = new Set<number>();

self.onmessage = (event: MessageEvent<ProjectRequest | CancelRequest>) => {
  const request = event.data;
  if (request.type === "cancel") {
    cancelled.add(request.requestId);
    return;
  }
  void projectMindmapSnapshot(request.snapshot, request.theme, request.measurements).then((projection) => {
    if (!cancelled.delete(request.requestId)) {
      self.postMessage({ type: "projected", requestId: request.requestId, artifactId: request.artifactId, revision: request.revision, projection });
    }
  }).catch((reason: unknown) => {
    if (!cancelled.delete(request.requestId)) {
      self.postMessage({ type: "error", requestId: request.requestId, artifactId: request.artifactId, revision: request.revision, message: reason instanceof Error ? reason.message : String(reason) });
    }
  });
};

export {};
