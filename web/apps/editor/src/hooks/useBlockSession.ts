import { useCallback, useEffect, useRef, useState } from "react";

import {
  createDocumentEngine,
  loadWasmDocumentEngine,
  type DocumentEngineAdapter,
  type DocumentEngineSession,
  type DocumentSearchMatch,
  type DocumentSearchOptions,
  type DocumentTocItem,
  type DocumentPrintProjection,
} from "@open-office/document-engine";
import type {
  CodeBlockConfig,
  ArtifactPageSetup,
  BlockData,
  BlockPresentation,
  Invalidation,
  DocumentBlock,
  DocumentBlockKind,
  DocumentModel,
  DocumentCommand,
  DocumentSection,
  DocumentNote,
  InlineStylePatch,
  InlineStyle,
  RichText,
  SnapshotEnvelope,
  TableBorderPatch,
  TableBorder,
  TableBorderPreset,
  TextRange,
} from "@open-office/schema/artifact";
import { defaultCodeBlockConfig, defaultImageTransform } from "@open-office/schema/artifact";

import { api, ApiRequestError } from "../api.js";
import { AutosaveOutbox } from "../runtime/autosaveOutbox.js";
import { autosaveDelayMs } from "../runtime/autosaveTiming.js";
import { CommitApplier } from "../runtime/commitApplier.js";
import { ConflictRebaser } from "../runtime/conflictRebaser.js";
import { HistoryAdapter } from "../runtime/historyAdapter.js";
import { DocumentLoader } from "../runtime/loader.js";
import { BlockProjectionStore } from "../store/blockProjectionStore.js";
import { readBlockTextSelection, restoreBlockTextSelection } from "../utils/blockSelection.js";
import { concatRichText, sliceRichText } from "../blocks/richText.js";

export interface BlockSessionState {
  loading: boolean;
  error: string | null;
  dirty: boolean;
  saving: boolean;
  revision: number;
  wordCount: number;
  blockCount: number;
  activeBlockId: string | null;
  canUndo: boolean;
  canRedo: boolean;
}

const INITIAL: BlockSessionState = {
  loading: true,
  error: null,
  dirty: false,
  saving: false,
  revision: 0,
  wordCount: 0,
  blockCount: 0,
  activeBlockId: null,
  canUndo: false,
  canRedo: false,
};

export interface BlockSessionApi {
  snapshot: SnapshotEnvelope | null;
  /** Read-only block projection consumed by the DOM editor. */
  projection: BlockProjectionStore;
  state: BlockSessionState;
  setActiveBlock: (id: string | null) => void;
  updateContent: (id: string, content: RichText) => void;
  findText: (query: string, options?: DocumentSearchOptions) => DocumentSearchMatch[];
  tableOfContents: () => DocumentTocItem[];
  printProjection: () => DocumentPrintProjection | null;
  replaceAllText: (query: string, replacement: string, options?: DocumentSearchOptions) => boolean;
  replaceTextMatch: (
    match: DocumentSearchMatch,
    query: string,
    replacement: string,
    options?: DocumentSearchOptions,
  ) => boolean;
  convertBlock: (id: string, kind: DocumentBlockKind) => void;
  setBlockPresentation: (id: string, presentation: Record<string, unknown | null>) => void;
  insertAfter: (
    id: string,
    kind?: DocumentBlockKind,
    presentation?: Extract<DocumentCommand, { type: "setBlockPresentation" }>['patch'],
  ) => string | null;
  insertBefore: (
    id: string,
    kind?: DocumentBlockKind,
    presentation?: Extract<DocumentCommand, { type: "setBlockPresentation" }>['patch'],
  ) => string | null;
  insertPastedImage: (id: string, file: File) => Promise<string | null>;
  /** Persist an image-domain patch through the canonical document transaction. */
  setImageConfig: (
    id: string,
    patch: Extract<DocumentCommand, { type: "setImageConfig" }>['patch'],
  ) => void;
  /** Upload a derived image asset and atomically switch the image block to it. */
  replaceImageAsset: (id: string, file: File) => Promise<boolean>;
  assetUrl: (assetId: string) => string;
  insertTableAfter: (id: string, rows?: number, columns?: number) => string | null;
  updateTableCell: (blockId: string, rowId: string, cellId: string, content: RichText) => void;
  patchTableCellInlineRange: (
    blockId: string,
    rowId: string,
    cellId: string,
    range: TextRange,
    patch: InlineStylePatch,
  ) => void;
  formatTableCells: (
    blockId: string,
    selection: Extract<DocumentCommand, { type: "formatTableCells" }>["selection"],
    patch: Extract<DocumentCommand, { type: "formatTableCells" }>["patch"],
  ) => void;
  setTableBorders: (
    blockId: string,
    selection: Extract<DocumentCommand, { type: "setTableBorders" }>["selection"],
    patch: TableBorderPatch,
  ) => void;
  applyTableBorderPreset: (
    blockId: string,
    selection: Extract<DocumentCommand, { type: "applyTableBorderPreset" }>['selection'],
    preset: TableBorderPreset,
    border?: TableBorder,
  ) => void;
  setTodoChecked: (id: string, checked: boolean) => void;
  convertToLink: (id: string, url: string) => void;
  setLinkTarget: (id: string, url: string) => void;
  setCodeConfig: (id: string, config: CodeBlockConfig) => void;
  insertTableRow: (blockId: string, boundaryIndex?: number) => void;
  insertTableColumn: (blockId: string, boundaryIndex?: number) => void;
  deleteTableRow: (blockId: string, rowIndex: number) => void;
  deleteTableColumn: (blockId: string, columnIndex: number) => void;
  setTableColumnWidth: (blockId: string, columnId: string, width: number) => void;
  setTableColumnWidths: (blockId: string, leftColumnId: string, leftWidth: number, rightColumnId: string, rightWidth: number) => void;
  setTableRowHeight: (blockId: string, rowId: string, height: number) => void;
  mergeTableCells: (blockId: string, range: Extract<DocumentCommand, { type: "mergeTableCells" }>['range']) => void;
  splitTableCells: (blockId: string, range: Extract<DocumentCommand, { type: "splitTableCells" }>['range']) => void;
  setPageSetup: (pageSetup: ArtifactPageSetup | null) => void;
  upsertSection: (section: DocumentSection, index: number) => void;
  deleteSection: (sectionId: string) => void;
  upsertNote: (noteKind: "footnote" | "endnote", note: DocumentNote) => void;
  deleteNote: (noteKind: "footnote" | "endnote", noteId: string) => void;
  deleteBlock: (id: string) => void;
  /** Delete a native selection, including a selection spanning sibling blocks. */
  deleteTextSelection: () => string | null;
  clearDocument: () => void;
  toggleMark: (mark: "bold" | "italic" | "underline" | "strikethrough") => void;
  setInlineAttrs: (attrs: Record<string, unknown | null>) => void;
  captureInlineAttrs: () => Record<string, unknown> | null;
  adjustFontSize: (delta: -1 | 1) => void;
  undo: () => void;
  redo: () => void;
  save: () => Promise<void>;
  reload: () => Promise<void>;
  reportError: (error: unknown) => void;
}

/**
 * Document session backed by the canonical Rust DocumentEngine adapter.
 *
 * React owns only a view snapshot. Writes are dispatched to the engine and the resulting ChangeSet
 * reads changed blocks (or a full snapshot for structural changes). The hook
 * is only a command facade over the canonical projection and engine snapshot.
 */
