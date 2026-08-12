/**
 * Browser adapter for the canonical `oo-document-wasm` binding.
 *
 * This package owns no Document state and contains no operation implementation. It only turns
 * typed protocol values into the binding's JSON boundary and parses the incremental results
 * returned by `DocumentEngine::execute`.
 */

import type {
  DocumentBlock,
  DocumentMutation,
  DocumentCommand,
  SnapshotEnvelope,
} from "@open-office/schema/artifact";
import { parseSnapshot } from "@open-office/schema/artifact";

export interface DocumentCommandBatch {
  baseRevision: number;
  commands: DocumentCommand[];
}

export interface DocumentChangeSet {
  revision: number;
  changedBlocks: string[];
  changedContainers: string[];
  structureChanged: boolean;
  mutations: DocumentMutation[];
}

/** The generated wasm-bindgen module shape. */
export interface DocumentEngineBinding {
  loadSnapshot(snapshotJson: string): DocumentSessionBinding;
}

/** Lazy loader boundary for the generated wasm-bindgen module. The editor can inject this
 * loader during application bootstrap without importing a generated binary into its bundle. */
export type DocumentEngineLoader = () => DocumentEngineBinding | PromiseLike<DocumentEngineBinding>;

/** The generated `DocumentSession` shape. */
export interface DocumentSessionBinding {
  dispatch(transactionJson: string): string;
  undo(): string;
  redo(): string;
  canUndo(): boolean;
  canRedo(): boolean;
  readBlock(blockId: string): string;
  /** Read invalidated blocks in one wasm JSON crossing; never returns a snapshot. */
  readBlocks(blockIdsJson: string): string;
  readChangeSet(): string | null | undefined;
  readSnapshot(): string;
  revision(): number | bigint;
  free?: () => void;
}

/**
 * A typed façade over the generated binding. This class deliberately does not expose a mutable
 * model; all writes still go through the Rust `DocumentEngine::execute` entry point.
 */
export class DocumentEngineAdapter {
  constructor(private readonly binding: DocumentEngineBinding) {}

  loadSnapshot(snapshot: SnapshotEnvelope): DocumentEngineSession {
    return new DocumentEngineSession(this.binding.loadSnapshot(JSON.stringify(snapshot)));
  }
}

/**
 * Resolve a wasm binding only when the editor needs a document session. Keeping the loader
 * injectable lets tests use a deterministic fake and keeps the generated `.wasm` artifact out of
 * the TypeScript package's static dependency graph.
 */
export async function createDocumentEngine(
  loadBinding: DocumentEngineLoader,
): Promise<DocumentEngineAdapter> {
  return new DocumentEngineAdapter(await loadBinding());
}

let wasmBindingPromise: Promise<DocumentEngineBinding> | null = null;

/**
 * Lazily load the generated browser binding and its wasm binary.
 *
 * Keeping this in the adapter package gives the editor one canonical loading boundary. Tests
 * and embedders can still inject a deterministic `DocumentEngineLoader` instead of importing a
 * wasm module directly. The promise is shared so multiple editor mounts never instantiate the
 * binary more than once.
 */
export function loadWasmDocumentEngine(): Promise<DocumentEngineBinding> {
  if (!wasmBindingPromise) {
    wasmBindingPromise = import("../wasm/oo_document_wasm.js").then(async (module) => {
      await module.default({
        module_or_path: new URL("../wasm/oo_document_wasm_bg.wasm", import.meta.url),
      });
      return module;
    });
  }
  return wasmBindingPromise;
}

export class DocumentEngineSession {
  constructor(private readonly binding: DocumentSessionBinding) {}

  dispatch(transaction: DocumentCommandBatch): DocumentChangeSet {
    const raw = this.binding.dispatch(JSON.stringify(transaction));
    return parseChangeSet(JSON.parse(raw) as unknown);
  }

  undo(): DocumentChangeSet {
    return parseChangeSet(JSON.parse(this.binding.undo()) as unknown);
  }

  redo(): DocumentChangeSet {
    return parseChangeSet(JSON.parse(this.binding.redo()) as unknown);
  }

  canUndo(): boolean {
    return this.binding.canUndo();
  }

  canRedo(): boolean {
    return this.binding.canRedo();
  }

