import { useLayoutEffect, useRef, useState } from "react";

export type RowMenuAction = "cut" | "copy" | "paste" | "pasteValues" | "pasteFormats" | "insertAbove" | "insertBelow" | "hide" | "unhide" | "height" | "autoHeight" | "delete" | "format" | "merge" | "clearContents" | "clearFormats" | "clearAll" | "link";

export function SpreadsheetRowMenu({ x, y, startRow, endRow, busy, canPaste, canMerge, onAction, onClose }: {
  x: number; y: number; startRow: number; endRow: number; busy: boolean; canPaste: boolean; canMerge: boolean;
  onAction: (action: RowMenuAction, count?: number) => void; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [count, setCount] = useState(String(endRow - startRow + 1));
  const [position, setPosition] = useState({ left: x, top: y });
  const valid = /^\d+$/.test(count) && Number(count) >= 1 && Number(count) <= 1000;
  useLayoutEffect(() => {
    const menu = ref.current!;
    const place = () => { const rect = menu.getBoundingClientRect(); setPosition({ left: Math.max(8, Math.min(x, window.innerWidth - rect.width - 8)), top: Math.max(8, Math.min(y, window.innerHeight - rect.height - 8)) }); };
    place(); menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    const observer = new ResizeObserver(place); observer.observe(menu);
    window.addEventListener("resize", place);
    return () => { observer.disconnect(); window.removeEventListener("resize", place); };
  }, [x, y]);
  const item = (action: RowMenuAction, label: string, shortcut?: string, disabled = false) => <button type="button" role="menuitem" aria-label={label} disabled={busy || disabled} onClick={() => onAction(action, Number(count))}><span>{label}</span>{shortcut && <kbd>{shortcut}</kbd>}</button>;
  return <>
    <div className="ss__ctxmenu-overlay" onPointerDown={onClose} onContextMenu={event => { event.preventDefault(); onClose(); }} />
    <div ref={ref} className="ss__ctxmenu ss__rowmenu" role="menu" aria-label="行操作" style={position} onContextMenu={event => event.preventDefault()} onKeyDown={event => {
      event.stopPropagation();
      if (event.key === "Escape") { event.preventDefault(); onClose(); }
      if ((event.target as HTMLElement).tagName === "INPUT") return;
      if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
        event.preventDefault(); const items = Array.from(ref.current!.querySelectorAll<HTMLElement>('button:not(:disabled), summary, input')).filter(item => item.getClientRects().length > 0); const index = items.indexOf(document.activeElement as HTMLElement);
        items[event.key === "Home" ? 0 : event.key === "End" ? items.length - 1 : (index + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
      }
    }}>
      <div className="ss__rowmenu-caption">第 {startRow + 1}{endRow !== startRow ? `–${endRow + 1}` : ""} 行</div>
      {item("cut", "剪切", "⌘/Ctrl X")}{item("copy", "复制", "⌘/Ctrl C")}{item("paste", "粘贴", "⌘/Ctrl V", !canPaste)}
      <details><summary>选择性粘贴</summary>{item("pasteValues", "仅粘贴值", undefined, !canPaste)}{item("pasteFormats", "仅粘贴格式", undefined, !canPaste)}</details>
      <hr />
      <label className="ss__rowmenu-count">插入行数<input aria-label="插入行数" type="number" min="1" max="1000" value={count} onChange={event => setCount(event.target.value)} /><span>行</span></label>
      {item("insertAbove", "在上方插入", undefined, !valid)}{item("insertBelow", "在下方插入", undefined, !valid)}
      {item("hide", "隐藏行")}{item("unhide", "取消隐藏行")}
      {item("height", "设置行高…")}{item("autoHeight", "自动调整行高")}{item("delete", "删除所在行")}
      <hr />{item("format", "设置单元格格式…")}{item("merge", "合并单元格", undefined, !canMerge)}
      <details><summary>清除</summary>{item("clearContents", "清除内容")}{item("clearFormats", "清除格式")}{item("clearAll", "全部清除")}</details>
      <hr />{item("link", `获取指向此范围的链接（${startRow + 1}:${endRow + 1}）`)}
    </div>
  </>;
}
