import { useCallback, useDeferredValue, useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";

import type { CodeBlockConfig, DocumentBlock } from "@open-office/schema/artifact";
import { Icon, Popover, ToolbarSelect } from "@open-office/ui";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";
import {
  clampCodeBlockHeight,
  CODE_BLOCK_MAX_HEIGHT,
  CODE_BLOCK_MIN_HEIGHT,
  CODE_LANGUAGES,
  CODE_THEMES,
  codeLineNumbers,
  codeConfig,
  isCurrentHighlightRequest,
  indentText,
  tokenizeCode,
  type CodeToken,
} from "./codeConfig.js";

const WORKER_THRESHOLD = 4_096;

function useCodeHighlight(source: string, language: CodeBlockConfig["language"]): CodeToken[] {
  const deferredSource = useDeferredValue(source);
  // Do not synchronously tokenize a large initial payload during mount. The
  // textarea must be interactive first; the worker will replace this plain
  // fallback once the derived highlight is ready.
  const [tokens, setTokens] = useState<CodeToken[]>(() => source.length < WORKER_THRESHOLD ? tokenizeCode(source, language) : [{ text: source }]);
  const workerRef = useRef<Worker | null>(null);
  const requestRef = useRef(0);
  const disposedRef = useRef(false);

  useEffect(() => {
    disposedRef.current = false;
    const requestId = ++requestRef.current;
    if (deferredSource.length < WORKER_THRESHOLD) {
      setTokens(tokenizeCode(deferredSource, language));
      return;
    }

    if (typeof Worker === "undefined") {
      // Test/embedded WebViews may not expose module workers. Yield once so
      // mounting and typing are still responsive, then retain the same stale
      // response contract as the worker path.
      const fallback = globalThis.setTimeout(() => {
        if (isCurrentHighlightRequest(requestId, requestRef.current, disposedRef.current)) {
          setTokens(tokenizeCode(deferredSource, language));
        }
      }, 0);
      return () => {
        globalThis.clearTimeout(fallback);
        if (requestRef.current === requestId) requestRef.current += 1;
      };
    }

    const worker = workerRef.current ?? new Worker(new URL("./highlightWorker.ts", import.meta.url), { type: "module" });
    workerRef.current = worker;
    worker.onmessage = (event: MessageEvent<{ id: number; tokens: CodeToken[] }>) => {
      if (isCurrentHighlightRequest(event.data.id, requestRef.current, disposedRef.current)) setTokens(event.data.tokens);
    };
    worker.postMessage({ id: requestId, source: deferredSource, language });

    return () => {
      // Invalidate the request before React runs the next effect. A response
      // from an older source/language must never replace the current view.
      if (requestRef.current === requestId) requestRef.current += 1;
    };
  }, [deferredSource, language]);

  useEffect(() => () => {
    disposedRef.current = true;
    requestRef.current += 1;
    workerRef.current?.terminate();
    workerRef.current = null;
  }, []);

  return tokens;
}

export function CodeBlockView({ block, session }: { block: DocumentBlock; session: BlockSessionApi }) {
  const codeBlockRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const gutterRef = useRef<HTMLPreElement>(null);
  const source = block.content?.text ?? "";
  if (block.data.type !== "code") {
    throw new Error(`code block ${block.id} 缺少 code data`);
  }
  const config = codeConfig(block.data.data);
  const configuredHeight = clampCodeBlockHeight(config.height);
  const [moreOpen, setMoreOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [draftHeight, setDraftHeight] = useState(configuredHeight);
  const [resizing, setResizing] = useState(false);
  const sourceRef = useRef(source);
  const draftHeightRef = useRef(configuredHeight);
  const resizeRef = useRef<{ pointerId: number; startY: number; startHeight: number } | null>(null);
  const configRef = useRef(config);
  const sessionRef = useRef(session);
  const blockIdRef = useRef(block.id);
  sourceRef.current = source;
  configRef.current = config;
  sessionRef.current = session;
  blockIdRef.current = block.id;
  // Highlighting is derived UI state. Small snippets use the allocation-bounded tokenizer;
  // large snippets go through a module Worker so a full-source pass cannot block input/IME.
  const tokens = useCodeHighlight(source, config.language);
  const lineNumbers = codeLineNumbers(source);

  useEffect(() => {
    const element = textareaRef.current;
    if (!element || document.activeElement === element || element.value === source) return;
    element.value = source;
  }, [source]);

  useEffect(() => {
    if (!resizing) {
      draftHeightRef.current = configuredHeight;
      setDraftHeight(configuredHeight);
    }
  }, [configuredHeight, resizing]);

  const updateConfig = useCallback((patch: Partial<CodeBlockConfig>) => {
    const next: CodeBlockConfig = { ...config, ...patch };
    session.setCodeConfig(block.id, next);
  }, [block.id, config, session]);

  const commitSource = useCallback(() => {
    const value = textareaRef.current?.value ?? "";
    if (value === sourceRef.current) return;
    sourceRef.current = value;
    session.updateContent(block.id, { text: value, runs: [] });
  }, [block.id, session]);

  const copy = useCallback(async () => {
    const value = textareaRef.current?.value ?? sourceRef.current;
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      session.reportError("复制代码失败，请检查浏览器剪贴板权限");
    }
  }, [session]);

  const commitHeight = useCallback((value: number) => {
    const nextHeight = clampCodeBlockHeight(value);
    draftHeightRef.current = nextHeight;
    setDraftHeight(nextHeight);
    if (nextHeight === clampCodeBlockHeight(configRef.current.height)) return;
    sessionRef.current.setCodeConfig(blockIdRef.current, { ...configRef.current, height: nextHeight });
    void sessionRef.current.save();
  }, []);

  const handleResizePointerDown = useCallback((event: ReactPointerEvent<HTMLButtonElement>) => {
    event.preventDefault();
    resizeRef.current = { pointerId: event.pointerId, startY: event.clientY, startHeight: draftHeightRef.current };
    setResizing(true);
  }, []);

  const handleResizePointerMove = useCallback((event: ReactPointerEvent<HTMLButtonElement>) => {
    const resize = resizeRef.current;
    if (!resize || resize.pointerId !== event.pointerId) return;
    const nextHeight = clampCodeBlockHeight(resize.startHeight + event.clientY - resize.startY);
    draftHeightRef.current = nextHeight;
    setDraftHeight(nextHeight);
  }, []);

  const handleResizePointerUp = useCallback((event: ReactPointerEvent<HTMLButtonElement>) => {
    const resize = resizeRef.current;
    if (!resize || resize.pointerId !== event.pointerId) return;
    resizeRef.current = null;
    setResizing(false);
    commitHeight(draftHeightRef.current);
  }, [commitHeight]);

  useEffect(() => {
    if (!resizing) return;
    const onMove = (event: PointerEvent) => {
      const resize = resizeRef.current;
      if (!resize || resize.pointerId !== event.pointerId) return;
      const nextHeight = clampCodeBlockHeight(resize.startHeight + event.clientY - resize.startY);
      draftHeightRef.current = nextHeight;
      setDraftHeight(nextHeight);
    };
    const onUp = (event: PointerEvent) => {
      const resize = resizeRef.current;
      if (!resize || resize.pointerId !== event.pointerId) return;
      resizeRef.current = null;
      setResizing(false);
      commitHeight(draftHeightRef.current);
    };
    window.addEventListener("pointermove", onMove, true);
    window.addEventListener("pointerup", onUp, true);
    window.addEventListener("pointercancel", onUp, true);
    return () => {
      window.removeEventListener("pointermove", onMove, true);
      window.removeEventListener("pointerup", onUp, true);
      window.removeEventListener("pointercancel", onUp, true);
    };
  }, [commitHeight, resizing]);

  const handleResizeKeyDown = useCallback((event: ReactKeyboardEvent<HTMLButtonElement>) => {
    const increment = event.key === "PageUp" || event.key === "PageDown" ? 64 : 16;
    if (!["ArrowUp", "ArrowDown", "PageUp", "PageDown", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const nextHeight = event.key === "Home"
      ? CODE_BLOCK_MIN_HEIGHT
      : event.key === "End"
        ? CODE_BLOCK_MAX_HEIGHT
        : draftHeightRef.current + (event.key === "ArrowUp" || event.key === "PageUp" ? -increment : increment);
    commitHeight(nextHeight);
  }, [commitHeight]);

  return (
    <div ref={codeBlockRef} className={`code-block code-block--${config.theme}`} data-code-language={config.language}>
      <header className="code-block__toolbar">
        <input
          className="code-block__title"
          value={config.title}
          placeholder="请输入代码块名称"
          maxLength={256}
          aria-label="代码块标题"
          onChange={(event) => updateConfig({ title: event.target.value })}
        />
        <ToolbarSelect
          className="code-block__select code-block__select--language"
          popupClassName={`code-block__select-popover code-block__select-popover--${config.theme}`}
          value={config.language}
          aria-label="代码语言"
          options={CODE_LANGUAGES.map((language) => ({ value: language.id, label: language.label }))}
          onValueChange={(language) => updateConfig({ language })}
        />
        <ToolbarSelect
          className="code-block__select code-block__select--theme"
          popupClassName={`code-block__select-popover code-block__select-popover--${config.theme}`}
          value={config.theme}
          aria-label="代码主题"
          options={CODE_THEMES.map((theme) => ({ value: theme.id, label: theme.label }))}
          onValueChange={(theme) => updateConfig({ theme })}
        />
        <button className="code-block__tool code-block__tool--copy" type="button" onClick={() => void copy()} aria-label="复制代码" title={copied ? "已复制" : "复制代码"}>
          <Icon name={copied ? "check" : "copy"} />
          <span className="sr-only">{copied ? "已复制" : "复制代码"}</span>
        </button>
        <Popover
          open={moreOpen}
          onOpenChange={setMoreOpen}
          placement="bottom-end"
          role="presentation"
          popupClassName="oo-overlay--code-settings"
          content={(
            <div className="code-block__settings" role="menu" aria-label="代码块设置">
              <label><span>主题</span><ToolbarSelect
                className="code-block__settings-select"
                popupClassName={`code-block__select-popover code-block__select-popover--${config.theme}`}
                value={config.theme}
                aria-label="设置代码主题"
                options={CODE_THEMES.map((theme) => ({ value: theme.id, label: theme.label }))}
                onValueChange={(theme) => updateConfig({ theme })}
              /></label>
              <label><span>字号</span><input type="number" min={8} max={32} step={1} value={config.fontSize} onChange={(event) => {
                const value = Number(event.target.value);
                if (Number.isFinite(value)) updateConfig({ fontSize: Math.min(32, Math.max(8, Math.round(value))) });
              }} /></label>
              <label><span>缩进</span><ToolbarSelect
                className="code-block__settings-select"
                value={config.indentMode}
                aria-label="缩进模式"
                options={[{ value: "spaces", label: "空格" }, { value: "tabs", label: "Tab" }]}
                onValueChange={(indentMode) => updateConfig({ indentMode: indentMode as CodeBlockConfig["indentMode"] })}
              /></label>
              <label><span>缩进宽度</span><ToolbarSelect
                className="code-block__settings-select"
                value={String(config.indentWidth)}
                aria-label="缩进宽度"
                disabled={config.indentMode === "tabs"}
                options={[{ value: "2", label: "2" }, { value: "4", label: "4" }, { value: "8", label: "8" }]}
                onValueChange={(indentWidth) => updateConfig({ indentWidth: Number(indentWidth) as CodeBlockConfig["indentWidth"] })}
              /></label>
              <label><span>行号</span><input type="checkbox" checked={config.showLineNumbers} onChange={(event) => updateConfig({ showLineNumbers: event.target.checked })} /></label>
              <label><span>自动换行</span><input type="checkbox" checked={config.wrap} onChange={(event) => updateConfig({ wrap: event.target.checked })} /></label>
              <div className="code-block__settings-divider" role="separator" />
              <button
                className="code-block__settings-danger"
                type="button"
                role="menuitem"
                onClick={() => {
                  setMoreOpen(false);
                  session.deleteBlock(block.id);
                }}
              >
                <Icon name="delete" />
                <span>删除代码块</span>
              </button>
            </div>
          )}
        >
          <button className="code-block__tool" type="button" aria-label="代码块更多设置" title="更多设置"><Icon name="ellipsis" /></button>
        </Popover>
      </header>
      <div className="code-block__body" style={{ height: `${draftHeight}px`, fontSize: `${config.fontSize}px` }}>
        {config.showLineNumbers && <pre ref={gutterRef} className="code-block__gutter" aria-hidden="true">{lineNumbers}</pre>}
        <div className="code-block__editor">
          <pre className="code-block__highlight" aria-hidden="true" style={{ whiteSpace: config.wrap ? "pre-wrap" : "pre" }}>{tokens.map((token, index) => <span key={`${index}-${token.text}`} className={token.className}>{token.text}</span>)}</pre>
          <textarea
            ref={textareaRef}
            className="code-block__textarea"
            defaultValue={source}
            aria-label="代码内容"
            spellCheck={false}
            wrap={config.wrap ? "soft" : "off"}
            onInput={commitSource}
            onBlur={() => void session.save()}
            onScroll={(event) => {
              const target = event.currentTarget;
              const highlight = target.parentElement?.querySelector<HTMLElement>(".code-block__highlight");
              if (highlight) { highlight.scrollTop = target.scrollTop; highlight.scrollLeft = target.scrollLeft; }
              if (gutterRef.current) gutterRef.current.scrollTop = target.scrollTop;
            }}
            onKeyDown={(event) => {
              if (event.key !== "Tab") return;
              event.preventDefault();
              const element = event.currentTarget;
              const indentation = indentText(config);
              const start = element.selectionStart;
              const end = element.selectionEnd;
              element.setRangeText(indentation, start, end, "end");
              commitSource();
            }}
          />
        </div>
      </div>
      <button
        className={`code-block__resize-handle${resizing ? " is-resizing" : ""}`}
        type="button"
        role="separator"
        aria-orientation="horizontal"
        aria-label="调整代码块高度"
        aria-valuemin={CODE_BLOCK_MIN_HEIGHT}
        aria-valuemax={CODE_BLOCK_MAX_HEIGHT}
        aria-valuenow={draftHeight}
        onPointerDown={handleResizePointerDown}
        onPointerMove={handleResizePointerMove}
        onPointerUp={handleResizePointerUp}
        onPointerCancel={handleResizePointerUp}
        onKeyDown={handleResizeKeyDown}
      ><span aria-hidden="true" /></button>
    </div>
  );
}
