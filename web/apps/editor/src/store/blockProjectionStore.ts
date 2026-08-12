import { useSyncExternalStore } from "react";

import type {
  ArtifactPageSetup,
  DocumentBlock,
  SnapshotEnvelope,
} from "@open-office/schema/artifact";

type Listener = () => void;

export interface BlockProjectionStructure {
  /** Root ids are the only structure data needed by the document renderer. */
  root: readonly string[];
  pageSetup: ArtifactPageSetup | null;
  revision: number;
}

export interface BlockProjectionLocation {
  parentId?: string;
  index: number;
}

export interface BlockProjectionChange {
  revision: number;
  /** Blocks read from the canonical engine for this ChangeSet. */
  blocks: readonly DocumentBlock[];
  /** A full snapshot is supplied for structural changes. */
  root?: readonly string[];
  pageSetup?: ArtifactPageSetup | null;
}

/**
 * View-only projection of the canonical DocumentEngine.
 *
 * This store deliberately does not retain a SnapshotEnvelope or DocumentModel. The Rust/WASM
 * engine owns the canonical tree; the projection retains only block references and the small
 * structure index needed to mount DOM nodes. A content ChangeSet updates only the affected block
 * entries, so BlockNode subscriptions are local and the editor never clones the whole model.
 */
export class BlockProjectionStore {
  private blocks = new Map<string, DocumentBlock>();
  private blockListeners = new Map<string, Set<Listener>>();
  private structureListeners = new Set<Listener>();
  private structureSnapshot: BlockProjectionStructure = {
    root: [],
    pageSetup: null,
    revision: 0,
  };
  private pendingNotify: Set<string> | null = null;
  private structureDirty = false;
  private notifyScheduled = false;

  constructor(snapshot: SnapshotEnvelope | null = null) {
    if (snapshot) this.replaceSnapshot(snapshot, false);
  }

  getBlock(id: string): DocumentBlock | null {
    return this.blocks.get(id) ?? null;
  }

  getStructureSnapshot(): BlockProjectionStructure {
    return this.structureSnapshot;
  }

  /**
   * Resolve a block's sibling position from the view projection. Structural commands
   * may use this read-only index, while the engine remains the only writer.
   */
  findLocation(id: string): BlockProjectionLocation | null {
    const rootIndex = this.structureSnapshot.root.indexOf(id);
    if (rootIndex >= 0) return { index: rootIndex };
    const visit = (parentId: string, children: readonly string[]): BlockProjectionLocation | null => {
      const index = children.indexOf(id);
      if (index >= 0) return { parentId, index };
      for (const childId of children) {
        const child = this.blocks.get(childId);
        if (!child || child.children.length === 0) continue;
        const nested = visit(childId, child.children);
        if (nested) return nested;
      }
      return null;
    };
    for (const rootId of this.structureSnapshot.root) {
      const root = this.blocks.get(rootId);
      if (!root || root.children.length === 0) continue;
      const nested = visit(rootId, root.children);
      if (nested) return nested;
    }
    return null;
  }

  subscribe(listener: Listener): () => void {
    this.structureListeners.add(listener);
    return () => this.structureListeners.delete(listener);
  }

  subscribeStructure(listener: Listener): () => void {
    return this.subscribe(listener);
  }

  subscribeBlock(id: string, listener: Listener): () => void {
    let listeners = this.blockListeners.get(id);
    if (!listeners) {
      listeners = new Set<Listener>();
      this.blockListeners.set(id, listeners);
    }
    listeners.add(listener);
    return () => {
      listeners?.delete(listener);
      if (listeners?.size === 0) this.blockListeners.delete(id);
    };
  }

  /**
   * Replaces the projection after an initial load, conflict rebase, or structural ChangeSet.
   * The incoming snapshot is read-only input; no envelope/model is retained by this store.
   */
  replaceSnapshot(next: SnapshotEnvelope | null, notify = true): void {
    const nextModel = next?.artifact.payload.kind === "document"
      ? next.artifact.payload.data
      : null;
    const nextBlocks = new Map<string, DocumentBlock>();
    const changed = new Set<string>();

    for (const block of nextModel?.blocks ?? []) {
      const previous = this.blocks.get(block.id);
      nextBlocks.set(block.id, block);
      if (previous !== block) changed.add(block.id);
    }
    for (const id of this.blocks.keys()) {
      if (!nextBlocks.has(id)) changed.add(id);
    }

    const nextStructure: BlockProjectionStructure = {
      root: nextModel?.root ?? [],
      pageSetup: nextModel?.pageSetup ?? null,
      revision: next?.artifact.revision ?? 0,
    };
    const structureChanged = !sameStructure(this.structureSnapshot, nextStructure);
    this.blocks = nextBlocks;
    this.structureSnapshot = structureChanged ? nextStructure : {
      ...this.structureSnapshot,
      revision: nextStructure.revision,
    };

    if (!notify) return;
    if (structureChanged) this.queueNotify(null);
    if (changed.size > 0) this.queueNotify(changed);
  }

