import { useRef } from "react";

import { Icon } from "@open-office/ui";

export type ColorValue = string | null;
export type ColorRole = "text" | "highlight";

interface ColorPaletteProps {
  role: ColorRole;
  selectedColor: ColorValue;
  recentColors: readonly string[];
  onSelect: (color: ColorValue) => void;
}

/**
 * Tencent Docs-style color surface shared by text color and highlight color.
 * The palette is a view only: selection is handed back to the editor session
 * by the toolbar, so this component never mutates document state itself.
 */
export function ColorPalette({ role, selectedColor, recentColors, onSelect }: ColorPaletteProps) {
  const customColorRef = useRef<HTMLInputElement>(null);
  const label = role === "text" ? "字体颜色" : "文字高亮";

  return (
    <div className="toolbar-color-panel" role="menu" aria-label={`${label}颜色选择`}>
      <button
        className="toolbar-color-default"
        type="button"
        role="menuitemradio"
        aria-checked={selectedColor === null}
        onClick={() => onSelect(null)}
      >
        默认
      </button>

      <div className="toolbar-color-section-title">主题色</div>
      <div className="toolbar-color-theme-grid" role="group" aria-label="主题色">
        {THEME_COLORS.map((color, index) => (
          <ColorSwatch
            key={`theme-${index}`}
            color={color}
            selected={selectedColor === color}
            onSelect={onSelect}
          />
        ))}
      </div>

      <div className="toolbar-color-section-title">标准色</div>
      <div className="toolbar-color-standard-row" role="group" aria-label="标准色">
        {STANDARD_COLORS.map((color) => (
          <ColorSwatch key={color} color={color} selected={selectedColor === color} onSelect={onSelect} />
        ))}
      </div>

      <div className="toolbar-color-section-title">最近使用</div>
      <div className="toolbar-color-recent-row" role="group" aria-label="最近使用">
        {recentColors.length === 0 ? (
          <span className="toolbar-color-empty" aria-label="暂无最近使用颜色">暂无</span>
        ) : recentColors.slice(0, 10).map((color) => (
          <ColorSwatch key={`recent-${color}`} color={color} selected={selectedColor === color} onSelect={onSelect} />
        ))}
      </div>

      <button
        className="toolbar-color-more"
        type="button"
        role="menuitem"
        onClick={() => customColorRef.current?.click()}
      >
        <span className="toolbar-color-spectrum" aria-hidden="true" />
        <span>更多颜色</span>
        <Icon name="arrow-right" />
      </button>
      <input
        ref={customColorRef}
        className="toolbar-color-native-input"
        type="color"
        aria-label="选择自定义颜色"
        onChange={(event) => onSelect(event.currentTarget.value)}
      />
    </div>
  );
}

function ColorSwatch({ color, selected, onSelect }: { color: string; selected: boolean; onSelect: (color: string) => void }) {
  return (
    <button
      className="toolbar-color-swatch"
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      aria-label={color}
      title={color}
      style={{ backgroundColor: color }}
      onClick={() => onSelect(color)}
    />
  );
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
