import { useId, useRef } from "react";
import { Icon } from "../icons/index.js";

export type ColorValue = string | null;
export type ColorPaletteRole = "text" | "highlight" | "fill" | "stroke";

export interface ColorPaletteProps {
  role: ColorPaletteRole;
  value: ColorValue;
  recentColors?: readonly string[];
  onValueChange: (value: ColorValue) => void;
  /** A palette does not own persistence; the product decides how to retain recents. */
  onCustomColor?: (value: string) => void;
}

const THEME_COLORS = [
  "#ffffff", "#1d2129", "#86909c", "#165dff", "#0fc6c2", "#00b42a", "#ff7d00", "#f53f3f", "#fadc19", "#722ed1",
  "#f7f8fa", "#4e5969", "#c9cdd4", "#94bfff", "#8fdfe0", "#95de64", "#ffcf8b", "#ffb7b2", "#fff1b8", "#d3adf7",
  "#f2f3f5", "#86909c", "#a9b0bb", "#bedaff", "#b7e8e8", "#b7eb8f", "#ffe4ba", "#ffccc7", "#fff7d1", "#e4c8f5",
  "#e5e6eb", "#4e5969", "#86909c", "#d6e4ff", "#c6f3d7", "#d9f7be", "#fff1b8", "#ffece8", "#fffbe6", "#f5e8ff",
  "#c9cdd4", "#272e3b", "#4e5969", "#94bfff", "#95de64", "#a7e8a3", "#ffcf8b", "#ff9c9c", "#ffe58f", "#b37feb",
  "#86909c", "#1d2129", "#272e3b", "#165dff", "#0fc6c2", "#00b42a", "#ff7d00", "#f53f3f", "#fadc19", "#722ed1",
] as const;

const STANDARD_COLORS = [
  "#f53f3f", "#ff7d00", "#fadc19", "#9cdc19", "#00b42a", "#14c9c2", "#0fc6c2", "#165dff", "#1d39c4", "#722ed1",
] as const;

export function ColorPalette({ role, value, recentColors = [], onValueChange, onCustomColor }: ColorPaletteProps) {
  const generatedId = useId();
  const customColorRef = useRef<HTMLInputElement>(null);
  const label = colorRoleLabel(role);
  const recent = [...new Set(recentColors)].slice(0, 10);

  const select = (next: ColorValue) => onValueChange(next);
  const selectCustom = (next: string) => {
    onCustomColor?.(next);
    select(next);
  };

  return (
    <div className="oo-color-palette" role="menu" aria-label={`${label}颜色选择`}>
      <button className="oo-color-palette__default" type="button" role="menuitemradio" aria-checked={value === null} onClick={() => select(null)}>默认</button>
      <ColorSection id={`${generatedId}-theme`} title="主题色" colors={THEME_COLORS} value={value} onValueChange={select} />
      <ColorSection id={`${generatedId}-standard`} title="标准色" colors={STANDARD_COLORS} value={value} onValueChange={select} compact />
      <div className="oo-color-palette__title">最近使用</div>
      <div className="oo-color-palette__recent" role="group" aria-label="最近使用颜色">
        {recent.length === 0
          ? <span className="oo-color-palette__empty">暂无</span>
          : recent.map((color) => <ColorSwatch key={color} color={color} selected={value === color} onSelect={select} />)}
      </div>
      <button className="oo-color-palette__more" type="button" role="menuitem" onClick={() => customColorRef.current?.click()}>
        <span className="oo-color-palette__spectrum" aria-hidden="true" />
        <span>更多颜色</span>
        <Icon name="arrow-right" />
      </button>
      <input
        ref={customColorRef}
        className="oo-color-palette__native-input"
        type="color"
        aria-label="选择自定义颜色"
        value={value ?? "#1d2129"}
        onChange={(event) => selectCustom(event.currentTarget.value)}
      />
    </div>
  );
}

function ColorSection({ id, title, colors, value, onValueChange, compact = false }: {
  id: string;
  title: string;
  colors: readonly string[];
  value: ColorValue;
  onValueChange: (value: string) => void;
  compact?: boolean;
}) {
  return <>
    <div className="oo-color-palette__title" id={id}>{title}</div>
    <div className={compact ? "oo-color-palette__standard" : "oo-color-palette__theme"} role="group" aria-labelledby={id}>
      {colors.map((color, index) => <ColorSwatch key={`${color}-${index}`} color={color} selected={value === color} onSelect={onValueChange} />)}
    </div>
  </>;
}

function ColorSwatch({ color, selected, onSelect }: { color: string; selected: boolean; onSelect: (color: string) => void }) {
  return <button
    className="oo-color-palette__swatch"
    type="button"
    role="menuitemradio"
    aria-checked={selected}
    aria-label={color}
    title={color}
    style={{ backgroundColor: color }}
    onClick={() => onSelect(color)}
  />;
}

function colorRoleLabel(role: ColorPaletteRole): string {
  switch (role) {
    case "text": return "字体";
    case "highlight": return "文字高亮";
    case "fill": return "填充";
    case "stroke": return "边框";
  }
}
