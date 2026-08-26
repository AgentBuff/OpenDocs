import { useSyncExternalStore } from "react";

import { selectionReducer } from "./selectionReducer.js";
import { EMPTY_EDITOR_SELECTION, type EditorSelection, type SelectionEvent } from "./types.js";

type Listener = () => void;

/**
 * Session-local interaction store. It holds no DocumentModel data and has no
 * write path to the engine; consumers can subscribe without re-rendering the
 * whole editor for unrelated selection changes.
 */
export class InteractionStore {
  private selection: EditorSelection = EMPTY_EDITOR_SELECTION;
  private readonly listeners = new Set<Listener>();

  getSnapshot = (): EditorSelection => this.selection;

  subscribe = (listener: Listener): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  dispatch(event: SelectionEvent): EditorSelection {
    const next = selectionReducer(this.selection, event);
    if (next === this.selection) return next;
    this.selection = next;
    for (const listener of this.listeners) listener();
    return next;
  }

  select(selection: EditorSelection): EditorSelection {
    return this.dispatch({ type: "select", selection });
  }

  clear(): EditorSelection {
    return this.dispatch({ type: "clear" });
  }
}

export function useEditorSelection(store: InteractionStore): EditorSelection {
  return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}
