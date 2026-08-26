import { MenuItem, MenuPanel, MenuSeparator, Portal } from "@open-office/ui";
import type { MouseEvent as ReactMouseEvent } from "react";
import { useRef } from "react";
import { useManagedOverlay } from "../interaction/OverlayCoordinator.js";

export interface BlockContextMenuTarget {
  x: number;
  y: number;
  hasSelection: boolean;
  canCut: boolean;
}

interface BlockContextMenuProps {
  target: BlockContextMenuTarget;
  onDismiss: () => void;
  onCut: () => void;
  onCopy: () => void;
}

/**
 * The editor owns its context surface instead of leaking the browser's native
 * menu.  Commands without a semantic document implementation stay disabled;
 * this keeps the office vocabulary visible without manufacturing writes.
 */
export function BlockContextMenu({ target, onDismiss, onCut, onCopy }: BlockContextMenuProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  useManagedOverlay({
    id: `block-context-menu:${target.x}:${target.y}`,
    kind: "contextMenu",
    priority: 80,
    rootRef,
    onDismiss: () => onDismiss(),
  });
  const width = 184;
  const left = Math.max(8, Math.min(target.x, window.innerWidth - width - 8));
  const top = Math.max(8, Math.min(target.y, window.innerHeight - 360));
  const keepSelection = (event: ReactMouseEvent<HTMLDivElement>) => event.preventDefault();

  return (
    <Portal>
      <div ref={rootRef}>
      <MenuPanel
        className="block-row__context-menu"
        role="menu"
        aria-label="文档右键菜单"
        style={{ left, top }}
        onMouseDown={keepSelection}
        onContextMenu={(event) => event.preventDefault()}
      >
        <MenuItem disabled={!target.canCut} onClick={onCut} trailing="⌘X">剪切</MenuItem>
        <MenuItem disabled={!target.hasSelection} onClick={onCopy} trailing="⌘C">复制</MenuItem>
        <MenuItem disabled trailing="⌘V">粘贴</MenuItem>
        <MenuItem disabled trailing="⌘⇧V">仅文本粘贴</MenuItem>
        <MenuSeparator />
        <MenuItem disabled>段落设置</MenuItem>
        <MenuItem disabled>字体设置</MenuItem>
        <MenuSeparator />
        <MenuItem disabled trailing="⌘⌥M">批注</MenuItem>
        <MenuSeparator />
        <MenuItem disabled trailing="⌘K">插入链接</MenuItem>
        <MenuItem disabled>书签</MenuItem>
        <MenuItem disabled>清除格式</MenuItem>
      </MenuPanel>
      </div>
    </Portal>
  );
}