export function useBlockSession(documentId: string): BlockSessionApi {
  const [snapshot, setSnapshot] = useState<SnapshotEnvelope | null>(null);
  const projectionRef = useRef<BlockProjectionStore | null>(null);
  const snapshotRef = useRef<SnapshotEnvelope | null>(null);
  const committedSnapshotRef = useRef<SnapshotEnvelope | null>(null);
  const adapterRef = useRef<DocumentEngineAdapter | null>(null);
  const engineRef = useRef<DocumentEngineSession | null>(null);
  const outboxRef = useRef<AutosaveOutbox | null>(null);
  const loaderRef = useRef<DocumentLoader | null>(null);
  const commitApplierRef = useRef<CommitApplier | null>(null);
  const conflictRebaserRef = useRef<ConflictRebaser | null>(null);
  const historyAdapterRef = useRef<HistoryAdapter | null>(null);
  const sequenceRef = useRef(0);
  const baseRevisionRef = useRef(0);
  const autosaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const firstPendingAtRef = useRef<number | null>(null);
  const lastEditAtRef = useRef<number | null>(null);
  const flightRef = useRef(false);
  const flushRef = useRef<() => Promise<void>>(() => Promise.resolve());
  const wordCountRef = useRef(0);
  const [state, setState] = useState<BlockSessionState>(INITIAL);

  const projection = projectionRef.current ?? (projectionRef.current = new BlockProjectionStore());
  const historyAdapter = historyAdapterRef.current ?? (historyAdapterRef.current = new HistoryAdapter({
    getState: api.getHistoryState,
    submit: api.submitHistory,
  }));
  const outbox = outboxRef.current ?? (outboxRef.current = new AutosaveOutbox());
  const loader = loaderRef.current ?? (loaderRef.current = new DocumentLoader({
    getArtifact: api.getArtifact,
    getHistoryState: (id) => historyAdapter.readState(id),
  }));

  const clearAutosaveTimer = useCallback(() => {
    if (autosaveTimerRef.current) clearTimeout(autosaveTimerRef.current);
    autosaveTimerRef.current = null;
  }, []);

  const scheduleRetry = useCallback((delay: number) => {
    if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
    retryTimerRef.current = setTimeout(() => {
      retryTimerRef.current = null;
      void flushRef.current();
    }, Math.max(25, delay));
  }, []);

  const scheduleAutosave = useCallback(() => {
    if (outbox.isEmpty || flightRef.current) return;
    const transportDelay = outbox.nextAttemptDelay();
    if (transportDelay === null) return;
    if (transportDelay > 0) {
      scheduleRetry(transportDelay);
      return;
    }
    const now = Date.now();
    firstPendingAtRef.current ??= now;
    lastEditAtRef.current ??= now;
    clearAutosaveTimer();
    autosaveTimerRef.current = setTimeout(() => {
      autosaveTimerRef.current = null;
      void flushRef.current();
    }, autosaveDelayMs({
      now,
      firstPendingAt: firstPendingAtRef.current,
      lastEditAt: lastEditAtRef.current,
    }));
  }, [clearAutosaveTimer, outbox, scheduleRetry]);

  const reportError = useCallback((error: unknown) => {
    setState((current) => ({
      ...current,
      error: error instanceof Error ? error.message : String(error),
      saving: false,
    }));
  }, []);

  const syncState = useCallback((next: SnapshotEnvelope | null, patch?: Partial<BlockSessionState>) => {
    const model = next?.artifact.payload.kind === "document" ? next.artifact.payload.data : null;
    setState((current) => ({
      ...current,
      revision: next?.artifact.revision ?? current.revision,
      wordCount: model ? wordCountRef.current : 0,
      blockCount: model?.blocks.length ?? 0,
      ...patch,
    }));
  }, []);

  const replaceEngine = useCallback((next: SnapshotEnvelope): DocumentEngineSession => {
    const adapter = adapterRef.current;
    if (!adapter) throw new Error("DocumentEngine 尚未加载");
    engineRef.current?.dispose();
    const engine = adapter.loadSnapshot(next);
    engineRef.current = engine;
    return engine;
  }, []);

  const load = useCallback(async (projectionInvalidation?: Invalidation) => {
    setState((current) => ({ ...current, loading: true, error: null }));
    try {
      const { snapshot, history } = await loader.load(documentId);
      adapterRef.current ??= await createDocumentEngine(loadWasmDocumentEngine);
      commitApplierRef.current = new CommitApplier(adapterRef.current);
      conflictRebaserRef.current = new ConflictRebaser(commitApplierRef.current);
      replaceEngine(snapshot);
      outbox.clear();
      baseRevisionRef.current = snapshot.artifact.revision;
      committedSnapshotRef.current = snapshot;
      wordCountRef.current = snapshot.artifact.payload.kind === "document" ? countWords(snapshot.artifact.payload.data) : 0;
      snapshotRef.current = snapshot;
      if (projectionInvalidation) {
        const changedIds = projectionInvalidation.changedEntities
          .filter((entity) => entity.entityType === "document.block")
          .map((entity) => entity.entityId)
          .concat(projectionInvalidation.changedContainers
            .filter((entity) => entity.entityType === "document.container")
            .map((entity) => entity.entityId));
        projection.applySnapshot(snapshot, changedIds, false);
      } else {
        projection.replaceSnapshot(snapshot, false);
      }
      setSnapshot(snapshot);
      syncState(snapshot, {
        loading: false,
        dirty: false,
        saving: false,
        canUndo: history.canUndo,
        canRedo: history.canRedo,
      });
    } catch (error) {
      reportError(error);
      setState((current) => ({ ...current, loading: false }));
    }
  }, [documentId, loader, outbox, replaceEngine, reportError, syncState]);

  useEffect(() => {
    outbox.clear();
    firstPendingAtRef.current = null;
    lastEditAtRef.current = null;
    void load();
    return () => {
      clearAutosaveTimer();
      if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
      retryTimerRef.current = null;
      engineRef.current?.dispose();
      engineRef.current = null;
    };
  }, [clearAutosaveTimer, load]);

  const readChangeIntoSnapshot = useCallback((engine: DocumentEngineSession, change: {
    revision: number;
    changedBlocks: string[];
    changedContainers: string[];
    structureChanged: boolean;
  }): { snapshot: SnapshotEnvelope; changedBlocks: DocumentBlock[]; previousBlocks: DocumentBlock[] } => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document" || change.structureChanged) {
      return { snapshot: engine.readSnapshot(), changedBlocks: [], previousBlocks: [] };
    }
    // Read all invalidated entities through one typed WASM boundary crossing. This keeps the
    // hot path proportional to the changed BlockIds without repeatedly serializing JSON.
    const changedBlocks = engine.readBlocks(change.changedBlocks);
    const previousBlocks = change.changedBlocks
      .map((id) => projection.getBlock(id))
      .filter((block): block is DocumentBlock => block !== null);
    // Keep the authoritative full snapshot in the engine/projection. The session ref only
    // carries the new revision here; copying `model.blocks` for every keystroke would turn a
    // block-local ChangeSet into an O(document-size) allocation.
    return {
      snapshot: {
        ...current,
        artifact: {
          ...current.artifact,
          revision: change.revision,
        },
      },
      changedBlocks,
      previousBlocks,
    };
  }, [projection]);

  const dispatchCommands = useCallback((commands: DocumentCommand[], queue = true): boolean => {
    const engine = engineRef.current;
    if (!engine || commands.length === 0) return false;
    const previous = snapshotRef.current;
    try {
      const change = engine.dispatch({ baseRevision: engine.revision(), commands: commands });
      const projectionChange = readChangeIntoSnapshot(engine, change);
      const next = projectionChange.snapshot;
      if (change.structureChanged) {
        projection.applySnapshot(next, [...change.changedBlocks, ...change.changedContainers]);
      } else {
        projection.applyChange({ revision: change.revision, blocks: projectionChange.changedBlocks });
      }
      wordCountRef.current = nextWordCount(
        wordCountRef.current,
        previous,
        next,
        projectionChange.previousBlocks,
        projectionChange.changedBlocks,
        change.structureChanged,
      );
      snapshotRef.current = next;
      // Structural changes need a new snapshot for root/page consumers. Block-local edits are
      // delivered through BlockProjectionStore and must not fan out a full document render.
      if (change.structureChanged) setSnapshot(next);
      if (queue) {
        outbox.enqueue({
          artifactId: documentId,
          baseRevision: baseRevisionRef.current,
          sequence: ++sequenceRef.current,
          commands,
          coalesceKey: autosaveCoalesceKey(commands),
        });
      }
      syncState(next, {
        dirty: queue || !outbox.isEmpty,
        error: null,
        canUndo: engine.canUndo(),
        canRedo: engine.canRedo(),
      });
      if (queue) {
        const now = Date.now();
        firstPendingAtRef.current ??= now;
        lastEditAtRef.current = now;
        scheduleAutosave();
      }
      return true;
    } catch (error) {
      reportError(error);
      return false;
    }
  }, [documentId, outbox, projection, readChangeIntoSnapshot, reportError, scheduleAutosave, syncState]);

  const dispatchCommand = useCallback((command: DocumentCommand, queue = true): boolean => {
    return dispatchCommands([command], queue);
  }, [dispatchCommands]);

  const flush = useCallback(async () => {
    if (flightRef.current || outbox.isEmpty) return;
    const transaction = outbox.peekReady();
    if (!transaction) return;
    const engine = engineRef.current;
    const commitApplier = commitApplierRef.current;
    if (!engine || !commitApplier) return;
    if (!outbox.beginAttempt(transaction.envelope.transactionId)) return;
    flightRef.current = true;
    setState((current) => ({ ...current, saving: true, error: null }));
    try {
      const result = await api.submitTransaction(documentId, transaction.envelope);
      const committedBase = committedSnapshotRef.current;
      if (!committedBase) throw new Error("缺少已提交文档快照");
      const remaining = outbox.entries().slice(1);
      const applied = commitApplier.applyAcknowledgement({
        committedSnapshot: committedBase,
        transaction,
        revision: result.revision,
        invalidation: result.invalidation,
      }, remaining);
      committedSnapshotRef.current = applied.committedSnapshot;
      outbox.acknowledge(transaction.envelope.transactionId);
      baseRevisionRef.current = result.revision;
      outbox.retarget(result.revision);
      const normalizedLocal = applied.localSnapshot;
      replaceEngine(normalizedLocal);
      wordCountRef.current = normalizedLocal.artifact.payload.kind === "document"
        ? countWords(normalizedLocal.artifact.payload.data)
        : 0;
      snapshotRef.current = normalizedLocal;
      projection.applySnapshot(normalizedLocal, [
        ...applied.localChangedBlockIds,
        ...applied.localChangedContainerIds,
      ]);
      setSnapshot(normalizedLocal);
      syncState(normalizedLocal, {
        dirty: !outbox.isEmpty,
        saving: false,
        canUndo: result.canUndo,
        canRedo: result.canRedo,
      });
    } catch (error) {
      if (isVersionConflict(error)) {
        try {
          const latest = await api.getArtifact(documentId);
          if (latest.artifact.kind !== "document") throw new Error("当前文档不是 Document Artifact");
          const conflictRebaser = conflictRebaserRef.current;
          if (!conflictRebaser) throw new Error("ConflictRebaser 尚未加载");
          const rebased = conflictRebaser.rebase(latest, outbox.entries());
          committedSnapshotRef.current = rebased.committedSnapshot;
          baseRevisionRef.current = latest.artifact.revision;
          outbox.retarget(latest.artifact.revision);
          replaceEngine(rebased.localSnapshot);
          snapshotRef.current = rebased.localSnapshot;
          wordCountRef.current = rebased.localSnapshot.artifact.payload.kind === "document"
            ? countWords(rebased.localSnapshot.artifact.payload.data)
            : 0;
          // A 409 response does not carry the remote invalidation set. The latest snapshot may
          // contain arbitrary changes made by another client, so a full projection refresh is
          // the only safe boundary here. Normal acknowledgements above stay block-local.
          if (rebased.requiresFullProjectionRefresh) projection.replaceSnapshot(rebased.localSnapshot);
          setSnapshot(rebased.localSnapshot);
          outbox.retry(transaction.envelope.transactionId);
          syncState(rebased.localSnapshot, { dirty: !outbox.isEmpty, error: null });
        } catch (rebaseError) {
          outbox.markRetry(transaction.envelope.transactionId, rebaseError);
          reportError(rebaseError);
        }
      } else {
        const retry = isRetryableError(error)
          ? outbox.markRetry(transaction.envelope.transactionId, error)
          : (outbox.markFailed(transaction.envelope.transactionId, error) ? { state: "failed" as const } : null);
        reportError(error);
        if (retry?.state === "failed") {
          setState((current) => ({ ...current, error: `保存失败，已停止重试：${current.error ?? "未知错误"}` }));
        }
      }
    } finally {
      flightRef.current = false;
      setState((current) => ({ ...current, saving: false, dirty: !outbox.isEmpty }));
      const delay = outbox.nextAttemptDelay();
      if (delay === null) {
        firstPendingAtRef.current = null;
        lastEditAtRef.current = null;
      } else if (delay > 0) {
        scheduleRetry(delay);
      } else {
        // A user may have continued typing while this request was in flight.
        // Respect the same idle/max-latency policy instead of immediately
        // sending the next local draft in a request storm.
        firstPendingAtRef.current = lastEditAtRef.current ?? Date.now();
        scheduleAutosave();
      }
    }
  }, [documentId, outbox, projection, replaceEngine, reportError, scheduleAutosave, scheduleRetry, syncState]);
  flushRef.current = flush;

  const flushAll = useCallback(async () => {
    const deadline = Date.now() + 30_000;
    while (!outbox.isEmpty || flightRef.current) {
      if (Date.now() >= deadline) {
        reportError("保存重试超时，请稍后重试");
        return;
      }
      if (!flightRef.current) {
        const pendingBefore = outbox.size;
        await flushRef.current();
        if (!flightRef.current && outbox.size >= pendingBefore) {
          if (outbox.blocked) return;
          const delay = outbox.nextAttemptDelay();
          if (delay === null) break;
          await new Promise((resolve) => setTimeout(resolve, Math.max(25, delay)));
        }
      }
      if (flightRef.current) await new Promise((resolve) => setTimeout(resolve, 20));
    }
  }, [outbox]);

  const updateContent = useCallback((id: string, content: RichText) => {
    dispatchCommand({ type: "replaceBlockText", blockId: id, content });
  }, [dispatchCommand]);

  const convertBlock = useCallback((id: string, kind: DocumentBlockKind) => {
    if (kind.type === "link") return;
    dispatchCommand({ type: "convertBlock", blockId: id, kind });
  }, [dispatchCommand]);

  const setBlockPresentation = useCallback((id: string, presentation: Record<string, unknown | null>) => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document") return;
    const block = projection.getBlock(id);
    if (!block) return;
    type PresentationPatch = Extract<DocumentCommand, { type: "setBlockPresentation" }>["patch"];
    const patch = {} as PresentationPatch;
    for (const [key, value] of Object.entries(presentation)) {
      switch (key) {
        case "align":
          if (value !== null && !["left", "center", "right", "justify"].includes(String(value))) {
            reportError("无效的段落对齐方式");
            return;
          }
          patch.align = value as PresentationPatch["align"];
          break;
        case "listType": {
          if (value !== null && value !== "bullet" && value !== "ordered") {
            reportError("无效的列表类型");
            return;
          }
          const level = presentation.listLevel;
          if (level !== undefined && level !== null && (typeof level !== "number" || !Number.isInteger(level) || level < 0 || level > 9)) {
            reportError("列表级别必须在 0 到 9 之间");
            return;
          }
          patch.list = value === null
            ? null
            : { kind: value, level: typeof level === "number" ? level : block.presentation.list?.level ?? 0 };
          break;
        }
        case "listLevel":
          // listType owns the canonical wire representation. A level without
          // a list kind is ignored rather than producing an incomplete patch.
          break;
        case "indentLevel":
          if (value !== null && (typeof value !== "number" || !Number.isFinite(value))) {
            reportError(`${key} 必须是有限数字`);
            return;
          }
          patch.indentStart = value as PresentationPatch["indentStart"];
          break;
        case "indentRight":
          if (value !== null && (typeof value !== "number" || !Number.isFinite(value))) {
            reportError(`${key} 必须是有限数字`);
            return;
          }
          patch.indentEnd = value as PresentationPatch["indentEnd"];
          break;
        case "spacingBefore":
        case "spacingAfter":
        case "lineHeight":
          if (value !== null && (typeof value !== "number" || !Number.isFinite(value))) {
            reportError(`${key} 必须是有限数字`);
            return;
          }
          patch[key] = value as never;
          break;
        default:
          reportError(`不支持的段落属性：${key}`);
          return;
      }
    }
    if (Object.keys(patch).length > 0) dispatchCommand({ type: "setBlockPresentation", blockId: id, patch });
  }, [dispatchCommand, reportError]);

  const insertRelative = useCallback((id: string, offset: 0 | 1, kind: DocumentBlockKind = { type: "paragraph" }, presentation: Extract<DocumentCommand, { type: "setBlockPresentation" }>['patch'] = {}) => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document") return null;
    const location = projection.findLocation(id);
    if (!location) return null;
    const block: DocumentBlock = {
      id: `block-${randomId()}`,
      kind,
      presentation: presentationFromCommandPatch(presentation),
      content: kind.type === "divider" ? null : { text: "", runs: [] },
      children: [],
      data: kind.type === "code"
        ? { type: "code", data: defaultCodeBlockConfig() }
        : kind.type === "todo"
          ? { type: "todo", data: { checked: false } }
          : { type: "none" },
    };
    return dispatchCommand({ type: "insertBlock", block, parentId: location.parentId, index: location.index + offset }) ? block.id : null;
  }, [dispatchCommand]);

  const insertAfter = useCallback((id: string, kind: DocumentBlockKind = { type: "paragraph" }, presentation: Extract<DocumentCommand, { type: "setBlockPresentation" }>['patch'] = {}) => (
    insertRelative(id, 1, kind, presentation)
  ), [insertRelative]);

  const insertBefore = useCallback((id: string, kind: DocumentBlockKind = { type: "paragraph" }, presentation: Extract<DocumentCommand, { type: "setBlockPresentation" }>['patch'] = {}) => (
    insertRelative(id, 0, kind, presentation)
  ), [insertRelative]);

  const insertPastedImage = useCallback(async (id: string, file: File): Promise<string | null> => {
    const initial = snapshotRef.current;
    if (!initial || initial.artifact.payload.kind !== "document" || !file.type.startsWith("image/")) return null;
    const source = projection.getBlock(id);
    const initialLocation = projection.findLocation(id);
    if (!source || !initialLocation) return null;

    try {
      const asset = await api.uploadAsset(documentId, file, file.name || "pasted-image.png");
      // The document may have changed while the binary was uploading. Resolve
      // the stable block id again instead of applying a stale array index.
      const latestSource = projection.getBlock(id);
      const location = projection.findLocation(id);
      if (!latestSource || !location) {
        await api.deleteAsset(documentId, asset.assetId).catch(() => undefined);
        return null;
      }

      const imageBlockId = `block-${randomId()}`;
      const commands = buildPastedImageInsertCommands({
        source: latestSource,
        parentId: location.parentId ?? null,
        index: location.index,
        imageBlockId,
        assetId: asset.assetId,
        alt: file.name || "粘贴图片",
      });
      if (dispatchCommands(commands)) return imageBlockId;
      await api.deleteAsset(documentId, asset.assetId).catch(() => undefined);
      return null;
    } catch (error) {
      reportError(error);
      return null;
    }
  }, [dispatchCommands, documentId, projection, reportError]);

  const setImageConfig = useCallback((id: string, patch: Extract<DocumentCommand, { type: "setImageConfig" }>['patch']) => {
    if (Object.keys(patch).length > 0) dispatchCommand({ type: "setImageConfig", blockId: id, patch });
  }, [dispatchCommand]);

  const replaceImageAsset = useCallback(async (id: string, file: File): Promise<boolean> => {
    if (!file.type.startsWith("image/")) {
      reportError("只能替换为图片文件");
      return false;
    }
    const source = projection.getBlock(id);
    if (!source || source.data.type !== "image") return false;
    try {
      const asset = await api.uploadAsset(documentId, file, file.name || "compressed-image.webp");
      const latest = projection.getBlock(id);
      if (!latest || latest.data.type !== "image") {
        await api.deleteAsset(documentId, asset.assetId).catch(() => undefined);
        return false;
      }
      const originalAssetId = latest.data.data.originalAssetId ?? latest.data.data.assetId;
      const changed = dispatchCommand({
        type: "setImageConfig",
        blockId: id,
        patch: { assetId: asset.assetId, originalAssetId },
      });
      if (!changed) await api.deleteAsset(documentId, asset.assetId).catch(() => undefined);
      return changed;
    } catch (error) {
      reportError(error);
      return false;
    }
  }, [dispatchCommand, documentId, projection, reportError]);

  const assetUrl = useCallback((assetId: string) => api.assetUrl(documentId, assetId), [documentId]);

  const insertTableAfter = useCallback((id: string, rows = 2, columns = 2) => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document") return null;
    const location = projection.findLocation(id);
    if (!location) return null;
    const safeRows = Math.max(1, Math.min(20, Math.floor(rows)));
    const safeColumns = Math.max(1, Math.min(12, Math.floor(columns)));
    const columnIds = Array.from({ length: safeColumns }, () => `column-${randomId()}`);
    const data: BlockData = {
      type: "table",
      data: {
        columns: columnIds.map((columnId) => ({ id: columnId, width: null })),
        rows: Array.from({ length: safeRows }, () => ({
          id: `row-${randomId()}`,
          height: null,
          cells: columnIds.map(() => ({ id: `cell-${randomId()}`, content: { text: "", runs: [] } })),
        })),
        mergedRanges: [],
      },
    };
    const block: DocumentBlock = {
      id: `block-${randomId()}`,
      kind: { type: "table" },
      presentation: defaultBlockPresentation(),
      content: null,
      children: [],
      data,
    };
    return dispatchCommand({ type: "insertBlock", block, parentId: location.parentId, index: location.index + 1 }) ? block.id : null;
  }, [dispatchCommand]);

  const updateTableCell = useCallback((blockId: string, rowId: string, cellId: string, content: RichText) => {
    dispatchCommand({ type: "replaceTableCellText", blockId, rowId, cellId, content });
  }, [dispatchCommand]);

  const patchTableCellInlineRange = useCallback((
    blockId: string,
    rowId: string,
    cellId: string,
    range: TextRange,
    patch: InlineStylePatch,
  ) => {
    dispatchCommand({ type: "patchTableCellInlineRange", blockId, rowId, cellId, range, patch });
  }, [dispatchCommand]);

  const formatTableCells = useCallback((
    blockId: string,
    selection: Extract<DocumentCommand, { type: "formatTableCells" }>["selection"],
    patch: Extract<DocumentCommand, { type: "formatTableCells" }>["patch"],
  ) => {
    dispatchCommand({ type: "formatTableCells", blockId, selection, patch });
  }, [dispatchCommand]);

  const setTableBorders = useCallback((
    blockId: string,
    selection: Extract<DocumentCommand, { type: "setTableBorders" }>["selection"],
    patch: TableBorderPatch,
  ) => {
    dispatchCommand({ type: "setTableBorders", blockId, selection, patch });
  }, [dispatchCommand]);

  const applyTableBorderPreset = useCallback((
    blockId: string,
    selection: Extract<DocumentCommand, { type: "applyTableBorderPreset" }>['selection'],
    preset: TableBorderPreset,
    border?: TableBorder,
  ) => {
    dispatchCommand({ type: "applyTableBorderPreset", blockId, selection, preset, border });
  }, [dispatchCommand]);

  const setTodoChecked = useCallback((id: string, checked: boolean) => {
    dispatchCommand({ type: "setTodoChecked", blockId: id, checked });
  }, [dispatchCommand]);

  const convertToLink = useCallback((id: string, url: string) => {
    dispatchCommand({ type: "convertToLink", blockId: id, url });
  }, [dispatchCommand]);

  const setLinkTarget = useCallback((id: string, url: string) => {
    dispatchCommand({ type: "setLinkTarget", blockId: id, url });
  }, [dispatchCommand]);

  const setCodeConfig = useCallback((id: string, config: CodeBlockConfig) => {
    dispatchCommand({ type: "setCodeConfig", blockId: id, config });
  }, [dispatchCommand]);

  const insertTableRow = useCallback((blockId: string, boundaryIndex?: number) => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document") return;
    const block = projection.getBlock(blockId);
    if (!block || block.data.type !== "table" || block.data.data.rows.length >= 100) return;
    const index = Math.max(0, Math.min(boundaryIndex ?? block.data.data.rows.length, block.data.data.rows.length));
    dispatchCommand({
      type: "insertTableRow",
      blockId,
      index,
      row: {
        id: `row-${randomId()}`,
        height: null,
        cells: block.data.data.columns.map(() => ({ id: `cell-${randomId()}`, content: { text: "", runs: [] } })),
      },
    });
  }, [dispatchCommand]);

  const insertTableColumn = useCallback((blockId: string, boundaryIndex?: number) => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document") return;
    const block = projection.getBlock(blockId);
    if (!block || block.data.type !== "table" || block.data.data.columns.length >= 20) return;
    const index = Math.max(0, Math.min(boundaryIndex ?? block.data.data.columns.length, block.data.data.columns.length));
    dispatchCommand({
      type: "insertTableColumn",
      blockId,
      index,
      column: { id: `column-${randomId()}`, width: null },
      cells: block.data.data.rows.map(() => ({ id: `cell-${randomId()}`, content: { text: "", runs: [] } })),
    });
  }, [dispatchCommand]);

  const deleteTableRow = useCallback((blockId: string, rowIndex: number) => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document") return;
    const block = projection.getBlock(blockId);
    if (!block || block.data.type !== "table" || block.data.data.rows.length <= 1) return;
    const index = Math.max(0, Math.min(Math.floor(rowIndex), block.data.data.rows.length - 1));
    dispatchCommand({ type: "deleteTableRow", blockId, rowId: block.data.data.rows[index].id });
  }, [dispatchCommand]);

  const deleteTableColumn = useCallback((blockId: string, columnIndex: number) => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document") return;
    const block = projection.getBlock(blockId);
    if (!block || block.data.type !== "table" || block.data.data.columns.length <= 1) return;
    const index = Math.max(0, Math.min(Math.floor(columnIndex), block.data.data.columns.length - 1));
    dispatchCommand({ type: "deleteTableColumn", blockId, columnId: block.data.data.columns[index].id });
  }, [dispatchCommand]);

  const setTableColumnWidth = useCallback((blockId: string, columnId: string, width: number) => {
    dispatchCommand({ type: "setTableColumnWidth", blockId, columnId, width });
  }, [dispatchCommand]);

  const setTableColumnWidths = useCallback((blockId: string, leftColumnId: string, leftWidth: number, rightColumnId: string, rightWidth: number) => {
    dispatchCommands([
      { type: "setTableColumnWidth", blockId, columnId: leftColumnId, width: leftWidth },
      { type: "setTableColumnWidth", blockId, columnId: rightColumnId, width: rightWidth },
    ]);
  }, [dispatchCommands]);

  const setTableRowHeight = useCallback((blockId: string, rowId: string, height: number) => {
    dispatchCommand({ type: "setTableRowHeight", blockId, rowId, height });
  }, [dispatchCommand]);

  const mergeTableCells = useCallback((blockId: string, range: Extract<DocumentCommand, { type: "mergeTableCells" }>['range']) => {
    dispatchCommand({ type: "mergeTableCells", blockId, range });
  }, [dispatchCommand]);

  const splitTableCells = useCallback((blockId: string, range: Extract<DocumentCommand, { type: "splitTableCells" }>['range']) => {
    dispatchCommand({ type: "splitTableCells", blockId, range });
  }, [dispatchCommand]);

  const setPageSetup = useCallback((pageSetup: ArtifactPageSetup | null) => {
    dispatchCommand({ type: "setPageSetup", pageSetup });
  }, [dispatchCommand]);

  const upsertSection = useCallback((section: DocumentSection, index: number) => {
    dispatchCommand({ type: "upsertSection", section, index });
  }, [dispatchCommand]);

  const deleteSection = useCallback((sectionId: string) => {
    dispatchCommand({ type: "deleteSection", sectionId });
  }, [dispatchCommand]);

  const upsertNote = useCallback((noteKind: "footnote" | "endnote", note: DocumentNote) => {
    dispatchCommand({ type: "upsertNote", noteKind, note });
  }, [dispatchCommand]);

  const deleteNote = useCallback((noteKind: "footnote" | "endnote", noteId: string) => {
    dispatchCommand({ type: "deleteNote", noteKind, noteId });
  }, [dispatchCommand]);

  const deleteBlock = useCallback((id: string) => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document") return;
    const root = projection.getStructureSnapshot().root;
    if (root.length === 1 && root[0] === id) return;
    dispatchCommand({ type: "deleteBlock", blockId: id });
  }, [dispatchCommand]);

  const deleteTextSelection = useCallback((): string | null => {
    const current = snapshotRef.current;
    const ranges = readBlockTextSelection();
    if (!current || current.artifact.payload.kind !== "document" || ranges.length === 0) return null;
    const blocks = new Map(ranges.map(({ blockId }) => [blockId, projection.getBlock(blockId)] as const));
    const first = ranges[0];
    const firstBlock = blocks.get(first.blockId);
    if (!firstBlock?.content) return null;
    if (ranges.length === 1) {
      const length = Array.from(firstBlock.content.text).length;
      const nextContent = concatRichText(
        sliceRichText(firstBlock.content, 0, first.start),
        sliceRichText(firstBlock.content, first.end, length),
      );
      if (nextContent.text === firstBlock.content.text && JSON.stringify(nextContent.runs) === JSON.stringify(firstBlock.content.runs)) return null;
      return dispatchCommands([{ type: "replaceBlockText", blockId: first.blockId, content: nextContent }]) ? first.blockId : null;
    }

    const last = ranges[ranges.length - 1];
    const lastBlock = blocks.get(last.blockId);
    if (!lastBlock?.content) return null;
    const firstLength = Array.from(firstBlock.content.text).length;
    const lastLength = Array.from(lastBlock.content.text).length;
    // Keep the unselected prefix and suffix in the first block. Removing the
    // intervening blocks as one command batch preserves undo/autosave atomicity.
    const merged = concatRichText(
      sliceRichText(firstBlock.content, 0, first.start),
      sliceRichText(lastBlock.content, last.end, lastLength),
    );
    const commands: DocumentCommand[] = [{ type: "replaceBlockText", blockId: first.blockId, content: merged }];
    const seen = new Set<string>([first.blockId]);
    for (const range of ranges.slice(1)) {
      if (seen.has(range.blockId)) continue;
      seen.add(range.blockId);
      commands.push({ type: "deleteBlock", blockId: range.blockId });
    }
    // A selection may end at the first block's end and begin at the last
    // block's start; this still intentionally leaves one editable block.
    if (firstLength === first.start && merged.text.length === 0 && commands.length === 1) return null;
    return dispatchCommands(commands) ? first.blockId : null;
  }, [dispatchCommands]);

  const clearDocument = useCallback(() => {
    const current = snapshotRef.current;
    if (!current || current.artifact.payload.kind !== "document") return;
    const keepId = projection.getStructureSnapshot().root[0];
    if (!keepId) return;
    const keep = projection.getBlock(keepId);
    if (!keep) return;

    // A document must always retain one root block. Clear that block in place and
    // remove every other root/child subtree in the same engine transaction so
    // deletion, undo/redo and autosave stay atomic.
    const commands: DocumentCommand[] = [{ type: "resetBlock", blockId: keepId }];
    for (const blockId of projection.getStructureSnapshot().root.slice(1)) commands.push({ type: "deleteBlock", blockId });
    for (const blockId of keep.children) commands.push({ type: "deleteBlock", blockId });
    dispatchCommands(commands);
  }, [dispatchCommands]);

  const toggleMark = useCallback((mark: "bold" | "italic" | "underline" | "strikethrough") => {
    const current = snapshotRef.current;
    const ranges = readBlockTextSelection();
    if (!current || current.artifact.payload.kind !== "document" || ranges.length === 0) return;
    const blocks = new Map(ranges.map(({ blockId }) => [blockId, projection.getBlock(blockId)] as const));
    const enabled = ranges.some(({ blockId, start, end }) => {
      const block = blocks.get(blockId);
      return Boolean(block?.content && !isRichTextRangeMarked(block.content, start, end, mark));
    });
    const commands = ranges.flatMap(({ blockId, start, end }) => {
      const block = blocks.get(blockId);
      return block?.content
        ? [{
            type: "patchInlineRange" as const,
            blockId,
            range: { start, end },
            patch: { [mark]: enabled },
          }]
        : [];
    });
    if (commands.length > 0 && dispatchCommands(commands)) requestAnimationFrame(() => restoreBlockTextSelection(ranges));
  }, [dispatchCommands]);

  const setInlineAttrs = useCallback((attrs: Record<string, unknown | null>) => {
    const current = snapshotRef.current;
    const ranges = readBlockTextSelection();
    if (!current || current.artifact.payload.kind !== "document" || ranges.length === 0) return;
    const patch = toInlineStylePatch(attrs);
    if (!patch) {
      reportError("不支持的行内格式属性");
      return;
    }
    const blocks = new Map(ranges.map(({ blockId }) => [blockId, projection.getBlock(blockId)] as const));
    const commands = ranges.flatMap(({ blockId, start, end }) => {
      const block = blocks.get(blockId);
      return block?.content
        ? [{ type: "patchInlineRange" as const, blockId, range: { start, end }, patch }]
        : [];
    });
    if (commands.length > 0 && dispatchCommands(commands)) requestAnimationFrame(() => restoreBlockTextSelection(ranges));
  }, [dispatchCommands, reportError]);

  const captureInlineAttrs = useCallback((): Record<string, unknown> | null => {
    const current = snapshotRef.current;
    const range = readBlockTextSelection()[0];
    if (!current || current.artifact.payload.kind !== "document" || !range) return null;
    const block = projection.getBlock(range.blockId);
    if (!block?.content) return null;
    const chars = Array.from(block.content.text);
    const runs = block.content.runs.length > 0 ? block.content.runs : [{ start: 0, end: chars.length, style: emptyInlineStyle() }];
    return { ...(runs.find((run) => range.start >= run.start && range.start < run.end)?.style ?? emptyInlineStyle()) };
  }, []);

  /** Adjust the selected runs by one readable step without mutating the DOM. */
  const adjustFontSize = useCallback((delta: -1 | 1) => {
    const current = snapshotRef.current;
    const ranges = readBlockTextSelection();
    if (!current || current.artifact.payload.kind !== "document" || ranges.length === 0) return;
    const blocks = new Map(ranges.map(({ blockId }) => [blockId, projection.getBlock(blockId)] as const));
    const commands = ranges.flatMap(({ blockId, start, end }) => {
      const block = blocks.get(blockId);
      return block?.content
        ? [{
            type: "patchInlineRange" as const,
            blockId,
            range: { start, end },
            patch: { fontSize: nextInlineFontSize(block.content, start, delta) },
          }]
        : [];
    });
    if (commands.length > 0 && dispatchCommands(commands)) requestAnimationFrame(() => restoreBlockTextSelection(ranges));
  }, [dispatchCommands]);

  const submitHistory = useCallback(async (action: "undo" | "redo") => {
    if (!outbox.isEmpty) {
      await flushAll();
      // Never undo a server revision while unsaved local commands remain queued.
      // Otherwise the history intent would target an older server state and the local
      // queue could be silently reordered around it.
      if (!outbox.isEmpty || flightRef.current) return;
    }
    const waitStarted = Date.now();
    while (flightRef.current && Date.now() - waitStarted < 10_000) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (flightRef.current) {
      reportError("保存仍在进行，暂时无法撤销或重做");
      return;
    }
    const baseRevision = baseRevisionRef.current;
    setState((current) => ({ ...current, saving: true, error: null }));
    try {
  const result = await historyAdapter.submit(documentId, action, baseRevision);
      await load(result.invalidation);
      // The reload rebuilds block DOM, which drops focus to <body>; without
      // restoring it the very next undo/redo shortcut is silently swallowed.
      requestAnimationFrame(() =>
        document.querySelector<HTMLElement>('.block-row__content[contenteditable="true"]')?.focus(),
      );
      setState((current) => ({ ...current, canUndo: result.canUndo, canRedo: result.canRedo, saving: false }));
    } catch (error) {
      reportError(error);
      await load();
    } finally {
      setState((current) => ({ ...current, saving: false }));
    }
  }, [documentId, flushAll, historyAdapter, load, outbox, reportError]);

  const undo = useCallback(() => { void submitHistory("undo"); }, [submitHistory]);
  const redo = useCallback(() => { void submitHistory("redo"); }, [submitHistory]);

  const save = useCallback(async () => {
    clearAutosaveTimer();
    if (retryTimerRef.current) clearTimeout(retryTimerRef.current);
    retryTimerRef.current = null;
    await flushAll();
  }, [clearAutosaveTimer, flushAll]);

  const setActiveBlock = useCallback((id: string | null) => {
    setState((current) => ({ ...current, activeBlockId: id }));
  }, []);

  const findText = useCallback((query: string, options: DocumentSearchOptions = {}) => (
    engineRef.current?.findText(query, options) ?? []
  ), []);

  const tableOfContents = useCallback(() => engineRef.current?.tableOfContents() ?? [], []);

  const printProjection = useCallback(() => engineRef.current?.printProjection() ?? null, []);

  const replaceAllText = useCallback((
    query: string,
    replacement: string,
    options: DocumentSearchOptions = {},
  ) => dispatchCommand({ type: "replaceAllText", query, replacement, options }), [dispatchCommand]);

  const replaceTextMatch = useCallback((
    match: DocumentSearchMatch,
    query: string,
    replacement: string,
    options: DocumentSearchOptions = {},
  ) => dispatchCommand({
    type: "replaceTextMatch",
    target: match.target,
    range: { start: match.start, end: match.end },
    query,
    replacement,
    options,
  }), [dispatchCommand]);

  return {
    snapshot,
    projection,
    state,
    setActiveBlock,
    updateContent,
    findText,
    tableOfContents,
    printProjection,
    replaceAllText,
    replaceTextMatch,
    convertBlock,
    setBlockPresentation,
    insertAfter,
    insertBefore,
    insertPastedImage,
    setImageConfig,
    replaceImageAsset,
    assetUrl,
    insertTableAfter,
    updateTableCell,
    patchTableCellInlineRange,
    formatTableCells,
    setTableBorders,
    applyTableBorderPreset,
    setTodoChecked,
    convertToLink,
    setLinkTarget,
    setCodeConfig,
    insertTableRow,
    insertTableColumn,
    deleteTableRow,
    deleteTableColumn,
    setTableColumnWidth,
    setTableColumnWidths,
    setTableRowHeight,
    mergeTableCells,
    splitTableCells,
    setPageSetup,
    upsertSection,
    deleteSection,
    upsertNote,
    deleteNote,
    deleteBlock,
    deleteTextSelection,
    clearDocument,
    toggleMark,
    setInlineAttrs,
    captureInlineAttrs,
    adjustFontSize,
    undo,
    redo,
    save,
    reload: load,
    reportError,
  };
}

