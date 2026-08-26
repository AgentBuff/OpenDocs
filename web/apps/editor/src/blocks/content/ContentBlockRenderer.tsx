import { useRef, useState, type CSSProperties } from "react";

import { Icon } from "@open-office/ui";

import type { BlockRendererProps } from "../registry.js";

/** DOM-first text/todo surface; all persistence stays in the supplied session. */
export function ContentBlockRenderer({
  block,
  session,
  contentRef,
  empty,
  align,
  lineHeight,
  placeholder,
  onInput,
  onKeyDown,
  onFocus,
}: BlockRendererProps) {
  const isTodo = block.kind.type === "todo";
  const checked = block.data.type === "todo" && block.data.data.checked;
  const composingRef = useRef(false);
  const [pastingImage, setPastingImage] = useState(false);
  // A 32px hit target should not pin a compact text line to the top.
  const isBaselineCenteredTextRow = block.kind.type === "paragraph" || isTodo;
  const effectiveLineHeight = lineHeight ?? 1.75;
  const baselinePadding = isBaselineCenteredTextRow
    ? Math.max(2, (32 - 16 * effectiveLineHeight) / 2)
    : undefined;
  const contentStyle: CSSProperties = {
    textAlign: align,
    lineHeight: effectiveLineHeight,
    paddingBlock: baselinePadding === undefined ? undefined : `${baselinePadding}px`,
  };

  return (
    <div className={isTodo ? `block-row__todo${checked ? " is-checked" : ""}` : undefined}>
      {isTodo && (
        <button
          className="block-row__todo-check"
          type="button"
          role="checkbox"
          aria-checked={checked}
          aria-label={checked ? "标记为未完成" : "标记为已完成"}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => session.setTodoChecked(block.id, !checked)}
        >
          {checked && <Icon name="check" />}
        </button>
      )}
      <div
        ref={contentRef}
        className="block-row__content"
        contentEditable
        suppressContentEditableWarning
        role="textbox"
        tabIndex={0}
        style={contentStyle}
        data-placeholder={empty ? placeholder : undefined}
        aria-busy={pastingImage || undefined}
        aria-label={placeholder}
        onFocus={onFocus ?? (() => session.setActiveBlock(block.id))}
        onCompositionStart={() => { composingRef.current = true; }}
        onCompositionEnd={() => {
          composingRef.current = false;
          onInput();
        }}
        onInput={() => {
          if (!composingRef.current) onInput();
        }}
        onPaste={(event) => {
          const itemFile = Array.from(event.clipboardData.items)
            .find((item) => item.kind === "file" && item.type.startsWith("image/"))
            ?.getAsFile();
          const file = itemFile ?? Array.from(event.clipboardData.files).find((candidate) => candidate.type.startsWith("image/"));
          if (!file) return;
          event.preventDefault();
          setPastingImage(true);
          void session.insertPastedImage(block.id, file)
            .then((imageBlockId) => {
              if (imageBlockId) session.setActiveBlock(imageBlockId);
            })
            .finally(() => setPastingImage(false));
        }}
        onKeyDown={onKeyDown}
        onBlur={() => void session.save()}
      />
      {pastingImage && <span className="block-row__paste-status" role="status">正在插入图片…</span>}
    </div>
  );
}
