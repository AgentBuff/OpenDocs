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
  DocumentHeaderFooter,
  DocumentNote,
  DocumentPageNumbering,
  ArtifactPageSetup,
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

export interface DocumentSearchOptions {
  caseSensitive?: boolean;
  wholeWord?: boolean;
}

export type DocumentTextTarget =
  | { type: "block"; blockId: string }
  | { type: "tableCell"; blockId: string; rowId: string; cellId: string };

export interface DocumentSearchMatch {
  target: DocumentTextTarget;
  start: number;
  end: number;
}

export interface DocumentTocItem {
  blockId: string;
  level: number;
  text: string;
}

export interface DocumentPrintProjection {
  revision: number;
  sections: DocumentPrintSection[];
  footnotes: DocumentNote[];
  endnotes: DocumentNote[];
}

export interface DocumentPrintSection {
  sectionId: string | null;
  rootBlockIds: string[];
  pageSetup: ArtifactPageSetup | null;
  header: DocumentHeaderFooter | null;
  footer: DocumentHeaderFooter | null;
  pageNumbering: DocumentPageNumbering | null;
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
  findText(query: string, optionsJson: string): string;
  tableOfContents(): string;
  printProjection(): string;
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

  findText(query: string, options: DocumentSearchOptions = {}): DocumentSearchMatch[] {
    if (typeof query !== "string") throw new Error("搜索文本必须是字符串");
    const value: unknown = JSON.parse(this.binding.findText(query, JSON.stringify(options)));
    if (!Array.isArray(value)) throw new Error("DocumentEngine 返回了无效的搜索结果");
    return value.map((item, index) => parseSearchMatch(item, index));
  }

  tableOfContents(): DocumentTocItem[] {
    const value: unknown = JSON.parse(this.binding.tableOfContents());
    if (!Array.isArray(value)) throw new Error("DocumentEngine 返回了无效的目录投影");
    return value.map((item, index) => {
      if (!isRecord(item)) throw new Error(`目录项目 ${index} 无效`);
      const blockId = item.blockId;
      const level = item.level;
      const text = item.text;
      if (typeof blockId !== "string" || !blockId || !isNonNegativeInteger(level) || level < 1 || level > 6 || typeof text !== "string") {
        throw new Error(`目录项目 ${index} 无效`);
      }
      return { blockId, level, text };
    });
  }

  printProjection(): DocumentPrintProjection {
    const value: unknown = JSON.parse(this.binding.printProjection());
    if (!isRecord(value)) throw new Error("DocumentEngine 返回了无效的打印投影");
    const revision = asNonNegativeInteger(value.revision, "printProjection.revision");
    if (!Array.isArray(value.sections) || !Array.isArray(value.footnotes) || !Array.isArray(value.endnotes)) {
      throw new Error("DocumentEngine 返回了无效的打印投影");
    }
    const sections = value.sections.map((item, index) => {
      if (!isRecord(item) || !isNullableString(item.sectionId)) {
        throw new Error(`printProjection.sections[${index}] 无效`);
      }
      const rootBlockIds = asStringArray(item.rootBlockIds, `printProjection.sections[${index}].rootBlockIds`);
      if ((item.pageSetup !== null && !isRecord(item.pageSetup))
        || (item.header !== null && !isRecord(item.header))
        || (item.footer !== null && !isRecord(item.footer))
        || (item.pageNumbering !== null && !isRecord(item.pageNumbering))) {
        throw new Error(`printProjection.sections[${index}] 无效`);
      }
      return {
        sectionId: item.sectionId,
        rootBlockIds,
        pageSetup: item.pageSetup as ArtifactPageSetup | null,
        header: item.header as DocumentHeaderFooter | null,
        footer: item.footer as DocumentHeaderFooter | null,
        pageNumbering: item.pageNumbering as DocumentPageNumbering | null,
      };
    });
    if ([...value.footnotes, ...value.endnotes].some((note) => !isRecord(note) || typeof note.id !== "string" || !isRecord(note.anchor) || !Array.isArray(note.content))) {
      throw new Error("DocumentEngine 返回了无效的 note 投影");
    }
    return {
      revision,
      sections,
      footnotes: value.footnotes as unknown as DocumentNote[],
      endnotes: value.endnotes as unknown as DocumentNote[],
    };
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

function parseSearchMatch(value: unknown, index: number): DocumentSearchMatch {
  if (!isRecord(value) || !isNonNegativeInteger(value.start) || !isNonNegativeInteger(value.end) || value.end < value.start) {
    throw new Error(`搜索结果 ${index} 范围无效`);
  }
  const target = value.target;
  if (!isRecord(target) || typeof target.blockId !== "string" || !target.blockId) {
    throw new Error(`搜索结果 ${index} 目标无效`);
  }
  if (target.type === "block") {
    return { target: { type: "block", blockId: target.blockId }, start: value.start, end: value.end };
  }
  if (target.type === "tableCell" && typeof target.rowId === "string" && target.rowId && typeof target.cellId === "string" && target.cellId) {
    return {
      target: { type: "tableCell", blockId: target.blockId, rowId: target.rowId, cellId: target.cellId },
      start: value.start,
      end: value.end,
    };
  }
  throw new Error(`搜索结果 ${index} 目标无效`);
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
    case "setPageSemantics":
      return isRecord(value.before) && isRecord(value.after)
        && Array.isArray(value.before.sections) && Array.isArray(value.before.footnotes) && Array.isArray(value.before.endnotes)
        && Array.isArray(value.after.sections) && Array.isArray(value.after.footnotes) && Array.isArray(value.after.endnotes);
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
