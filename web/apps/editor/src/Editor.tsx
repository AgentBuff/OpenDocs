/**
 * Document 编辑器壳层。
 *
 * 文档正文是 Block Tree 的 DOM 视图，编辑动作通过 `DocumentCommand` 进入会话；
 * 组件只负责顶栏、工具栏和页面布局，不再维护第二套 Paragraph/Canvas 状态。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";

import type { ArtifactPageSetup, DocumentBlockKind } from "@open-office/schema/artifact";
import { useThemeRuntime } from "@open-office/ui";

import { api } from "./api.js";
import type { DocumentSnapshotMeta } from "./api.js";
import { action, resolveShortcut } from "./actions/registry.js";
import type { EditorActionContext } from "./actions/registry.js";
import { BlockEditor } from "./blockEditor.js";
import { HomeIcon, StarIcon, ThemeIcon } from "./icons/index.js";
import { useBlockSession } from "./hooks/useBlockSession.js";
import { BlockToolbar } from "./chrome/BlockToolbar.js";
import { DEFAULT_PAGE_SETUP } from "./chrome/PageSetupPanel.js";
import { selectDocumentText, isDocumentWideSelection } from "./utils/selection.js";
import { focusBlock } from "./blocks/focus.js";
import { formatDate } from "./utils/date.js";
import { EMPTY_TOOLBAR_SELECTION, readToolbarSelectionState, type ToolbarSelectionState } from "./toolbar/selectionState.js";
import { useBlockProjection, useBlockProjectionStructure } from "./store/blockProjectionStore.js";

interface Props {
  id: string;
  title: string;
  onBack: () => void;
}

export function Editor({ id, title, onBack }: Props) {
  const { theme, toggleTheme } = useThemeRuntime();
  const session = useBlockSession(id);
  const projectionStructure = useBlockProjectionStructure(session.projection);
  const projectedActiveBlock = useBlockProjection(session.projection, session.state.activeBlockId ?? "");
  const [docTitle, setDocTitle] = useState(title);
  const [starred, setStarred] = useState(false);
  const [snapshots, setSnapshots] = useState<DocumentSnapshotMeta[]>([]);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [restoringVersion, setRestoringVersion] = useState<number | null>(null);
  const formatPainterRef = useRef<Record<string, unknown> | null>(null);
  const [formatPainterActive, setFormatPainterActive] = useState(false);
  const [pageSetupOpen, setPageSetupOpen] = useState(false);
  const [toolbarSelection, setToolbarSelection] = useState<ToolbarSelectionState>(EMPTY_TOOLBAR_SELECTION);

  useEffect(() => {
    const updateToolbarSelection = () => setToolbarSelection(readToolbarSelectionState());
    document.addEventListener("selectionchange", updateToolbarSelection);
    updateToolbarSelection();
    return () => document.removeEventListener("selectionchange", updateToolbarSelection);
  }, []);

  useEffect(() => {
    setDocTitle(title);
    let cancelled = false;
    void api.getMeta(id).then((meta) => {
      if (cancelled) return;
      setDocTitle(meta.title);
      setStarred(meta.starred);
    }).catch(session.reportError);
    return () => {
      cancelled = true;
    };
  }, [id, session.reportError, title]);

  // Block content is subscribed through the projection store. This keeps a local text change
  // from making the editor walk the complete snapshot just to resolve the active block.
  const activeBlock = projectedActiveBlock;

  // Insertion is still valid before the first caret focus. Use the last root
  // block as the deterministic target so the toolbar never looks inert on a
  // freshly opened document.
  const insertTarget = useMemo(() => {
    if (activeBlock) return activeBlock;
    const lastId = projectionStructure.root.length > 0
      ? projectionStructure.root[projectionStructure.root.length - 1]
      : null;
    return lastId ? session.projection.getBlock(lastId) : null;
  }, [activeBlock, projectionStructure.root, session.projection]);

  const pageSetup = projectionStructure.pageSetup;

  const updatePageSetup = useCallback((patch: Partial<ArtifactPageSetup>) => {
    const next = { ...DEFAULT_PAGE_SETUP, ...pageSetup, ...patch };
    next.marginLeft = Math.min(next.marginLeft, Math.max(0, next.width - next.marginRight - 1));
    next.marginRight = Math.min(next.marginRight, Math.max(0, next.width - next.marginLeft - 1));
    next.marginTop = Math.min(next.marginTop, Math.max(0, next.height - next.marginBottom - 1));
    next.marginBottom = Math.min(next.marginBottom, Math.max(0, next.height - next.marginTop - 1));
    session.setPageSetup(next);
  }, [pageSetup, session]);

  const commitTitle = useCallback((next: string) => {
    const value = next.trim();
    if (!value || value === docTitle) {
      setDocTitle(docTitle);
      return;
    }
    setDocTitle(value);
    void api.patch(id, { title: value }).catch(session.reportError);
  }, [docTitle, id, session.reportError]);

  const toggleStar = useCallback(() => {
    const next = !starred;
    setStarred(next);
    void api.patch(id, { starred: next }).catch((error) => {
      setStarred(!next);
      session.reportError(error);
    });
  }, [id, session.reportError, starred]);

  const refreshHistory = useCallback(async () => {
    setHistoryLoading(true);
    try {
      setSnapshots(await api.listSnapshots(id));
    } catch (error) {
      session.reportError(error);
    } finally {
      setHistoryLoading(false);
    }
  }, [id, session.reportError]);

  const toggleHistory = useCallback(() => {
    const next = !historyOpen;
    setHistoryOpen(next);
    if (next) void refreshHistory();
  }, [historyOpen, refreshHistory]);

  const restoreVersion = useCallback(async (version: number) => {
    if (version === session.state.revision) return;
    setRestoringVersion(version);
    try {
      await api.restoreSnapshot(id, version, session.state.revision);
      await session.reload();
      await refreshHistory();
    } catch (error) {
      session.reportError(error);
    } finally {
      setRestoringVersion(null);
    }
  }, [id, refreshHistory, session]);

  const applyKind = useCallback((kind: DocumentBlockKind) => {
    if (activeBlock) session.convertBlock(activeBlock.id, kind);
  }, [activeBlock, session]);

  const insertCodeBlock = useCallback(() => {
    if (insertTarget) session.insertAfter(insertTarget.id, { type: "code" });
  }, [insertTarget, session]);

  const insertQuoteBlock = useCallback(() => {
    if (insertTarget) session.insertAfter(insertTarget.id, { type: "quote" });
  }, [insertTarget, session]);

  const insertCalloutBlock = useCallback(() => {
    if (insertTarget) session.insertAfter(insertTarget.id, { type: "callout" });
  }, [insertTarget, session]);

  const insertDivider = useCallback(() => {
    if (insertTarget) session.insertAfter(insertTarget.id, { type: "divider" });
  }, [insertTarget, session]);

  const insertTodoBlock = useCallback(() => {
    if (insertTarget) session.insertAfter(insertTarget.id, { type: "todo" });
  }, [insertTarget, session]);

  const actionContext = useMemo<EditorActionContext>(() => ({
    hasActiveBlock: activeBlock !== null,
    hasTextSelection: toolbarSelection.hasTextSelection,
    canUndo: session.state.canUndo,
    canRedo: session.state.canRedo,
    onBold: () => session.toggleMark("bold"),
    onItalic: () => session.toggleMark("italic"),
    onUnderline: () => session.toggleMark("underline"),
    onStrike: () => session.toggleMark("strikethrough"),
    onFormatPainter: () => {
      if (formatPainterRef.current) {
        session.setInlineAttrs(formatPainterRef.current);
        formatPainterRef.current = null;
        setFormatPainterActive(false);
      } else {
        const captured = session.captureInlineAttrs();
        formatPainterRef.current = captured;
        setFormatPainterActive(Boolean(captured));
      }
    },
    onClearFormat: () => session.setInlineAttrs({
      bold: null,
      italic: null,
      underline: null,
      strikethrough: null,
      fontFamily: null,
      fontSize: null,
      color: null,
      highlight: null,
    }),
    onFontSizeAdjust: (delta) => session.adjustFontSize(delta),
    onKind: applyKind,
    onAlignment: (align) => activeBlock && session.setBlockPresentation(activeBlock.id, { align }),
    onList: (type) => {
      if (!activeBlock) return;
      const current = activeBlock.presentation.list?.kind ?? null;
      session.setBlockPresentation(activeBlock.id, { listType: current === type ? null : type });
    },
    onTodo: () => {
      if (!activeBlock) return;
      if (activeBlock.kind.type === "todo") {
        session.convertBlock(activeBlock.id, { type: "paragraph" });
        return;
      }
      session.convertBlock(activeBlock.id, { type: "todo" });
      session.setBlockPresentation(activeBlock.id, { listType: null });
    },
    onIndent: (delta) => {
      if (!activeBlock) return;
      const current = typeof activeBlock.presentation.indentStart === "number" ? activeBlock.presentation.indentStart : 0;
      session.setBlockPresentation(activeBlock.id, { indentLevel: Math.max(0, Math.min(20, current + delta)) });
    },
    onLink: () => {
      if (!insertTarget) return;
      const currentUrl = insertTarget.data.type === "link" ? insertTarget.data.data.url : "";
      const url = window.prompt("链接地址", currentUrl || "https://");
      if (!url?.trim()) return;
      if (insertTarget.kind.type === "link") session.setLinkTarget(insertTarget.id, url.trim());
      else session.convertToLink(insertTarget.id, url.trim());
    },
    onInsertTable: (rows = 2, columns = 2) => {
      if (insertTarget) session.insertTableAfter(insertTarget.id, rows, columns);
    },
    onInsert: () => insertTarget && session.insertAfter(insertTarget.id),
    onDelete: () => activeBlock && session.deleteBlock(activeBlock.id),
    onSelectAll: selectDocumentText,
    onDeleteSelection: session.clearDocument,
    onUndo: session.undo,
    onRedo: session.redo,
  }), [activeBlock, applyKind, insertTarget, session, toolbarSelection.hasTextSelection]);

  const handleShortcut = useCallback((event: ReactKeyboardEvent<HTMLDivElement>) => {
    const isSelectAll = event.key.toLowerCase() === "a"
      && (event.metaKey || event.ctrlKey)
      && !event.altKey
      && !event.shiftKey;
    if (isSelectAll && event.target instanceof HTMLInputElement) {
      event.preventDefault();
      event.target.select();
      return;
    }
    if (isSelectAll && event.target instanceof HTMLTextAreaElement) {
      event.preventDefault();
      event.target.select();
      return;
    }
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
    if ((event.key === "Backspace" || event.key === "Delete") && isDocumentWideSelection()) {
      event.preventDefault();
      window.getSelection()?.removeAllRanges();
      const focusedElement = document.activeElement;
      if (focusedElement instanceof HTMLElement && focusedElement.closest(".block-editor__page")) focusedElement.blur();
      action("deleteSelection").run(actionContext);
      return;
    }
    if (event.key === "Backspace" || event.key === "Delete") {
      const deletedBlockId = session.deleteTextSelection();
      if (deletedBlockId) {
        event.preventDefault();
        window.getSelection()?.removeAllRanges();
        requestAnimationFrame(() => focusBlock(deletedBlockId));
        return;
      }
    }
    const id = resolveShortcut(event.nativeEvent);
    if (!id) return;
    const registered = action(id);
    if (!registered.isEnabled(actionContext)) return;
    event.preventDefault();
    registered.run(actionContext);
  }, [actionContext]);

  const statusText = session.state.saving ? "正在保存…" : session.state.dirty ? "有未保存的更改" : "所有更改已保存";

  return (
    <div className="editor" onKeyDownCapture={handleShortcut}>
      <header className="editor__topbar">
        <button className="btn btn--ghost" onClick={onBack} title="返回文档列表"><HomeIcon /></button>
        <input
          className="editor__title"
          value={docTitle}
          onChange={(event) => setDocTitle(event.target.value)}
          onBlur={(event) => commitTitle(event.target.value)}
          onKeyDown={(event) => { if (event.key === "Enter") event.currentTarget.blur(); }}
          aria-label="文档标题"
        />
        <button className={`editor__star${starred ? " is-on" : ""}`} onClick={toggleStar} title={starred ? "取消收藏" : "收藏"}>
          <StarIcon filled={starred} />
        </button>
        <span className={`editor__status${session.state.saving ? " is-saving" : session.state.dirty ? " is-dirty" : ""}`}>{statusText}</span>
        <button
          className="editor__theme-toggle"
          type="button"
          onClick={toggleTheme}
          aria-pressed={theme === "office-dark"}
          aria-label={theme === "office-dark" ? "切换浅色主题" : "切换深色主题"}
          title={theme === "office-dark" ? "切换浅色主题" : "切换深色主题"}
        >
          <ThemeIcon dark={theme === "office-dark"} />
        </button>
        <span className="editor__spacer" />
      </header>

      {historyOpen && (
        <aside className="history-panel" aria-label="历史版本">
          <div className="history-panel__head">
            <strong>历史版本</strong>
            <button className="btn btn--ghost btn--sm" onClick={() => setHistoryOpen(false)}>关闭</button>
          </div>
          {historyLoading ? <p className="history-panel__empty">正在加载…</p> : snapshots.length === 0 ? (
            <p className="history-panel__empty">暂无已保存版本</p>
          ) : (
            <ul className="history-panel__list">
              {snapshots.map((snapshot) => (
                <li key={snapshot.version}>
                  <button
                    className={`history-panel__item${snapshot.current ? " is-current" : ""}`}
                    disabled={snapshot.current || restoringVersion !== null}
                    onClick={() => void restoreVersion(snapshot.version)}
                  >
                    <span>版本 {snapshot.version}{snapshot.current ? "（当前）" : ""}</span>
                    <time dateTime={snapshot.createdAt}>{formatDate(snapshot.createdAt)}</time>
                    {restoringVersion === snapshot.version && <small>恢复中…</small>}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>
      )}

      <BlockToolbar
        activeKind={activeBlock?.kind ?? { type: "paragraph" }}
        lineHeight={typeof activeBlock?.presentation.lineHeight === "number" && Number.isFinite(activeBlock.presentation.lineHeight) ? activeBlock.presentation.lineHeight : undefined}
        actionContext={actionContext}
        formatPainterActive={formatPainterActive}
        insertEnabled={insertTarget !== null}
        onInsertCode={insertCodeBlock}
        onInsertQuote={insertQuoteBlock}
        onInsertCallout={insertCalloutBlock}
        onInsertTodo={insertTodoBlock}
        onInsertDivider={insertDivider}
        onInlineAttrs={session.setInlineAttrs}
        onBlockPresentation={(attrs) => activeBlock && session.setBlockPresentation(activeBlock.id, attrs)}
        onHistory={toggleHistory}
        historyOpen={historyOpen}
        exportHref={api.exportDocx(id)}
        onPageSetupOpenChange={setPageSetupOpen}
        pageSetupOpen={pageSetupOpen}
        pageSetup={{ ...DEFAULT_PAGE_SETUP, ...pageSetup }}
        onPageSetupChange={updatePageSetup}
        onSave={() => void session.save()}
        saveDisabled={!session.state.dirty || session.state.saving}
        paragraphSettings={{
          align: activeBlock?.presentation.align ?? "left",
          indentLevel: activeBlock?.presentation.indentStart ?? 0,
          indentRight: activeBlock?.presentation.indentEnd ?? 0,
          spacingBefore: activeBlock?.presentation.spacingBefore ?? 0,
          spacingAfter: activeBlock?.presentation.spacingAfter ?? 0,
          lineHeight: activeBlock?.presentation.lineHeight ?? 1,
        }}
        toolbarSelection={toolbarSelection}
        activePresentation={{
          align: activeBlock?.presentation.align ?? "left",
          list: activeBlock?.presentation.list?.kind ?? null,
        }}
      />

      {session.state.error && <p className="alert">{session.state.error}</p>}

      <main className="editor__surface editor__surface--blocks">
        <BlockEditor session={session} />
      </main>

      <footer className="editor__statusbar">
        <span>{session.state.loading ? "正在加载…" : `${session.state.wordCount} 个字`}</span>
        <span>{session.state.blockCount} 个块</span>
        <span>revision {session.state.revision}</span>
      </footer>
    </div>
  );
}
