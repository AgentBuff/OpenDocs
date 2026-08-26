import type { OverlayRegistration } from "./types.js";

export type OverlayDismissReason = "escape" | "outside-pointer";

export interface ManagedOverlay extends OverlayRegistration {
  /** True when a pointer target belongs to this overlay or one of its triggers. */
  contains: (target: Node) => boolean;
  close: (reason: OverlayDismissReason) => void;
}

type Entry = ManagedOverlay & { sequence: number };

/**
 * One session-local overlay stack. It deliberately knows nothing about React,
 * DOM placement, or document data; it only arbitrates which visible overlay
 * receives an Escape/outside-pointer dismissal first.
 */
export class OverlayStore {
  private entries: Entry[] = [];
  private nextSequence = 0;

  register(overlay: ManagedOverlay): () => void {
    if (this.entries.some((entry) => entry.id === overlay.id)) {
      throw new Error(`重复的浮层标识: ${overlay.id}`);
    }
    const entry: Entry = { ...overlay, sequence: this.nextSequence += 1 };
    this.entries.push(entry);
    return () => {
      this.entries = this.entries.filter((candidate) => candidate !== entry);
    };
  }

  dismissEscape(): boolean {
    const top = this.top((entry) => entry.closeOnEscape);
    if (!top) return false;
    top.close("escape");
    return true;
  }

  dismissOutsidePointer(target: Node): boolean {
    const top = this.top();
    if (!top || top.contains(target) || !top.closeOnOutsidePointer) return false;
    top.close("outside-pointer");
    return true;
  }

  snapshot(): readonly Pick<Entry, "id" | "kind" | "priority" | "sequence">[] {
    return [...this.entries]
      .sort(compareEntries)
      .map(({ id, kind, priority, sequence }) => ({ id, kind, priority, sequence }));
  }

  private top(predicate: (entry: Entry) => boolean = () => true): Entry | undefined {
    return [...this.entries].filter(predicate).sort(compareEntries)[0];
  }
}

function compareEntries(left: Entry, right: Entry): number {
  return right.priority - left.priority || right.sequence - left.sequence;
}