/**
 * Build the canonical structural transaction for a pasted image. An empty
 * text/list item is replaced in place; otherwise the image becomes the next
 * sibling. Presentation is copied so list kind/level and paragraph indents
 * remain semantic document state rather than renderer-only CSS.
 */
export function buildPastedImageInsertCommands({
  source,
  parentId,
  index,
  imageBlockId,
  assetId,
  alt,
}: {
  source: DocumentBlock;
  parentId: string | null;
  index: number;
  imageBlockId: string;
  assetId: string;
  alt: string;
}): DocumentCommand[] {
  const imageBlock: DocumentBlock = {
    id: imageBlockId,
    kind: { type: "image" },
    presentation: {
      ...source.presentation,
      list: source.presentation.list ? { ...source.presentation.list } : null,
      namedStyle: source.presentation.namedStyle ? { ...source.presentation.namedStyle } : null,
    },
    content: null,
    children: [],
    data: {
      type: "image",
      data: {
        assetId,
        alt,
        originalAssetId: null,
        transform: defaultImageTransform(),
        size: { width: null, height: null, lockAspectRatio: true },
        placement: { offsetX: 0, offsetY: 0 },
        caption: "",
      },
    },
  };
  const replaceEmptyTextItem = source.data.type === "none"
    && (source.kind.type === "paragraph" || source.kind.type === "heading")
    && !source.content?.text.trim();
  const commands: DocumentCommand[] = [{
    type: "insertBlock",
    block: imageBlock,
    parentId,
    index: replaceEmptyTextItem ? index : index + 1,
  }];
  if (replaceEmptyTextItem) commands.push({ type: "deleteBlock", blockId: source.id });
  return commands;
}