  readBlock(blockId: string): DocumentBlock {
    return JSON.parse(this.binding.readBlock(blockId)) as DocumentBlock;
  }

  readBlocks(blockIds: readonly string[]): DocumentBlock[] {
    if (blockIds.some((id) => typeof id !== "string" || id.length === 0)) {
      throw new Error("readBlocks 需要非空 BlockId");
    }
    const value: unknown = JSON.parse(this.binding.readBlocks(JSON.stringify(blockIds)));
    if (!Array.isArray(value) || value.length !== blockIds.length) {
      throw new Error("DocumentEngine 返回了无效的 block 列表");
    }
    return value.map((block, index) => {
      if (!isRecord(block) || block.id !== blockIds[index]) {
        throw new Error(`DocumentEngine 返回的 block 顺序或 ID 不匹配：${blockIds[index]}`);
      }
      return block as unknown as DocumentBlock;
    });
  }

  readChangeSet(): DocumentChangeSet | null {
    const raw = this.binding.readChangeSet();
    return raw == null ? null : parseChangeSet(JSON.parse(raw) as unknown);
  }

  /** Full snapshots are intentionally explicit and never part of `dispatch`. */
  readSnapshot(): SnapshotEnvelope {
    return parseSnapshot(JSON.parse(this.binding.readSnapshot()) as unknown);
  }

  revision(): number {
    const revision = this.binding.revision();
    const numberRevision = typeof revision === "bigint" ? Number(revision) : revision;
    if (!Number.isSafeInteger(numberRevision) || numberRevision < 0) {
      throw new Error("DocumentEngine revision 超出 JavaScript 安全整数范围");
    }
    return numberRevision;
  }

  dispose(): void {
    this.binding.free?.();
  }
}

function parseChangeSet(value: unknown): DocumentChangeSet {
  if (!isRecord(value)) throw new Error("DocumentEngine 返回了无效 ChangeSet");
  const revision = asNonNegativeInteger(value.revision, "ChangeSet.revision");
  const changedBlocks = asStringArray(value.changedBlocks, "ChangeSet.changedBlocks");
  const changedContainers = asStringArray(value.changedContainers, "ChangeSet.changedContainers");
  if (typeof value.structureChanged !== "boolean") {
    throw new Error("ChangeSet.structureChanged 必须是 boolean");
  }
  if (!Array.isArray(value.mutations)) throw new Error("ChangeSet.mutations 必须是数组");
  const mutations = value.mutations.map((mutation, index) => parseDocumentMutation(mutation, index));
  return {
    revision,
    changedBlocks,
    changedContainers,
    structureChanged: value.structureChanged,
    mutations,
  };
}

function parseDocumentMutation(value: unknown, index: number): DocumentMutation {
  if (!isDocumentMutation(value)) {
    throw new Error(`ChangeSet.mutations[${index}] 不是合法的 DocumentMutation`);
  }
  return value;
}

function isDocumentMutation(value: unknown): value is DocumentMutation {
  if (!isRecord(value) || typeof value.type !== "string") return false;
  switch (value.type) {
    case "insert":
    case "removeInserted":
      return isRecord(value.block) && typeof value.block.id === "string"
        && isNullableString(value.parentId) && isNonNegativeInteger(value.index);
    case "delete":
    case "restore":
      return typeof value.blockId === "string" && Array.isArray(value.removed)
        && isNullableString(value.parentId) && isNonNegativeInteger(value.index);
    case "update":
      return typeof value.blockId === "string" && isRecord(value.before) && isRecord(value.after);
    case "move":
      return typeof value.blockId === "string"
        && isNullableString(value.fromParentId)
        && isNullableString(value.toParentId)
        && isNonNegativeInteger(value.fromIndex)
        && isNonNegativeInteger(value.toIndex);
    case "setPageSetup":
      return (value.before === null || isRecord(value.before))
        && (value.after === null || isRecord(value.after));
    default:
      return false;
  }
}

function isNullableString(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function asNonNegativeInteger(value: unknown, field: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${field} 必须是非负整数`);
  }
  return value;
}

function asStringArray(value: unknown, field: string): string[] {
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) {
    throw new Error(`${field} 必须是字符串数组`);
  }
  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
