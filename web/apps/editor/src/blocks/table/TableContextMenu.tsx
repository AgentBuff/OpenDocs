import { useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { MenuItem, MenuPanel, MenuSeparator, Portal } from "@open-office/ui";
import type { TableBorder, TableBorderPreset } from "@open-office/schema/artifact";
import type { TableSelection } from "./model.js";
import { TableBorderMenu } from "./TableBorderMenu.js";
import { useManagedOverlay } from "../../interaction/OverlayCoordinator.js";

interface TableContextMenuProps {
  target: { x: number; y: number; selection: TableSelection };
  onDismiss: () => void;
  onCut: () => void;
  onCopy: () => void;
  onInsert: (direction: "before" | "after") => void;
  onDelete: () => void;
  onMerge: () => void;
  onSplit: () => void;
  onApplyBorderPreset: (preset: TableBorderPreset, border: TableBorder) => void;
  canMerge: boolean;
  canSplit: boolean;
}

export function TableContextMenu({ target, onDismiss, onCut, onCopy, onInsert, onDelete, onMerge, onSplit, onApplyBorderPreset, canMerge, canSplit }: TableContextMenuProps) {
  const [submenu, setSubmenu] = useState<"insert" | "delete" | "border" | null>(null);
  const [submenuPosition, setSubmenuPosition] = useState<{ left: number; top: number } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const submenuRef = useRef<HTMLDivElement>(null);
  useManagedOverlay({
    id: `table-context-menu:${target.x}:${target.y}`,
    kind: "contextMenu",
    priority: 80,
    rootRef,
    excludedRefs: [submenuRef],
    onDismiss: () => onDismiss(),
  });
  const { selection } = target;
  const isRow = selection.kind === "row";
  const isColumn = selection.kind === "column";
  const structuralSelection = isRow || isColumn;
  const label = selection.kind === "cell" ? "单元格操作菜单" : isRow ? "行操作菜单" : isColumn ? "列操作菜单" : "表格操作菜单";
  const openSubmenu = (kind: "insert" | "delete" | "border", event: ReactMouseEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const width = 180;
    const gap = 4;
    const fitsRight = rect.right + gap + width <= window.innerWidth - 8;
    setSubmenu(kind);
    setSubmenuPosition({
      left: fitsRight ? rect.right + gap : Math.max(8, rect.left - width - gap),
      top: Math.max(8, Math.min(rect.top, window.innerHeight - 112)),
    });
  };

  return (
    <>
      <Portal>
      <div ref={rootRef}><MenuPanel
        className="block-table__context-menu"
        role="menu"
        aria-label={label}
        data-table-menu="context"
        style={{ left: `${target.x}px`, top: `${target.y}px` }}
        onPointerDown={(event) => event.stopPropagation()}
        onContextMenu={(event) => event.preventDefault()}
      >
      <MenuItem onClick={onCut} trailing="⌘X">剪切</MenuItem>
      <MenuItem onClick={onCopy} trailing="⌘C">复制</MenuItem>
      <MenuItem disabled trailing="⌘V">粘贴</MenuItem>
      <MenuItem disabled>段落设置</MenuItem>
      <MenuItem disabled>字体设置</MenuItem>
      <MenuSeparator />
      {!structuralSelection ? <>
        <MenuItem disabled trailing="›">插入行列</MenuItem>
        <MenuItem disabled trailing="›">删除行列</MenuItem>
      </> : <>
        <div className="block-table__context-submenu-anchor" onMouseEnter={(event) => openSubmenu("insert", event)}>
          <MenuItem trailing="›">插入行列</MenuItem>
        </div>
        <div className="block-table__context-submenu-anchor" onMouseEnter={(event) => openSubmenu("delete", event)}>
          <MenuItem trailing="›">删除行列</MenuItem>
        </div>
      </>}
      <MenuSeparator />
      <MenuItem disabled={!canMerge} onClick={onMerge}>合并单元格</MenuItem>
      <MenuItem disabled={!canSplit} onClick={onSplit}>拆分单元格</MenuItem>
      <div className="block-table__context-submenu-anchor" onMouseEnter={(event) => openSubmenu("border", event)}>
        <MenuItem trailing="›">边框</MenuItem>
      </div>
      <MenuItem disabled>单元格对齐方式</MenuItem>
      <MenuItem disabled>表格属性</MenuItem>
      <MenuSeparator />
      <MenuItem disabled>批注</MenuItem>
      <MenuItem disabled>题注</MenuItem>
      <MenuItem disabled>书签</MenuItem>
      <MenuItem disabled>清除格式</MenuItem>
      </MenuPanel></div>
      </Portal>
      {submenu && submenuPosition && (
        <Portal>
          <div ref={submenuRef}><MenuPanel
            className="block-table__context-submenu"
            role="menu"
            aria-label={submenu === "insert" ? "插入行列" : submenu === "delete" ? "删除行列" : "边框"}
            data-table-menu="submenu"
            style={submenuPosition}
            onPointerDown={(event) => event.stopPropagation()}
          >
            {submenu === "insert" ? <>
              <MenuItem onClick={() => onInsert("before")}>{isRow ? "在上方插入行" : "在左侧插入列"}</MenuItem>
              <MenuItem onClick={() => onInsert("after")}>{isRow ? "在下方插入行" : "在右侧插入列"}</MenuItem>
            </> : submenu === "delete" ? (
              <MenuItem onClick={onDelete} danger>{isRow ? "删除当前行" : "删除当前列"}</MenuItem>
            ) : <>
              <TableBorderMenu onApply={onApplyBorderPreset} />
            </>}
          </MenuPanel></div>
        </Portal>
      )}
    </>
  );
}