function defaultBlockPresentation(): BlockPresentation {
  return {
    align: "left",
    list: null,
    indentStart: 0,
    indentEnd: 0,
    spacingBefore: 0,
    spacingAfter: 0,
    lineHeight: 1,
    namedStyle: null,
  };
}

/** Translate the editor's semantic presentation command into its canonical block projection. */
function presentationFromCommandPatch(
  patch: Extract<DocumentCommand, { type: "setBlockPresentation" }>['patch'],
): BlockPresentation {
  const next = defaultBlockPresentation();
  if (patch.align !== undefined && patch.align !== null) next.align = patch.align;
  if (patch.list !== undefined && patch.list !== null) {
    next.list = patch.list;
  }
  if (patch.indentStart !== undefined && patch.indentStart !== null) next.indentStart = patch.indentStart;
  if (patch.indentEnd !== undefined && patch.indentEnd !== null) next.indentEnd = patch.indentEnd;
  if (patch.spacingBefore !== undefined && patch.spacingBefore !== null) next.spacingBefore = patch.spacingBefore;
  if (patch.spacingAfter !== undefined && patch.spacingAfter !== null) next.spacingAfter = patch.spacingAfter;
  if (patch.lineHeight !== undefined && patch.lineHeight !== null) next.lineHeight = patch.lineHeight;
  return next;
}

