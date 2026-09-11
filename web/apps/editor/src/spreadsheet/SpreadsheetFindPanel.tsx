import { useMemo, useState } from "react";
import type { CellModel, GridRange } from "@open-office/schema/artifact";
import { cellRef } from "@open-office/spreadsheet-ui";

export function SpreadsheetFindPanel({ cells, selection, saving, onSelect, onReplace, onClose }: {
  cells: CellModel[]; selection: GridRange | null; saving: boolean;
  onSelect: (cell: { row: number; column: number }) => void;
  onReplace: (cells: CellModel[], search: string, replacement: string, matchCase: boolean, exact: boolean) => Promise<boolean>;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<"find" | "replace">("find");
  const [search, setSearch] = useState("");
  const [replacement, setReplacement] = useState("");
  const [matchCase, setMatchCase] = useState(false);
  const [exact, setExact] = useState(false);
  const [scope, setScope] = useState("sheet");
  const [originalSelection] = useState(selection);
  const [index, setIndex] = useState(-1);
  const [notice, setNotice] = useState("");
  const matches = useMemo(() => {
    if (!search) return [];
    const needle = matchCase ? search : search.toLowerCase();
    return cells.filter(cell => {
      if (cell.formula || cell.value == null) return false;
      if (scope === "selection" && originalSelection && (cell.row < originalSelection.startRow || cell.row > originalSelection.endRow || cell.column < originalSelection.startColumn || cell.column > originalSelection.endColumn)) return false;
      const text = matchCase ? String(cell.value) : String(cell.value).toLowerCase();
      return exact ? text === needle : text.includes(needle);
    }).sort((a, b) => a.row - b.row || a.column - b.column);
  }, [cells, search, matchCase, exact, scope, originalSelection]);
  const navigate = (direction: number) => {
    if (!matches.length) return;
    const next = index < 0 ? (direction > 0 ? 0 : matches.length - 1) : (index + direction + matches.length) % matches.length;
    setIndex(next); onSelect(matches[next]!); setNotice("");
  };
  const replace = async (all: boolean) => {
    const targets = all ? matches : matches.length ? [matches[Math.max(0, Math.min(index, matches.length - 1))]!] : [];
    if (!targets.length) return;
    if (await onReplace(targets, search, replacement, matchCase, exact)) { setNotice(`已替换 ${targets.length} 个单元格`); setIndex(-1); }
  };
  return <div className="ss__find-replace" role="dialog" aria-label="查找替换" onKeyDown={event => {
    event.stopPropagation();
    if (event.key === "Escape") onClose();
    if (event.key === "Enter" && event.target instanceof HTMLInputElement) { event.preventDefault(); navigate(event.shiftKey ? -1 : 1); }
  }}>
    <div className="ss__find-heading"><div role="tablist" aria-label="查找与替换">
      <button type="button" role="tab" aria-selected={tab === "find"} onClick={() => setTab("find")}>查找</button>
      <button type="button" role="tab" aria-selected={tab === "replace"} onClick={() => setTab("replace")}>替换</button>
    </div><button type="button" aria-label="关闭查找替换" onClick={onClose}>×</button></div>
    <label>查找内容<input autoFocus value={search} placeholder="输入查找内容" onChange={event => { setSearch(event.target.value); setIndex(-1); setNotice(""); }} /></label>
    {tab === "replace" && <label>替换为<input value={replacement} placeholder="输入替换内容" onChange={event => setReplacement(event.target.value)} /></label>}
    <label>搜索范围<select value={scope} onChange={event => { setScope(event.target.value); setIndex(-1); }}><option value="sheet">当前工作表</option><option value="selection" disabled={!originalSelection}>选定区域</option></select></label>
    <div className="ss__find-options"><label><input type="checkbox" checked={matchCase} onChange={event => { setMatchCase(event.target.checked); setIndex(-1); }} />区分大小写</label><label><input type="checkbox" checked={exact} onChange={event => { setExact(event.target.checked); setIndex(-1); }} />单元格完全匹配</label></div>
    <div className="ss__find-notice" role="status">{notice || (search ? matches.length ? `${index < 0 ? 0 : Math.min(index + 1, matches.length)} / ${matches.length} 个匹配${index >= 0 && matches[index] ? ` · ${cellRef(matches[index]!.row, matches[index]!.column)}` : ""}` : "没有找到匹配的文本" : "搜索单元格内容（不包含公式）")}</div>
    <div className="ss__find-actions"><button type="button" disabled={!matches.length} onClick={() => navigate(-1)}>上一个</button><button type="button" disabled={!matches.length} onClick={() => navigate(1)}>下一个</button>{tab === "replace" && <><button type="button" disabled={saving || !matches.length} onClick={() => void replace(false)}>替换</button><button type="button" disabled={saving || !matches.length} onClick={() => void replace(true)}>全部替换</button></>}</div>
  </div>;
}
