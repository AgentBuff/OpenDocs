import { useCallback, useEffect, useRef, useState } from "react";
import type { PresentationV5Node, PresentationV5RichText, PresentationV5TextStyle } from "@open-office/schema";

import { patchPresentationParagraphRange, patchPresentationTextRange, preservePresentationText, presentationRangeStyle, presentationTextStyle } from "./PresentationRichText.js";

export function PresentationTextEditor({ node, body: initialBody, pointScale, onSave }: {
  pointScale: number;
  node: PresentationV5Node;
  body: PresentationV5RichText;
  onSave: (node: PresentationV5Node, body: PresentationV5RichText) => void;
}) {
  const [body, setBody] = useState(initialBody);
  const [, setSelectionVersion] = useState(0);
  const bodyRef = useRef(initialBody);
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const selection = useRef({ start: 0, end: [...initialBody.text].length });
  const composing = useRef(false);
  const blurred = useRef(false);
  const cancelled = useRef(false);

  useEffect(() => {
    setBody(initialBody);
    bodyRef.current = initialBody;
    selection.current = { start: 0, end: [...initialBody.text].length };
  }, [initialBody, node.id]);

  const commit = useCallback(() => {
    if (!cancelled.current) onSave(node, bodyRef.current);
  }, [node, onSave]);

  const syncSelection = () => {
    const editor = editorRef.current;
    if (!editor) return selection.current;
    selection.current = {
      start: [...editor.value.slice(0, editor.selectionStart)].length,
      end: [...editor.value.slice(0, editor.selectionEnd)].length,
    };
    return selection.current;
  };
  const applyStyle = (patch: Partial<PresentationV5TextStyle>) => {
    syncSelection();
    const next = patchPresentationTextRange(bodyRef.current, selection.current.start, selection.current.end, patch);
    bodyRef.current = next;
    setBody(next);
  };

  const applyParagraph = (patch: Parameters<typeof patchPresentationParagraphRange>[3]) => {
    syncSelection();
    const next = patchPresentationParagraphRange(bodyRef.current, selection.current.start, selection.current.end, patch);
    bodyRef.current = next;
    setBody(next);
  };
  const selectedStyle = presentationRangeStyle(body, selection.current.start, selection.current.end);
  const selectedParagraph = body.paragraphs.find((paragraph) => paragraph.start <= selection.current.start && selection.current.start <= paragraph.end);

  return <div className="presentation-studio__text-edit-surface" onPointerDown={(event) => event.stopPropagation()}>
    <div className="presentation-studio__text-range-toolbar" role="toolbar" aria-label="所选文字格式">
      {(["bold", "italic", "underline"] as const).map((property) => <button
        key={property}
        type="button"
        aria-label={property === "bold" ? "加粗所选文字" : property === "italic" ? "倾斜所选文字" : "为所选文字添加下划线"}
        onPointerDown={(event) => event.preventDefault()}
        aria-pressed={selectedStyle[property]}
        onClick={() => {
          syncSelection();
          const current = presentationRangeStyle(bodyRef.current, selection.current.start, selection.current.end);
          applyStyle({ [property]: !current[property] });
        }}
      >{property === "bold" ? "B" : property === "italic" ? "I" : "U"}</button>)}
      <span className="presentation-studio__text-range-divider" aria-hidden="true" />
      {(["left", "center", "right", "justify"] as const).map((alignment) => <button
        key={alignment}
        type="button"
        aria-label={`${alignment === "left" ? "左" : alignment === "center" ? "居中" : alignment === "right" ? "右" : "两端"}对齐所选段落`}
        aria-pressed={selectedParagraph?.alignment === alignment}
        onPointerDown={(event) => event.preventDefault()}
        onClick={() => applyParagraph({ alignment })}
      >{alignment === "left" ? "L" : alignment === "center" ? "C" : alignment === "right" ? "R" : "J"}</button>)}
      <button type="button" aria-label="切换所选段落项目符号" aria-pressed={selectedParagraph?.list?.type === "bullet"} onPointerDown={(event) => event.preventDefault()} onClick={() => applyParagraph({ list: selectedParagraph?.list?.type === "bullet" ? null : { type: "bullet" } })}>•</button>
      <button type="button" aria-label="切换所选段落编号" aria-pressed={selectedParagraph?.list?.type === "ordered"} onPointerDown={(event) => event.preventDefault()} onClick={() => applyParagraph({ list: selectedParagraph?.list?.type === "ordered" ? null : { type: "ordered", startAt: 1 } })}>1.</button>
    </div>
    <textarea
    ref={editorRef}
    className="presentation-studio__text-editor"
    style={{
      fontSize: 12 * pointScale,
      ...presentationTextStyle(
        node.kind.type === "text" ? node.kind.data.frame.body.runs[0]?.style : undefined,
        pointScale,
      ),
    }}
    autoFocus
    aria-label="编辑文本对象"
    value={body.text}
    onSelect={(event) => {
      selection.current = {
        start: [...event.currentTarget.value.slice(0, event.currentTarget.selectionStart)].length,
        end: [...event.currentTarget.value.slice(0, event.currentTarget.selectionEnd)].length,
      };
      setSelectionVersion((current) => current + 1);
    }}
    onChange={(event) => {
      const next = preservePresentationText(bodyRef.current, event.currentTarget.value);
      bodyRef.current = next;
      setBody(next);
    }}
    onCompositionStart={() => {
      composing.current = true;
    }}
    onCompositionEnd={() => {
      composing.current = false;
      if (blurred.current) commit();
    }}
    onBlur={() => {
      blurred.current = true;
      if (!composing.current) commit();
    }}
    onKeyDown={(event) => {
      if (event.key === "Escape") {
        cancelled.current = true;
        event.currentTarget.blur();
      }
      if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
        event.currentTarget.blur();
      }
    }}
    />
  </div>;
}