function toInlineStylePatch(attrs: Record<string, unknown | null>): InlineStylePatch | null {
  const patch: InlineStylePatch = {};
  for (const [key, value] of Object.entries(attrs)) {
    switch (key) {
      case "bold":
      case "italic":
      case "underline":
      case "strikethrough":
        if (value !== null && typeof value !== "boolean") return null;
        patch[key] = value;
        break;
      case "fontFamily":
      case "color":
      case "highlight":
        if (value !== null && typeof value !== "string") return null;
        patch[key] = value;
        break;
      case "fontSize":
        if (value !== null && (typeof value !== "number" || !Number.isFinite(value))) return null;
        patch.fontSize = value;
        break;
      default:
        // Future/extension attrs remain opaque in the snapshot but are not
        // allowed to leak into this typed command. The painter simply copies
        // the supported presentation subset.
        continue;
    }
  }
  return Object.keys(patch).length > 0 ? patch : null;
}

function nextInlineFontSize(content: RichText, start: number, delta: -1 | 1): number {
  const chars = Array.from(content.text);
  const runs = content.runs.length > 0 ? content.runs : [{ start: 0, end: chars.length, style: emptyInlineStyle() }];
  const style = runs.find((run) => start >= run.start && start < run.end)?.style ?? emptyInlineStyle();
  const current = typeof style.fontSize === "number" && Number.isFinite(style.fontSize) ? style.fontSize : 16;
  return Math.min(72, Math.max(8, current + delta));
}

