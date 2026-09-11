import { useEffect, useMemo, useRef, useState } from "react";

import type { DocumentSearchMatch, DocumentSearchOptions } from "@open-office/document-engine";

import type { BlockSessionApi } from "../hooks/useBlockSession.js";
import { applyDomTextSelection } from "../interaction/domSelection.js";

interface FindProps {
  session: BlockSessionApi;
  revision: number;
  onClose: () => void;
}

export function DocumentFindBar({ session, revision, onClose }: FindProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [replacement, setReplacement] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [cursor, setCursor] = useState(-1);
  const [notice, setNotice] = useState("");
  const options = useMemo<DocumentSearchOptions>(
    () => ({ caseSensitive, wholeWord }),
    [caseSensitive, wholeWord],
  );
  const matches = useMemo(
    () => query ? session.findText(query, options) : [],
    [options, query, revision, session.findText],
  );

  useEffect(() => { inputRef.current?.focus(); }, []);
  useEffect(() => {
    setCursor(matches.length ? 0 : -1);
    setNotice("");
  }, [matches.length, query, caseSensitive, wholeWord]);
  useEffect(() => {
    if (cursor >= 0 && matches[cursor]) revealDocumentMatch(matches[cursor], session.setActiveBlock);
  }, [cursor, matches, session.setActiveBlock]);

  const move = (delta: -1 | 1) => {
    if (!matches.length) return;
    setCursor((current) => current < 0
      ? (delta > 0 ? 0 : matches.length - 1)
      : (current + delta + matches.length) % matches.length);
  };
  const replaceCurrent = () => {
    const matched = matches[cursor];
    if (!matched || !query) return;
    if (session.replaceTextMatch(matched, query, replacement, options)) {
      setNotice("已替换当前匹配");
    }
  };
  const replaceAll = () => {
    if (!matches.length || !query) return;
    const count = matches.length;
    if (session.replaceAllText(query, replacement, options)) {
      setNotice(`已替换 ${count} 处`);
    }
  };

  return (
    <section className="document-find" aria-label="查找和替换">
      <input
        ref={inputRef}
        type="search"
        value={query}
        placeholder="查找"
        aria-label="查找内容"
        onChange={(event) => setQuery(event.target.value)}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === "Enter") { event.preventDefault(); move(event.shiftKey ? -1 : 1); }
          if (event.key === "Escape") { event.preventDefault(); onClose(); }
        }}
      />
      <output aria-live="polite">{query ? `${cursor < 0 ? 0 : cursor + 1}/${matches.length}` : "0/0"}</output>
      <button type="button" disabled={!matches.length} onClick={() => move(-1)} aria-label="上一个匹配">↑</button>
      <button type="button" disabled={!matches.length} onClick={() => move(1)} aria-label="下一个匹配">↓</button>
      <input
        value={replacement}
        placeholder="替换为"
        aria-label="替换内容"
        onChange={(event) => setReplacement(event.target.value)}
      />
      <button type="button" disabled={cursor < 0} onClick={replaceCurrent}>替换</button>
      <button type="button" disabled={!matches.length} onClick={replaceAll}>全部替换</button>
      <label><input type="checkbox" checked={caseSensitive} onChange={(event) => setCaseSensitive(event.target.checked)} />区分大小写</label>
      <label><input type="checkbox" checked={wholeWord} onChange={(event) => setWholeWord(event.target.checked)} />全字匹配</label>
      <span className="document-find__notice" role="status">{notice}</span>
      <button type="button" onClick={onClose} aria-label="关闭查找">×</button>
    </section>
  );
}

interface TocProps {
  session: BlockSessionApi;
  revision: number;
  onClose: () => void;
}

export function DocumentToc({ session, revision, onClose }: TocProps) {
  const items = useMemo(() => session.tableOfContents(), [revision, session.tableOfContents]);
  return (
    <aside className="document-toc" aria-label="文档目录">
      <header><strong>目录</strong><button type="button" onClick={onClose} aria-label="关闭目录">×</button></header>
      {items.length ? (
        <ol>
          {items.map((item) => (
            <li key={item.blockId} style={{ paddingLeft: `${(item.level - 1) * 14}px` }}>
              <button type="button" onClick={() => revealDocumentBlock(item.blockId, session.setActiveBlock)}>
                {item.text || "未命名标题"}
              </button>
            </li>
          ))}
        </ol>
      ) : <p>文档中还没有标题</p>}
    </aside>
  );
}

export function revealDocumentBlock(blockId: string, setActiveBlock: (id: string) => void): boolean {
  const row = document.querySelector<HTMLElement>(`[data-block-id="${CSS.escape(blockId)}"]`);
  if (!row) return false;
  row.scrollIntoView({ block: "center", behavior: "smooth" });
  row.querySelector<HTMLElement>(".block-row__content[contenteditable='true']")?.focus();
  setActiveBlock(blockId);
  return true;
}

export function revealDocumentMatch(
  matched: DocumentSearchMatch,
  setActiveBlock: (id: string) => void,
): boolean {
  const target = matched.target.type === "tableCell"
    ? document.querySelector<HTMLElement>(
        `[data-block-id="${CSS.escape(matched.target.blockId)}"] [data-table-row-id="${CSS.escape(matched.target.rowId)}"][data-table-cell-id="${CSS.escape(matched.target.cellId)}"]`,
      )
    : document.querySelector<HTMLElement>(
        `[data-block-id="${CSS.escape(matched.target.blockId)}"] .block-row__content[contenteditable='true']`,
      );
  if (!target) return false;
  target.scrollIntoView({ block: "center", behavior: "smooth" });
  target.focus();
  applyDomTextSelection(target, { start: matched.start, end: matched.end });
  setActiveBlock(matched.target.blockId);
  return true;
}
