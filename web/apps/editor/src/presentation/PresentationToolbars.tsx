import { Button, Icon, IconButton, Toolbar, ToolbarButton, ToolbarGroup, ToolbarSeparator, type IconName } from "@open-office/ui";
import type {
  BuiltinPresentationNodeAction,
  PresentationMultiSelectionAction,
  PresentationMultiSelectionControl,
  ResolvedPresentationNodeUi,
} from "@open-office/presentation-ui";

import {
  tableAnchorAt,
  tableAnchorsInSelection,
  tableRange,
  tableSelectionCanMerge,
  type PresentationTableNode,
  type TableSelection,
} from "./presentationTableSelection.js";

export function MultiNodeToolbar({
  controls,
  disabled,
  onAction,
}: {
  controls: readonly PresentationMultiSelectionControl[];
  disabled: boolean;
  onAction: (action: PresentationMultiSelectionAction) => void;
}) {
  const alignment = controls.filter((control) => control.action.startsWith("selection.align"));
  const distribution = controls.filter((control) => control.action.startsWith("selection.distribute"));
  return <Toolbar className="presentation-studio__node-toolbar" aria-label="多对象排列工具栏">
    {controls.some((control) => control.action === "selection.group") && <ToolbarGroup aria-label="组合对象">
      <ToolbarButton aria-label="组合对象" title="组合对象" disabled={disabled} onClick={() => onAction("selection.group")}><Icon name="merge-cells" /></ToolbarButton>
    </ToolbarGroup>}
    <ToolbarGroup aria-label="对齐对象">
      {alignment.map((control) => <ToolbarButton key={control.action} aria-label={control.label} title={control.label} disabled={disabled} onClick={() => onAction(control.action)}>
        <Icon name={control.icon as IconName} />
      </ToolbarButton>)}
    </ToolbarGroup>
    {distribution.length > 0 && <>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="分布对象">
        {distribution.map((control) => <ToolbarButton key={control.action} aria-label={control.label} title={control.label} disabled={disabled} onClick={() => onAction(control.action)}>
          <Icon name={control.icon as IconName} />
        </ToolbarButton>)}
      </ToolbarGroup>
    </>}
  </Toolbar>;
}

/**
 * The contextual toolbar is intentionally compact; this companion inspector
 * makes the exact same capability-filtered arrangement surface discoverable
 * without inventing a second object-order state model.
 */
export function MultiSelectionInspector({
  controls,
  disabled,
  onAction,
  onClose,
}: {
  controls: readonly PresentationMultiSelectionControl[];
  disabled: boolean;
  onAction: (action: PresentationMultiSelectionAction) => void;
  onClose: () => void;
}) {
  const alignment = controls.filter((control) => control.action.startsWith("selection.align"));
  const distribution = controls.filter((control) => control.action.startsWith("selection.distribute"));
  const ordering = controls.filter((control) => control.action.startsWith("selection.bring") || control.action.startsWith("selection.send"));
  const grouping = controls.filter((control) => control.action === "selection.group");
  const Section = ({ title, items }: { title: string; items: readonly PresentationMultiSelectionControl[] }) => items.length > 0 ? <section className="presentation-studio__inspector-section">
    <h3>{title}</h3>
    <div className="presentation-studio__arrange-actions">
      {items.map((control) => <Button key={control.action} type="button" size="sm" variant="secondary" disabled={disabled} onClick={() => onAction(control.action)}>
        <Icon name={control.icon as IconName} />{control.label}
      </Button>)}
    </div>
  </section> : null;
  return <div className="presentation-studio__inspector-card">
    <header className="presentation-studio__inspector-header">
      <div><span>已选择对象</span><strong>{controls[0] ? "排列" : ""}</strong></div>
      <IconButton type="button" variant="ghost" size="sm" aria-label="关闭多对象排列" title="关闭多对象排列" onClick={onClose}><Icon name="close" /></IconButton>
    </header>
    <Section title="组合" items={grouping} />
    <Section title="对齐" items={alignment} />
    <Section title="分布" items={distribution} />
    <Section title="层级" items={ordering} />
  </div>;
}

export function SlideToolbar({
  disabled,
  availableCapabilities,
  onOpenInspector,
}: {
  disabled: boolean;
  availableCapabilities: ReadonlySet<string>;
  onOpenInspector: () => void;
}) {
  const canConfigure = [
    "presentation.setSlideBackground",
    "presentation.setSlideNotes",
    "presentation.setSlideTransition",
  ].some((capability) => availableCapabilities.has(capability));
  if (!canConfigure) return null;
  return <Toolbar className="presentation-studio__node-toolbar" aria-label="幻灯片工具栏">
    <ToolbarGroup aria-label="幻灯片属性">
      <ToolbarButton aria-label="打开幻灯片属性" title="背景、备注与切换" disabled={disabled} onClick={onOpenInspector}>
        <Icon name="settings" />
      </ToolbarButton>
    </ToolbarGroup>
  </Toolbar>;
}

/** Table-only controls consume the transient grid range, while their effects
 * remain registry-owned semantic commands. This is intentionally separate
 * from the generic node toolbar: a table range is not a second scene-node
 * selection model. */
