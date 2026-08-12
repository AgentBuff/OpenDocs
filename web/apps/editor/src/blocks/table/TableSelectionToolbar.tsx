import {
  Icon,
  MenuItem,
  MenuPanel,
  Popover,
  ToolbarButton,
  ToolbarGroup,
  ToolbarSplitGroup,
} from "@open-office/ui";
import { useState, type CSSProperties, type ReactNode } from "react";
import type { TableBorder, TableBorderPreset } from "@open-office/schema/artifact";
import { ColorPalette, type ColorRole, type ColorValue } from "../../chrome/ColorPalette.js";
import type { TableFormatState, TableGeometry, TableSelection } from "./model.js";
import { TABLE_SELECTION_TOOLBAR_GROUPS, type TableToolbarActionId } from "./toolbarContract.js";
import { TableBorderMenu } from "./TableBorderMenu.js";

interface TableSelectionToolbarProps {
  selection: TableSelection;
  geometry: TableGeometry;
  onInsert: (direction: "before" | "after") => void;
  onMerge: () => void;
  onSplit: () => void;
  canMerge: boolean;
  canSplit: boolean;
  formatState: TableFormatState;
  onFormat: (patch: {
    textAttrs?: Record<string, unknown>;
    fillColor?: string | null;
    horizontalAlign?: "left" | "center" | "right";
    verticalAlign?: "top" | "middle" | "bottom";
  }) => void;
  onApplyBorderPreset: (preset: TableBorderPreset, border: TableBorder) => void;
}

/**
 * Table selection toolbar deliberately mirrors the office vocabulary while
 * applying the canonical table-cell formatting and border commands for the
 * selected stable-id range. The view never mutates a cell payload directly.
 */
