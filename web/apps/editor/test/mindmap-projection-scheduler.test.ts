import { describe, expect, it } from "vitest";
import type { SnapshotEnvelope } from "@open-office/schema/artifact";
import { MindmapProjectionScheduler, ProjectionCancelledError } from "../src/mindmap/projection-scheduler.js";

class FakeWorker {
  onmessage: ((event: MessageEvent<any>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  messages: any[] = [];
  postMessage(value: unknown) { this.messages.push(value); }
  terminate() {}
}

function snapshot(revision: number): SnapshotEnvelope {
  return { protocolVersion: 1, artifact: { format: "open-office-artifact", schemaVersion: 10, artifactId: "m", revision, kind: "mindmap", payload: { kind: "mindmap", data: { root: null, nodes: [], edges: [], summaries: [], boundaries: [], formulas: [], settings: { layout: "logicalRight", themeId: null, connector: { shape: "orthogonal", color: null, width: 2, dashed: false } } } } } };
}

describe("mindmap projection scheduler", () => {
  it("cancels older work and ignores a late result", async () => {
    const worker = new FakeWorker();
    const scheduler = new MindmapProjectionScheduler(() => worker);
    const first = scheduler.project(snapshot(1), "light");
    const second = scheduler.project(snapshot(2), "light");
    await expect(first).rejects.toBeInstanceOf(ProjectionCancelledError);
    expect(worker.messages.map((message) => message.type)).toEqual(["project", "cancel", "project"]);
    worker.onmessage?.({ data: { type: "projected", requestId: 1, artifactId: "m", revision: 1, projection: {} } } as MessageEvent);
    worker.onmessage?.({ data: { type: "projected", requestId: 2, artifactId: "m", revision: 2, projection: { theme: "light", layout: { nodes: [], width: 0, height: 0 }, edges: { routes: [] }, advanced: { summaries: [], boundaries: [], formulas: [] } } } } as MessageEvent);
    await expect(second).resolves.toMatchObject({ theme: "light" });
  });

  it("rejects a mismatched revision without accepting stale output", async () => {
    const worker = new FakeWorker();
    const scheduler = new MindmapProjectionScheduler(() => worker);
    const result = scheduler.project(snapshot(4), "light");
    worker.onmessage?.({ data: { type: "projected", requestId: 1, artifactId: "m", revision: 3, projection: {} } } as MessageEvent);
    await expect(result).rejects.toBeInstanceOf(ProjectionCancelledError);
  });

  it("rejects output produced for another artifact", async () => {
    const worker = new FakeWorker();
    const scheduler = new MindmapProjectionScheduler(() => worker);
    const result = scheduler.project(snapshot(4), "light");
    worker.onmessage?.({ data: { type: "projected", requestId: 1, artifactId: "other", revision: 4, projection: {} } } as MessageEvent);
    await expect(result).rejects.toBeInstanceOf(ProjectionCancelledError);
  });

  it("recovers from a worker crash with the canonical direct projector", async () => {
    const worker = new FakeWorker();
    const fallback = async () => ({ theme: "dark", layout: { nodes: [], width: 0, height: 0 }, edges: { routes: [] }, advanced: { summaries: [], boundaries: [], formulas: [] } } as any);
    const scheduler = new MindmapProjectionScheduler(() => worker, fallback);
    const result = scheduler.project(snapshot(7), "dark");
    worker.onerror?.({ message: "boom" } as ErrorEvent);
    await expect(result).resolves.toMatchObject({ theme: "dark", layout: { nodes: [] } });
  });
});
