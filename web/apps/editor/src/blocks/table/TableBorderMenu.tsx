import { MenuItem, MenuSectionTitle, MenuSeparator } from "@open-office/ui";
import type { TableBorder, TableBorderPreset } from "@open-office/schema/artifact";
import { useState } from "react";

/** Shared office-style border gallery used by both the floating toolbar and
 * the table context menu. It emits one semantic preset, never a client-side
 * collection of per-cell patches. */
export function TableBorderMenu({
  onApply,
  border = DEFAULT_TABLE_BORDER,
}: {
  onApply: (preset: TableBorderPreset, border: TableBorder) => void;
  border?: TableBorder;
}) {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [configuredBorder, setConfiguredBorder] = useState<TableBorder>(border);
  if (settingsOpen) {
    return (
      <div className="block-table__border-settings" onClick={(event) => event.stopPropagation()}>
        <MenuSectionTitle>边框设置</MenuSectionTitle>
        <label className="block-table__border-setting">
          <span>颜色</span>
          <input aria-label="边框颜色" type="color" value={configuredBorder.color.slice(0, 7)} onChange={(event) => setConfiguredBorder((value) => ({ ...value, color: event.target.value }))} />
        </label>
        <label className="block-table__border-setting">
          <span>线型</span>
          <select aria-label="边框线型" value={configuredBorder.style} onChange={(event) => setConfiguredBorder((value) => ({ ...value, style: event.target.value as TableBorder["style"] }))}>
            <option value="solid">实线</option>
            <option value="dashed">虚线</option>
            <option value="dotted">点线</option>
            <option value="double">双线</option>
          </select>
        </label>
        <label className="block-table__border-setting">
          <span>宽度</span>
          <select aria-label="边框宽度" value={configuredBorder.width} onChange={(event) => setConfiguredBorder((value) => ({ ...value, width: Number(event.target.value) }))}>
            <option value={1}>1 px</option>
            <option value={1.5}>1.5 px</option>
            <option value={2}>2 px</option>
            <option value={3}>3 px</option>
          </select>
        </label>
        <MenuSeparator />
        <MenuItem onClick={() => { onApply("all", configuredBorder); setSettingsOpen(false); }}>应用到所有框线</MenuItem>
        <MenuItem onClick={() => setSettingsOpen(false)}>返回</MenuItem>
      </div>
    );
  }
  const apply = (preset: TableBorderPreset) => onApply(preset, border);
  return (
    <>
      <BorderMenuItem preset="bottom" label="下框线" onClick={() => apply("bottom")} />
      <BorderMenuItem preset="top" label="上框线" onClick={() => apply("top")} />
      <BorderMenuItem preset="left" label="左框线" onClick={() => apply("left")} />
      <BorderMenuItem preset="right" label="右框线" onClick={() => apply("right")} />
      <MenuSeparator />
      <BorderMenuItem preset="none" label="无框线" onClick={() => apply("none")} />
      <BorderMenuItem preset="all" label="所有框线" onClick={() => apply("all")} />
      <BorderMenuItem preset="outer" label="外部框线" onClick={() => apply("outer")} />
      <BorderMenuItem preset="inner" label="内部框线" onClick={() => apply("inner")} />
      <BorderMenuItem preset="innerHorizontal" label="内部横框线" onClick={() => apply("innerHorizontal")} />
      <BorderMenuItem preset="innerVertical" label="内部竖框线" onClick={() => apply("innerVertical")} />
      <BorderMenuItem preset="diagonalDown" label="斜向下框线" onClick={() => apply("diagonalDown")} />
      <BorderMenuItem preset="diagonalUp" label="斜向上框线" onClick={() => apply("diagonalUp")} />
      <MenuSeparator />
      <MenuItem onClick={(event) => { event.stopPropagation(); setSettingsOpen(true); }}>边框设置</MenuItem>
    </>
  );
}

function BorderMenuItem({ preset, label, onClick }: { preset: TableBorderPreset; label: string; onClick: () => void }) {
  return (
    <MenuItem icon={<BorderPresetGlyph preset={preset} />} onClick={onClick}>{label}</MenuItem>
  );
}

/** A compact visual preview, matching the semantics of each menu item. */
function BorderPresetGlyph({ preset }: { preset: TableBorderPreset }) {
  const classes = `block-table__border-glyph block-table__border-glyph--${preset}`;
  return <span className={classes} aria-hidden="true" />;
}

export const DEFAULT_TABLE_BORDER: TableBorder = { style: "solid", color: "#86909C", width: 1 };