export function TableSelectionToolbar({
  selection,
  geometry,
  onInsert,
  onMerge,
  onSplit,
  canMerge,
  canSplit,
  formatState,
  onFormat,
  onApplyBorderPreset,
}: TableSelectionToolbarProps) {
  const selectionStart = selection.kind === "row"
    ? geometry.rows.find((row) => row.id === selection.id)?.top ?? geometry.tableTop
    : geometry.tableTop;
  const selectionLeft = selection.kind === "cell"
    ? Math.max(0, geometry.tableLeft - 48)
    : selection.kind === "column"
    ? geometry.columns.find((column) => column.id === selection.id)?.left ?? geometry.tableLeft
    : selection.kind === "all" ? geometry.tableLeft : Math.max(0, geometry.tableLeft - 48);
  const label = selection.kind === "cell" ? "单元格工具栏" : selection.kind === "row" ? "行选区工具栏" : selection.kind === "column" ? "列选区工具栏" : "表格选区工具栏";
  const structuralSelection = selection.kind === "row" || selection.kind === "column";

  const renderAction = (action: TableToolbarActionId): ReactNode => {
    switch (action) {
      case "fontIncrease":
        return <ToolbarButton key={action} aria-label="增大字号" title="增大字号" onClick={() => onFormat({ textAttrs: { fontSize: Math.min(512, (formatState.fontSize ?? 14) + 1) } })}><Icon name="font-increase" /></ToolbarButton>;
      case "fontDecrease":
        return <ToolbarButton key={action} aria-label="减小字号" title="减小字号" onClick={() => onFormat({ textAttrs: { fontSize: Math.max(1, (formatState.fontSize ?? 14) - 1) } })}><Icon name="font-decrease" /></ToolbarButton>;
      case "bold":
        return <ToolbarButton key={action} active={formatState.bold} aria-label="加粗" title="加粗" onClick={() => onFormat({ textAttrs: { bold: !formatState.bold } })}><Icon name="bold" /></ToolbarButton>;
      case "textHighlight":
        return <TableColorButton key={action} label="底纹颜色" role="highlight" icon="highlight" defaultColor="#FADC19" selectedColor={formatState.highlightColor} onSelect={(color) => onFormat({ textAttrs: { highlight: color } })} />;
      case "textColor":
        return <TableColorButton key={action} label="字体颜色" role="text" icon="font-colors" defaultColor="#1D2129" selectedColor={formatState.textColor} onSelect={(color) => onFormat({ textAttrs: { color } })} />;
      case "italic":
        return <ToolbarButton key={action} active={formatState.italic} aria-label="斜体" title="斜体" onClick={() => onFormat({ textAttrs: { italic: !formatState.italic } })}><Icon name="italic" /></ToolbarButton>;
      case "underline":
        return <ToolbarButton key={action} active={formatState.underline} aria-label="下划线" title="下划线" onClick={() => onFormat({ textAttrs: { underline: !formatState.underline } })}><Icon name="underline" /></ToolbarButton>;
      case "strikethrough":
        return <ToolbarButton key={action} active={formatState.strikethrough} aria-label="删除线" title="删除线" onClick={() => onFormat({ textAttrs: { strikethrough: !formatState.strikethrough } })}><Icon name="strikethrough" /></ToolbarButton>;
      case "cellFill":
        return <TableColorButton key={action} label="单元格填充颜色" role="highlight" icon="bg-colors" defaultColor="#F53F3F" selectedColor={formatState.fillColor} onSelect={(color) => onFormat({ fillColor: color })} />;
      case "borders":
        return (
          <TableMenuButton key={action} label="边框和框线" icon="table-borders">
            <TableBorderMenu onApply={onApplyBorderPreset} />
          </TableMenuButton>
        );
      case "horizontalAlign":
        return (
          <TableMenuButton key={action} label="水平对齐方式" icon="align-left">
            <MenuItem selected={formatState.horizontalAlign === "left"} onClick={() => onFormat({ horizontalAlign: "left" })}>左对齐</MenuItem>
            <MenuItem selected={formatState.horizontalAlign === "center"} onClick={() => onFormat({ horizontalAlign: "center" })}>居中对齐</MenuItem>
            <MenuItem selected={formatState.horizontalAlign === "right"} onClick={() => onFormat({ horizontalAlign: "right" })}>右对齐</MenuItem>
          </TableMenuButton>
        );
      case "verticalAlign":
        return (
          <TableMenuButton key={action} label="垂直对齐方式" icon="vertical-align">
            <MenuItem selected={formatState.verticalAlign === "top"} onClick={() => onFormat({ verticalAlign: "top" })}>顶部对齐</MenuItem>
            <MenuItem selected={formatState.verticalAlign === "middle"} onClick={() => onFormat({ verticalAlign: "middle" })}>垂直居中</MenuItem>
            <MenuItem selected={formatState.verticalAlign === "bottom"} onClick={() => onFormat({ verticalAlign: "bottom" })}>底部对齐</MenuItem>
          </TableMenuButton>
        );
      case "mergeOrSplit":
        if (canSplit) return <ToolbarButton key={action} aria-label="拆分单元格" title="拆分单元格" onClick={onSplit}><Icon name="split-cells" /></ToolbarButton>;
        if (canMerge) return <ToolbarButton key={action} aria-label="合并单元格" title="合并单元格" onClick={onMerge}><Icon name="merge-cells" /></ToolbarButton>;
        return null;
      case "insertRowColumn":
        if (!structuralSelection) return null;
        return (
          <Popover
            key={action}
            placement="bottom-start"
            role="presentation"
            popupClassName="oo-overlay--table-toolbar-menu"
            content={(
              <MenuPanel role="menu" aria-label="插入行列">
                <MenuItem onClick={() => onInsert("before")}>{selection.kind === "row" ? "在上方插入行" : "在左侧插入列"}</MenuItem>
                <MenuItem onClick={() => onInsert("after")}>{selection.kind === "row" ? "在下方插入行" : "在右侧插入列"}</MenuItem>
              </MenuPanel>
            )}
          >
            <ToolbarButton aria-label="插入行列" title="插入行列" aria-haspopup="menu">
              <Icon name="insert-row-column" />
            </ToolbarButton>
          </Popover>
        );
    }
  };

  return (
    <div
      className="block-table__selection-toolbar"
      role="toolbar"
      aria-label={label}
      style={{ left: `${selectionLeft}px`, top: `${Math.max(-48, selectionStart - 48)}px` }}
      onMouseDown={(event) => {
        // Preserve a native cell text range until the click dispatches its
        // semantic PatchTableCellInlineRange command.
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      {TABLE_SELECTION_TOOLBAR_GROUPS.map((group) => {
        const actions = group.actions.map(renderAction).filter((action): action is Exclude<ReactNode, null> => action !== null);
        return actions.length > 0 ? (
          <ToolbarGroup key={group.id} role="group" aria-label={group.label} className="block-table__toolbar-group">
            {actions}
          </ToolbarGroup>
        ) : null;
      })}
    </div>
  );
}

function TableMenuButton({
  label,
  icon,
  children,
}: {
  label: string;
  icon: "table-borders" | "align-left" | "vertical-align";
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      placement="bottom-start"
      role="presentation"
      popupClassName="oo-overlay--table-toolbar-menu"
      content={<MenuPanel role="menu" aria-label={label} onClick={() => setOpen(false)}>{children}</MenuPanel>}
    >
      <ToolbarButton aria-label={label} title={label} aria-haspopup="menu">
        <Icon name={icon} />
        <Icon name="arrow-down" size={10} />
      </ToolbarButton>
    </Popover>
  );
}

function TableColorButton({
  label,
  role,
  icon,
  defaultColor,
  selectedColor,
  onSelect,
}: {
  label: string;
  role: ColorRole;
  icon: "highlight" | "font-colors" | "bg-colors";
  defaultColor: string;
  selectedColor: string | null;
  onSelect: (color: ColorValue) => void;
}) {
  const [open, setOpen] = useState(false);
  const [recentColors, setRecentColors] = useState<string[]>([]);
  const apply = (color: ColorValue) => {
    if (color) setRecentColors((previous) => [color, ...previous.filter((item) => item !== color)].slice(0, 10));
    onSelect(color);
    setOpen(false);
  };
  return (
    <ToolbarSplitGroup aria-label={label}>
      <ToolbarButton aria-label={label} title={label} onClick={() => apply(selectedColor ?? defaultColor)}>
        <span
          className="toolbar-color-trigger-icon"
          style={{ "--oo-toolbar-color-value": selectedColor ?? defaultColor } as CSSProperties}
        >
          <Icon name={icon} />
        </span>
      </ToolbarButton>
      <Popover
        open={open}
        onOpenChange={setOpen}
        placement="bottom-start"
        role="presentation"
        popupClassName="oo-overlay--toolbar-color"
        content={<ColorPalette role={role} selectedColor={selectedColor} recentColors={recentColors} onSelect={apply} />}
      >
        <ToolbarButton className="oo-toolbar__item--split-arrow" aria-label={`${label}更多选项`} aria-haspopup="menu">
          <Icon name="arrow-down" />
        </ToolbarButton>
      </Popover>
    </ToolbarSplitGroup>
  );
}