export function toggleRichTextMark(
  content: RichText,
  start: number,
  end: number,
  mark: "bold" | "italic" | "underline" | "strikethrough",
  enabledOverride?: boolean,
): RichText {
  const chars = Array.from(content.text);
  const runs = content.runs.length > 0 ? content.runs : [{ start: 0, end: chars.length, style: emptyInlineStyle() }];
  const stylesByChar = chars.map((_, index) => ({ ...(runs.find((run) => index >= run.start && index < run.end)?.style ?? emptyInlineStyle()) }));
  const selected = stylesByChar.slice(start, end);
  const enabled = enabledOverride ?? (selected.length > 0 && !selected.every((style) => style[mark] === true));
  for (let index = Math.max(0, start); index < Math.min(chars.length, end); index += 1) {
    stylesByChar[index][mark] = enabled;
  }
  return { text: content.text, runs: compactRuns(stylesByChar) };
}

function isRichTextRangeMarked(
  content: RichText,
  start: number,
  end: number,
  mark: "bold" | "italic" | "underline" | "strikethrough",
): boolean {
  if (start >= end) return false;
  const chars = Array.from(content.text);
  const runs = content.runs.length > 0 ? content.runs : [{ start: 0, end: chars.length, style: emptyInlineStyle() }];
  return chars.slice(start, end).every((_, index) => {
    const offset = start + index;
    return runs.find((run) => offset >= run.start && offset < run.end)?.style[mark] === true;
  });
}

