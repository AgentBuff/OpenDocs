import { useCallback, useRef, useState } from "react";

import type { PresentationV5Node } from "@open-office/schema";

import { nextNodeSelection } from "./interactions.js";
import {
  tableAnchorAt,
  type PresentationTableNode,
  type TableCellAddress,
  type TableSelection,
} from "./presentationTableSelection.js";

/** Ephemeral editor selection; none of this state is persisted in the Deck. */
export function usePresentationSelection() {
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedNodeIds, setSelectedNodeIds] = useState<readonly string[]>([]);
  const [tableSelection, setTableSelection] = useState<TableSelection | null>(null);
  const tableSelectionRef = useRef<TableSelection | null>(null);
  const [editingNodeId, setEditingNodeId] = useState<string | null>(null);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [slideInspectorOpen, setSlideInspectorOpen] = useState(false);
  const [deckInspectorOpen, setDeckInspectorOpen] = useState(false);

  const clearTableSelection = useCallback(() => {
    tableSelectionRef.current = null;
    setTableSelection(null);
  }, []);

  const clearNodeSelection = useCallback(() => {
    setSelectedNodeId(null);
    setSelectedNodeIds([]);
    clearTableSelection();
    setEditingNodeId(null);
  }, [clearTableSelection]);

  const reconcileSelection = useCallback((nodes: readonly PresentationV5Node[]) => {
    setSelectedNodeId((current) => nodes.some((node) => node.id === current) ? current : null);
    setSelectedNodeIds((current) => current.filter((nodeId) => nodes.some((node) => node.id === nodeId)));
  }, []);

  const selectNodes = useCallback((nodeId: string, extend = false) => {
    setSelectedNodeId(nodeId);
    setSelectedNodeIds((current) => nextNodeSelection(current, nodeId, extend));
    clearTableSelection();
    setEditingNodeId(null);
    setSlideInspectorOpen(false);
    setDeckInspectorOpen(false);
    setInspectorOpen(true);
  }, [clearTableSelection]);

  const selectTableCell = useCallback((
    node: PresentationTableNode,
    address: TableCellAddress,
    extend = false,
  ) => {
    const canonical = tableAnchorAt(node, address);
    if (!canonical) return;
    const current = tableSelectionRef.current;
    const next: TableSelection = extend && current?.nodeId === node.id
      ? { ...current, focus: { row: canonical.row, column: canonical.column } }
      : {
          nodeId: node.id,
          anchor: { row: canonical.row, column: canonical.column },
          focus: { row: canonical.row, column: canonical.column },
        };
    tableSelectionRef.current = next;
    setTableSelection(next);
    setSelectedNodeId(node.id);
    setSelectedNodeIds([node.id]);
    setEditingNodeId(null);
    setSlideInspectorOpen(false);
    setDeckInspectorOpen(false);
    setInspectorOpen(true);
  }, []);

  return {
    selectedNodeId,
    selectedNodeIds,
    tableSelection,
    tableSelectionRef,
    editingNodeId,
    inspectorOpen,
    slideInspectorOpen,
    deckInspectorOpen,
    setSelectedNodeId,
    setSelectedNodeIds,
    setTableSelection,
    setEditingNodeId,
    setInspectorOpen,
    setSlideInspectorOpen,
    setDeckInspectorOpen,
    clearTableSelection,
    clearNodeSelection,
    reconcileSelection,
    selectNodes,
    selectTableCell,
  };
}
