import { useCallback, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import type { InlineStylePatch, RichText, TextRange } from "@open-office/schema/artifact";

import { richTextFromHtml, richTextToDom } from "../blocks/richText.js";
import { applyDomTextSelection, readDomTextSelection } from "../interaction/domSelection.js";
import { preserveTextRuns } from "../typography/preserve-text-runs.js";

export function MindmapRichTextEditor({ nodeId, content, disabled, patchDisabled, onCommit, onPatch, onCancel }: {
  nodeId: string;
  content: RichText;
  disabled: boolean;
  patchDisabled: boolean;
  onCommit: (content: RichText) => void;
  onPatch: (draft: RichText, range: TextRange, patch: InlineStylePatch) => void;
  onCancel: () => void;
}) {
  const editorRef = useRef<HTMLDivElement>(null);
  const draftRef = useRef(content);
  const selectionRef = useRef<TextRange>({ start: 0, end: [...content.text].length });
  const composingRef = useRef(false);
  const blurredRef = useRef(false);
  const cancelledRef = useRef(false);
  const [selectionVersion, setSelectionVersion] = useState(0);

  useLayoutEffect(() => {
    const editor = editorRef.current;
    if (!editor || composingRef.current) return;
    draftRef.current = content;
    editor.replaceChildren(richTextToDom(content));
    editor.focus();
    applyDomTextSelection(editor, selectionRef.current);
  }, [content, nodeId]);

  const readSelection = useCallback(() => {
    const editor = editorRef.current;
    const selection = editor ? readDomTextSelection(editor, nodeId)?.range : null;
    if (selection) selectionRef.current = selection;
    setSelectionVersion((value) => value + 1);
    return selectionRef.current;
  }, [nodeId]);

  const commit = useCallback(() => {
    if (!cancelledRef.current) onCommit(normalizeNodeText(draftRef.current));
  }, [onCommit]);

  const patch = (field: "bold" | "italic" | "underline") => {
    const range = readSelection();
    if (range.start === range.end) return;
    const active = rangeHasStyle(draftRef.current, range, field);
    onPatch(draftRef.current, range, { [field]: !active });
  };

  return <div className="mindmap-rich-editor" onPointerDown={(event) => event.stopPropagation()}>
    <div className="mindmap-rich-editor__toolbar" role="toolbar" aria-label="所选主题文字格式">
      {(["bold", "italic", "underline"] as const).map((field) => <button
        key={field}
        type="button"
        aria-label={field === "bold" ? "加粗所选主题文字" : field === "italic" ? "倾斜所选主题文字" : "为所选主题文字添加下划线"}
        aria-pressed={rangeHasStyle(draftRef.current, selectionRef.current, field)}
        disabled={patchDisabled || selectionRef.current.start === selectionRef.current.end}
        onPointerDown={(event) => event.preventDefault()}
        onClick={() => patch(field)}
      >{field === "bold" ? "B" : field === "italic" ? "I" : "U"}</button>)}
    </div>
    <div
      ref={editorRef}
      className="mindmap-rich-editor__content"
      contentEditable={!disabled}
      suppressContentEditableWarning
      role="textbox"
      aria-label="主题文字"
      aria-multiline="false"
      data-selection-version={selectionVersion}
      onInput={(event) => {
        const parsed = richTextFromHtml(event.currentTarget);
        draftRef.current = parsed.runs.every((run) => isDefaultStyle(run.style))
          ? preserveTextRuns(draftRef.current, parsed.text)
          : parsed;
        readSelection();
      }}
      onSelect={readSelection}
      onKeyUp={readSelection}
      onPointerUp={readSelection}
      onCompositionStart={() => { composingRef.current = true; blurredRef.current = false; }}
      onCompositionEnd={(event) => {
        composingRef.current = false;
        const parsed = richTextFromHtml(event.currentTarget);
        draftRef.current = parsed.runs.every((run) => isDefaultStyle(run.style))
          ? preserveTextRuns(draftRef.current, parsed.text)
          : parsed;
        readSelection();
        if (blurredRef.current) commit();
      }}
      onBlur={(event) => {
        if (event.currentTarget.parentElement?.contains(event.relatedTarget as Node | null)) return;
        blurredRef.current = true;
        if (!composingRef.current) commit();
      }}
      onKeyDown={(event: KeyboardEvent<HTMLDivElement>) => {
        if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return;
        if (event.key === "Enter") { event.preventDefault(); event.currentTarget.blur(); }
        if (event.key === "Escape") {
          event.preventDefault();
          cancelledRef.current = true;
          onCancel();
        }
      }}
    />
  </div>;
}

function normalizeNodeText(content: RichText): RichText {
  return content.text.trim() ? content : { text: "未命名主题", runs: [] };
}

function rangeHasStyle(content: RichText, range: TextRange, field: "bold" | "italic" | "underline"): boolean {
  if (range.start === range.end || !content.text) return false;
  const length = [...content.text].length;
  const runs = content.runs.length ? content.runs : [{ start: 0, end: length, style: emptyStyle() }];
  const selected = runs.filter((run) => run.start < range.end && run.end > range.start);
  return selected.length > 0 && selected.every((run) => run.style[field]);
}

function emptyStyle() {
  return { bold: false, italic: false, underline: false, strikethrough: false, fontFamily: null, fontSize: null, color: null, highlight: null, verticalAlign: null } as const;
}

function isDefaultStyle(style: RichText["runs"][number]["style"]): boolean {
  return !style.bold && !style.italic && !style.underline && !style.strikethrough
    && style.fontFamily === null && style.fontSize === null && style.color === null
    && style.highlight === null && style.verticalAlign === null;
}