export function setRichTextAttrs(content: RichText, start: number, end: number, patch: Record<string, unknown | null>): RichText {
  const chars = Array.from(content.text);
  const runs = content.runs.length > 0 ? content.runs : [{ start: 0, end: chars.length, style: emptyInlineStyle() }];
  const stylesByChar = chars.map((_, index) => ({ ...(runs.find((run) => index >= run.start && index < run.end)?.style ?? emptyInlineStyle()) }));
  for (let index = Math.max(0, start); index < Math.min(chars.length, end); index += 1) {
    for (const [key, value] of Object.entries(patch)) {
      if (key in stylesByChar[index]) {
        if (key === "bold" || key === "italic" || key === "underline" || key === "strikethrough") {
          (stylesByChar[index] as Record<string, unknown>)[key] = value === true;
        } else {
          (stylesByChar[index] as Record<string, unknown>)[key] = value;
        }
      }
    }
  }
  return { text: content.text, runs: compactRuns(stylesByChar) };
}

export function adjustRichTextFontSize(content: RichText, start: number, end: number, delta: -1 | 1): RichText {
  const chars = Array.from(content.text);
  const runs = content.runs.length > 0 ? content.runs : [{ start: 0, end: chars.length, style: emptyInlineStyle() }];
  const firstStyle = runs.find((run) => start >= run.start && start < run.end)?.style ?? emptyInlineStyle();
  const currentSize = typeof firstStyle.fontSize === "number" && Number.isFinite(firstStyle.fontSize) ? firstStyle.fontSize : 16;
  const nextSize = Math.min(72, Math.max(8, currentSize + delta));
  return setRichTextAttrs(content, start, end, { fontSize: nextSize });
}

