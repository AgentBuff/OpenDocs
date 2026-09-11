/** Typed browser boundary for the canonical `oo-mindmap` Rust engine. */
import { parseMindmapProjection, type MindmapProjection } from "@open-office/schema/api";
import { parseSnapshot, type SnapshotEnvelope } from "@open-office/schema/artifact";

export type MindmapViewTheme = "light" | "dark" | "highContrast";

interface MindmapSessionBinding {
  dispatch(batchJson: string): string;
  undo(): string;
  redo(): string;
  canUndo(): boolean;
  canRedo(): boolean;
  projection(theme: string): string;
  projectionWithMeasurements(theme: string, measurements: string): string;
  readSnapshot(): string;
  revision(): number | bigint;
  free?: () => void;
}

interface MindmapEngineBinding {
  loadSnapshot(snapshotJson: string): MindmapSessionBinding;
  updateProjection(previousJson: string, snapshotJson: string, invalidationJson: string, theme: string): string;
}

export interface MindmapProjectionInvalidation {
  changedEntities: Array<{ entityType: string; entityId: string }>;
  changedContainers: Array<{ entityType: string; entityId: string }>;
  structureChanged: boolean;
}

export async function updateMindmapProjection(
  previous: MindmapProjection,
  snapshot: SnapshotEnvelope,
  invalidation: MindmapProjectionInvalidation,
  theme: MindmapViewTheme,
): Promise<MindmapProjection> {
  const binding = await loadWasmMindmapEngine();
  const value = JSON.parse(binding.updateProjection(JSON.stringify(previous), JSON.stringify(snapshot), JSON.stringify(invalidation), theme)) as Record<string, unknown>;
  return parseMindmapProjection(value.projection);
}

let bindingPromise: Promise<MindmapEngineBinding> | null = null;

export function loadWasmMindmapEngine(): Promise<MindmapEngineBinding> {
  if (!bindingPromise) {
    bindingPromise = import("../wasm/oo_mindmap_wasm.js").then(async (module) => {
      await module.default({ module_or_path: new URL("../wasm/oo_mindmap_wasm_bg.wasm", import.meta.url) });
      return module;
    });
  }
  return bindingPromise;
}

/** Produces a projection in-browser from the exact same Rust implementation used by the server. */
export async function projectMindmapSnapshot(snapshot: SnapshotEnvelope, theme: MindmapViewTheme, measurements?: Record<string, { width: number; height: number }>): Promise<MindmapProjection> {
  const binding = await loadWasmMindmapEngine();
  const session = binding.loadSnapshot(JSON.stringify(snapshot));
  try {
    return parseMindmapProjection(JSON.parse(measurements ? session.projectionWithMeasurements(theme, JSON.stringify(measurements)) : session.projection(theme)) as unknown);
  } finally {
    session.free?.();
  }
}

/** Explicit round-trip used by contract tests; hot-path consumers use projections and mutations. */
export async function roundTripMindmapSnapshot(snapshot: SnapshotEnvelope): Promise<SnapshotEnvelope> {
  const binding = await loadWasmMindmapEngine();
  const session = binding.loadSnapshot(JSON.stringify(snapshot));
  try {
    return parseSnapshot(JSON.parse(session.readSnapshot()) as unknown);
  } finally {
    session.free?.();
  }
}
