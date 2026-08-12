import { useState } from "react";

import { Icon, MenuItem, MenuPanel, MenuSectionTitle, MenuSeparator, Popover, ToolbarMenuButton } from "@open-office/ui";
import { TableInsertPicker } from "./TableInsertPicker.js";

interface InsertMenuProps {
  disabled?: boolean;
  onInsert: () => void;
  onInsertCode: () => void;
  onInsertQuote: () => void;
  onInsertTodo: () => void;
  onInsertDivider: () => void;
  onInsertLink: () => void;
  onInsertTable: (rows: number, columns: number) => void;
}

/** 顶部工具栏的统一“插入”菜单；只有已有真实 operation 的能力才在这里出现。 */
export function InsertMenu({
  disabled = false,
  onInsert,
  onInsertCode,
  onInsertQuote,
  onInsertTodo,
  onInsertDivider,
  onInsertLink,
  onInsertTable,
}: InsertMenuProps) {
  const [open, setOpen] = useState(false);

  const run = (callback: () => void) => {
    callback();
    setOpen(false);
  };

  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      placement="bottom-start"
      role="presentation"
      popupClassName="oo-overlay--menu"
      className="toolbar__insert-trigger"
      content={(
        <MenuPanel className="oo-menu-panel--toolbar" role="menu" aria-label="插入内容">
          <MenuSectionTitle>内容</MenuSectionTitle>
          <TableInsertPicker variant="menu" onSelect={(rows, columns) => run(() => onInsertTable(rows, columns))} />
          <MenuItem icon={<Icon name="quote" />} onClick={() => run(onInsertQuote)}>引用块</MenuItem>
          <MenuItem icon={<Icon name="todo" />} onClick={() => run(onInsertTodo)}>待办事项</MenuItem>
          <MenuItem icon={<Icon name="code" />} onClick={() => run(onInsertCode)}>代码块</MenuItem>
          <MenuItem icon={<Icon name="link" />} onClick={() => run(onInsertLink)}>链接块</MenuItem>
          <MenuSeparator />
          <MenuSectionTitle>结构</MenuSectionTitle>
          <MenuItem icon={<Icon name="divider" />} onClick={() => run(onInsertDivider)}>分隔线</MenuItem>
          <MenuItem icon={<Icon name="insert" />} onClick={() => run(onInsert)}>在下方插入空白块</MenuItem>
        </MenuPanel>
      )}
    >
      <ToolbarMenuButton
        type="button"
        disabled={disabled}
        aria-label="插入内容"
        aria-haspopup="menu"
      >
        <Icon name="insert" />
        <span>插入</span>
        <Icon name="arrow-down" className="toolbar__insert-chevron" />
      </ToolbarMenuButton>
    </Popover>
  );
}