function compactRuns(stylesByChar: InlineStyle[]): RichText["runs"] {
  const nextRuns: RichText["runs"] = [];
  stylesByChar.forEach((style, index) => {
    const previous = nextRuns[nextRuns.length - 1];
    if (previous && JSON.stringify(previous.style) === JSON.stringify(style)) previous.end = index + 1;
    else nextRuns.push({ start: index, end: index + 1, style });
  });
  return nextRuns;
}

function emptyInlineStyle(): InlineStyle {
  return { bold: false, italic: false, underline: false, strikethrough: false, fontFamily: null, fontSize: null, color: null, highlight: null, verticalAlign: null };
}

function countWords(model: DocumentModel): number {
  return model.blocks.reduce((sum, block) => {
    const contentLength = block.content?.text.trim() ? Array.from(block.content.text.trim()).length : 0;
    const tableLength = block.data.type === "table"
      ? block.data.data.rows.reduce((rowSum, row) => rowSum + row.cells.reduce((cellSum, cell) => cellSum + Array.from(cell.content.text.trim()).length, 0), 0)
      : 0;
    return sum + contentLength + tableLength;
  }, 0);
}

function nextWordCount(
  currentCount: number,
  previous: SnapshotEnvelope | null,
  next: SnapshotEnvelope,
  previousBlocks: DocumentBlock[],
  changedBlocks: DocumentBlock[],
  structureChanged: boolean,
): number {
  const nextModel = next.artifact.payload.kind === "document" ? next.artifact.payload.data : null;
  if (!nextModel) return 0;
  if (structureChanged || !previous || previous.artifact.payload.kind !== "document") {
    // Structural changes can add/remove descendants, so a complete count is intentional here.
    return countWords(nextModel);
  }
  const previousById = new Map(previousBlocks.map((block) => [block.id, block]));
  let delta = 0;
  for (const block of changedBlocks) {
    delta += blockWordCount(block) - blockWordCount(previousById.get(block.id));
  }
  return currentCount + delta;
}

function blockWordCount(block: DocumentBlock | undefined): number {
  if (!block) return 0;
  const contentLength = block.content?.text.trim() ? Array.from(block.content.text.trim()).length : 0;
  const tableLength = block.data.type === "table"
    ? block.data.data.rows.reduce((rowSum, row) => rowSum + row.cells.reduce((cellSum, cell) => cellSum + Array.from(cell.content.text.trim()).length, 0), 0)
    : 0;
  return contentLength + tableLength;
}

/**
 * Only commands whose later value fully supersedes their earlier unsent value
 * are folded. Structural and formatting commands remain individually ordered
 * semantic events, so the outbox never changes document meaning to save I/O.
 */
function autosaveCoalesceKey(commands: readonly DocumentCommand[]): string | null {
  if (commands.length !== 1) return null;
  const [command] = commands;
  switch (command.type) {
    case "replaceBlockText":
      return `block-text:${command.blockId}`;
    case "replaceTableCellText":
      return `table-cell-text:${command.blockId}:${command.rowId}:${command.cellId}`;
    case "setCodeConfig":
      return `code-config:${command.blockId}`;
    default:
      return null;
  }
}

function randomId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function isVersionConflict(error: unknown): boolean {
  return error instanceof ApiRequestError && error.status === 409;
}

function isRetryableError(error: unknown): boolean {
  if (!(error instanceof ApiRequestError)) return true;
  return error.status === 408 || error.status === 425 || error.status === 429 || error.status >= 500;
}
