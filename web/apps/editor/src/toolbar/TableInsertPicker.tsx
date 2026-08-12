import { useState } from "react";

import { Icon, MenuItem, Popover, ToolbarButton } from "@open-office/ui";

const MAX_ROWS = 8;
const MAX_COLUMNS = 10;

interface TableInsertPickerProps {
  onSelect: (rows: number, columns: number) => void;
  disabled?: boolean;
  variant?: "toolbar" | "menu";
}

/**
 * 腾讯文档式表格规格选择器：先在网格里预览行列，再提交一个真正的 table Block。
 * 自定义尺寸也留在同一个弹层内，不把尺寸输入散落到业务命令或 prompt 中。
 */
export function TableInsertPicker({ onSelect, disabled = false, variant = "toolbar" }: TableInsertPickerProps) {
  const [open, setOpen] = useState(false);
  const [customOpen, setCustomOpen] = useState(false);
  const [hovered, setHovered] = useState<{ rows: number; columns: number } | null>(null);
  const [customRows, setCustomRows] = useState(3);
  const [customColumns, setCustomColumns] = useState(3);

  const close = () => {
    setOpen(false);
    setCustomOpen(false);
    setHovered(null);
  };

  const select = (rows: number, columns: number) => {
    onSelect(rows, columns);
    close();
  };

  const preview = hovered ?? { rows: customRows, columns: customColumns };

  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      placement={variant === "menu" ? "right-start" : "bottom"}
      role="presentation"
      popupClassName="oo-overlay--table-picker"
      className={`table-picker table-picker--${variant}`}
      content={(
        <div className="table-picker__popover" role="dialog" aria-label="选择表格规格">
          <div className="table-picker__title">{preview.rows}×{preview.columns} 表格</div>
          <div
            className="table-picker__grid"
            role="grid"
            aria-label="表格行列选择"
            onMouseLeave={() => setHovered(null)}
            style={{ gridTemplateColumns: `repeat(${MAX_COLUMNS}, 18px)` }}
          >
            {Array.from({ length: MAX_ROWS * MAX_COLUMNS }, (_, index) => {
              const row = Math.floor(index / MAX_COLUMNS) + 1;
              const column = index % MAX_COLUMNS + 1;
              const selected = row <= preview.rows && column <= preview.columns;
              return (
                <button
                  key={`${row}-${column}`}
                  className={`table-picker__cell${selected ? " is-selected" : ""}`}
                  type="button"
                  role="gridcell"
                  aria-label={`${row}行${column}列表格`}
                  aria-selected={selected}
                  onMouseEnter={() => setHovered({ rows: row, columns: column })}
                  onFocus={() => setHovered({ rows: row, columns: column })}
                  onClick={() => select(row, column)}
                />
              );
            })}
          </div>
          <div className="table-picker__footer">
            {!customOpen ? (
              <button className="table-picker__custom" type="button" onClick={() => setCustomOpen(true)}>
                自定义行列
              </button>
            ) : (
              <div className="table-picker__custom-form">
                <label><span>行</span><input type="number" min={1} max={100} value={customRows} onChange={(event) => setCustomRows(clampDimension(event.target.value, customRows, 100))} /></label>
                <label><span>列</span><input type="number" min={1} max={20} value={customColumns} onChange={(event) => setCustomColumns(clampDimension(event.target.value, customColumns, 20))} /></label>
                <button type="button" onClick={() => select(customRows, customColumns)}>插入</button>
              </div>
            )}
          </div>
        </div>
      )}
    >
      {variant === "menu" ? (
        <MenuItem
          icon={<Icon name="table" />}
          trailing={<span>›</span>}
          aria-label="插入表格"
          aria-haspopup="dialog"
          aria-expanded={open}
          title="插入表格"
          disabled={disabled}
        >
          表格
        </MenuItem>
      ) : (
        <ToolbarButton
          active={open}
          aria-label="插入表格"
          aria-haspopup="dialog"
          aria-expanded={open}
          title="插入表格"
          disabled={disabled}
        >
          <Icon name="table" />
        </ToolbarButton>
      )}
    </Popover>
  );
}

function clampDimension(value: string, fallback: number, maximum: number): number {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return fallback;
  return Math.max(1, Math.min(maximum, Math.floor(parsed)));
}