export function TableToolbar({
  node,
  selection,
  disabled,
  availableCapabilities,
  onAction,
  onOpenInspector,
}: {
  node: PresentationTableNode;
  selection: TableSelection;
  disabled: boolean;
  availableCapabilities: ReadonlySet<string>;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
  onOpenInspector: () => void;
}) {
  const range = tableRange(selection);
  const anchors = tableAnchorsInSelection(node, selection);
  const focused = tableAnchorAt(node, selection.focus);
  const can = (capability: string) => availableCapabilities.has(capability);
  const canMerge = can("presentation.mergeTableCells") && tableSelectionCanMerge(node, selection);
  const canSplit = can("presentation.splitTableCell") && anchors.length === 1 && Boolean(focused && (focused.rowSpan > 1 || focused.columnSpan > 1));
  return <Toolbar className="presentation-studio__node-toolbar" aria-label="表格工具栏">
    <ToolbarGroup aria-label="单元格">
      {can("presentation.setTableCellContent") && <ToolbarButton aria-label="编辑单元格" title="编辑单元格" disabled={disabled || anchors.length !== 1} onClick={onOpenInspector}><Icon name="text" /></ToolbarButton>}
      {can("presentation.setTableCellStyle") && <ToolbarButton aria-label="单元格样式" title="单元格样式" disabled={disabled} onClick={onOpenInspector}><Icon name="table" /></ToolbarButton>}
    </ToolbarGroup>
    {(can("presentation.insertTableRows") || can("presentation.insertTableColumns") || canMerge || canSplit) && <>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="表格结构">
        {can("presentation.insertTableRows") && <ToolbarButton aria-label="在下方插入行" title="在下方插入行" disabled={disabled} onClick={() => onAction("table.insertRows", { index: range.end.row + 1, count: 1 })}><Icon name="insert-row-column" /></ToolbarButton>}
        {can("presentation.insertTableColumns") && <ToolbarButton aria-label="在右侧插入列" title="在右侧插入列" disabled={disabled} onClick={() => onAction("table.insertColumns", { index: range.end.column + 1, count: 1 })}><Icon name="insert-row-column" /></ToolbarButton>}
        {canMerge && <ToolbarButton aria-label="合并单元格" title="合并单元格" disabled={disabled} onClick={() => onAction("table.mergeCells", range)}><Icon name="merge-cells" /></ToolbarButton>}
        {canSplit && focused && <ToolbarButton aria-label="拆分单元格" title="拆分单元格" disabled={disabled} onClick={() => onAction("table.splitCell", { row: focused.row, column: focused.column })}><Icon name="split-cells" /></ToolbarButton>}
      </ToolbarGroup>
    </>}
    <ToolbarSeparator />
    <ToolbarGroup aria-label="对象属性">
      <ToolbarButton aria-label="打开表格属性" title="打开表格属性" disabled={disabled} onClick={onOpenInspector}><Icon name="settings" /></ToolbarButton>
    </ToolbarGroup>
  </Toolbar>;
}

export function NodeToolbar({
  ui,
  locked,
  disabled,
  downloadUrl,
  onAction,
  onOpenInspector,
}: {
  ui: ResolvedPresentationNodeUi<BuiltinPresentationNodeAction>;
  locked: boolean;
  disabled: boolean;
  /** Asset downloads are a read-only resource operation, not a document command. */
  downloadUrl: string | null;
  onAction: (action: BuiltinPresentationNodeAction) => void;
  onOpenInspector: () => void;
}) {
  const iconForAction: Record<BuiltinPresentationNodeAction, IconName> = {
    "node.duplicate": "copy",
    "node.delete": "delete",
    "node.transform": "settings",
    "node.lock": locked ? "unlock" : "lock",
    "node.bringForward": "bring-forward",
    "node.sendBackward": "send-backward",
    "node.bringToFront": "bring-front",
    "node.sendToBack": "send-back",
    "shape.style": "shape",
    "shape.geometry": "shape",
    "chart.spec": "table",
    "connector.endpoints": "arrow-right",
    "table.cellContent": "text",
    "table.cellStyle": "table",
    "table.insertRows": "insert-row-column",
    "table.insertColumns": "insert-row-column",
    "table.deleteRow": "delete",
    "table.deleteColumn": "delete",
    "table.mergeCells": "merge-cells",
    "table.splitCell": "split-cells",
    "text.content": "text",
    "text.frame": "text",
    "image.config": "crop",
    "media.config": "settings",
    "group.ungroup": "split-cells",
  };
  return (
    <Toolbar className="presentation-studio__node-toolbar" aria-label={`${ui.inspector.title}工具栏`}>
      <ToolbarGroup aria-label={`${ui.inspector.title}操作`}>
        {ui.toolbar.map((item) => item.action && (
          <ToolbarButton
            key={item.id}
            aria-label={item.action === "node.lock" ? locked ? "解除锁定" : "锁定对象" : item.ariaLabel ?? item.label}
            title={item.action === "node.lock" ? locked ? "解除锁定" : "锁定对象" : item.label}
            disabled={disabled || item.enabled === false}
            onClick={() => onAction(item.action as BuiltinPresentationNodeAction)}
          >
            <Icon name={iconForAction[item.action as BuiltinPresentationNodeAction]} />
          </ToolbarButton>
        ))}
        {downloadUrl && <a className="presentation-studio__node-tool presentation-studio__node-tool--download" href={downloadUrl} download title="下载原始图片" aria-label="下载原始图片"><Icon name="download" /></a>}
      </ToolbarGroup>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="对象属性">
        <ToolbarButton aria-label="打开对象属性" title="对象属性" disabled={disabled} onClick={onOpenInspector}><Icon name="settings" /></ToolbarButton>
      </ToolbarGroup>
    </Toolbar>
  );
}