  /**
   * Reconciles a canonical snapshot after a commit without replacing every block reference.
   *
   * The engine is still the only source of the incoming values. `changedBlockIds` comes from
   * the engine ChangeSet or the server CommitResult invalidation; blocks outside that set are
   * deliberately retained by reference. This is not a comparison or a second model: it is the
   * projection's identity-preserving application of an authoritative invalidation boundary.
   */
  applySnapshot(
    next: SnapshotEnvelope | null,
    changedBlockIds: readonly string[] = [],
    notify = true,
  ): void {
    const nextModel = next?.artifact.payload.kind === "document"
      ? next.artifact.payload.data
      : null;
    if (!next || !nextModel) {
      this.replaceSnapshot(next, notify);
      return;
    }

    const changedIds = new Set(changedBlockIds);
    const nextBlocks = new Map<string, DocumentBlock>();
    const changed = new Set<string>();
    for (const block of nextModel.blocks) {
      const previous = this.blocks.get(block.id);
      const shouldReplace = previous === undefined || changedIds.has(block.id);
      const projected = shouldReplace ? block : previous;
      nextBlocks.set(block.id, projected);
      if (projected !== previous) changed.add(block.id);
    }
    for (const id of this.blocks.keys()) {
      if (!nextBlocks.has(id)) changed.add(id);
    }

    const nextStructure: BlockProjectionStructure = {
      root: nextModel.root,
      pageSetup: nextModel.pageSetup ?? null,
      revision: next.artifact.revision,
    };
    const structureChanged = !sameStructure(this.structureSnapshot, nextStructure);
    this.blocks = nextBlocks;
    this.structureSnapshot = structureChanged ? nextStructure : {
      ...this.structureSnapshot,
      revision: nextStructure.revision,
    };

    if (!notify) return;
    if (structureChanged) this.queueNotify(null);
    if (changed.size > 0) this.queueNotify(changed);
  }

  /**
   * Applies an incremental ChangeSet projection. The caller must provide blocks read from the
   * canonical engine; this method never accepts patches and therefore cannot become a second
   * writable document model.
   */
  applyChange(change: BlockProjectionChange, notify = true): void {
    const changed = new Set<string>();
    for (const block of change.blocks) {
      const previous = this.blocks.get(block.id);
      this.blocks.set(block.id, block);
      if (previous !== block) changed.add(block.id);
    }
    const structureChanged = change.root !== undefined || change.pageSetup !== undefined;
    if (structureChanged) {
      const nextRoot = change.root ?? this.structureSnapshot.root;
      const nextPageSetup = change.pageSetup === undefined
        ? this.structureSnapshot.pageSetup
        : change.pageSetup;
      this.structureSnapshot = {
        root: nextRoot,
        pageSetup: nextPageSetup,
        revision: change.revision,
      };
    } else {
      this.structureSnapshot = { ...this.structureSnapshot, revision: change.revision };
    }
    if (!notify) return;
    if (structureChanged) this.queueNotify(null);
    if (changed.size > 0) this.queueNotify(changed);
  }

  private queueNotify(ids: Set<string> | null): void {
    if (ids === null) {
      this.structureDirty = true;
    } else if (this.pendingNotify !== null) {
      for (const id of ids) this.pendingNotify.add(id);
    } else {
      this.pendingNotify = new Set(ids);
    }
    if (this.notifyScheduled) return;
    this.notifyScheduled = true;
    queueMicrotask(() => {
      this.notifyScheduled = false;
      const pending = this.pendingNotify;
      const structureDirty = this.structureDirty;
      this.pendingNotify = null;
      this.structureDirty = false;
      if (structureDirty) {
        for (const listener of this.structureListeners) listener();
      }
      if (pending !== null) {
        for (const id of pending) {
          for (const listener of this.blockListeners.get(id) ?? []) listener();
        }
      }
    });
  }
}

export function useBlockProjectionStructure(store: BlockProjectionStore): BlockProjectionStructure {
  return useSyncExternalStore(
    store.subscribeStructure.bind(store),
    store.getStructureSnapshot.bind(store),
    store.getStructureSnapshot.bind(store),
  );
}

export function useBlockProjection(store: BlockProjectionStore, id: string): DocumentBlock | null {
  return useSyncExternalStore(
    (listener) => store.subscribeBlock(id, listener),
    () => store.getBlock(id),
    () => store.getBlock(id),
  );
}

function sameStructure(previous: BlockProjectionStructure, next: BlockProjectionStructure): boolean {
  return sameArray(previous.root, next.root) && samePageSetup(previous.pageSetup, next.pageSetup);
}

function samePageSetup(previous: ArtifactPageSetup | null, next: ArtifactPageSetup | null): boolean {
  if (previous === next) return true;
  if (!previous || !next) return false;
  return previous.width === next.width
    && previous.height === next.height
    && previous.marginTop === next.marginTop
    && previous.marginRight === next.marginRight
    && previous.marginBottom === next.marginBottom
    && previous.marginLeft === next.marginLeft;
}

function sameArray(previous: readonly string[], next: readonly string[]): boolean {
  return previous.length === next.length && previous.every((value, index) => value === next[index]);
}
