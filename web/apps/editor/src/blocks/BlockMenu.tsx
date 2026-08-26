import { useRef, type ReactNode } from "react";
import type { DocumentBlock, DocumentBlockKind } from "@open-office/schema/artifact";
import { Icon } from "@open-office/ui";
import { TableInsertPicker } from "../toolbar/TableInsertPicker.js";

const alignIcons = {
  left: "align-left",
  center: "align-center",
  right: "align-right",
  justify: "align-justify",
} as const;

export function BlockMenu({
  block,
  onClose,
  onInsert,
  onDelete,
  onKind,
  activeAlign,
  activeList,
  onAlignment,
  onList,
  onLink,
  onInsertTable,
  onInsertImage,
  onInsertQuote,
  onInsertCallout,
  onInsertTodo,
  onInsertCode,
  onDivider,
}: {
  block: DocumentBlock;
  onClose: () => void;
  onInsert: () => void;
  onDelete: () => void;
  onKind: (kind: DocumentBlockKind) => void;
  activeAlign?: "left" | "center" | "right" | "justify";
  activeList: "bullet" | "ordered" | null;
  onAlignment: (align: "left" | "center" | "right" | "justify") => void;
  onList: (type: "bullet" | "ordered") => void;
  onLink: () => void;
  onInsertTable: (rows: number, columns: number) => void;
  onInsertImage: (file: File) => Promise<void> | void;
  onInsertQuote: () => void;
  onInsertCallout: () => void;
  onInsertTodo: () => void;
  onInsertCode: () => void;
  onDivider: () => void;
}) {
  const imageInputRef = useRef<HTMLInputElement>(null);
  const kind = block.kind.type === "heading" ? `heading-${block.kind.level}` : block.kind.type;
  const styleButton = (value: string, label: string, content: ReactNode) => (
    <button
      key={value}
      className={`block-row__menu-icon${kind === value ? " is-active" : ""}`}
      type="button"
      role="menuitemradio"
      aria-checked={kind === value}
      aria-label={label}
      title={label}
      onClick={() => {
        if (value === "paragraph") onKind({ type: "paragraph" });
        else if (value.startsWith("heading-")) onKind({ type: "heading", level: Number(value.slice(8)) });
      }}
    >
      {content}
    </button>
  );

  const alignButton = (value: "left" | "center" | "right" | "justify", label: string) => (
    <button
      className={`block-row__menu-icon${activeAlign === value ? " is-active" : ""}`}
      type="button"
      role="menuitemradio"
      aria-checked={activeAlign === value}
      aria-label={label}
      title={label}
      onClick={() => onAlignment(value)}
    >
      <Icon name={alignIcons[value]} />
    </button>
  );

  return (
    <div className="block-row__menu" role="menu" aria-label="块菜单" onKeyDown={(event) => { if (event.key === "Escape") onClose(); }}>
      <section className="block-row__menu-section" aria-label="常用格式">
        <div className="block-row__menu-title">样式</div>
        <div className="block-row__menu-grid block-row__menu-grid--styles">
          {styleButton("paragraph", "正文", <span className="block-row__menu-type">T</span>)}
          {[1, 2, 3, 4, 5, 6].map((level) => styleButton(`heading-${level}`, `标题 ${level}`, <span className="block-row__menu-type">H{level}</span>))}
        </div>
        <div className="block-row__menu-grid block-row__menu-grid--formats">
          {alignButton("left", "左对齐")}
          {alignButton("center", "居中对齐")}
          {alignButton("right", "右对齐")}
          {alignButton("justify", "两端对齐")}
          <button className={`block-row__menu-icon${activeList === "bullet" ? " is-active" : ""}`} type="button" role="menuitemradio" aria-checked={activeList === "bullet"} aria-label="项目符号" title="项目符号" onClick={() => onList("bullet")}><Icon name="bullet-list" /></button>
          <button className={`block-row__menu-icon${activeList === "ordered" ? " is-active" : ""}`} type="button" role="menuitemradio" aria-checked={activeList === "ordered"} aria-label="编号列表" title="编号列表" onClick={() => onList("ordered")}><Icon name="ordered-list" /></button>
        </div>
      </section>
      <div className="block-row__menu-sep" />
      <section className="block-row__menu-section" aria-label="插入">
        <div className="block-row__menu-title">插入</div>
        <div className="block-row__menu-list">
          <button className="block-row__menu-item" type="button" role="menuitem" onClick={() => imageInputRef.current?.click()}><Icon name="image" /><span>图片</span></button>
          <button className="block-row__menu-item" type="button" role="menuitem" onClick={onLink}><Icon name="link" /><span>链接块</span></button>
          <TableInsertPicker variant="menu" onSelect={onInsertTable} />
          <button className="block-row__menu-item" type="button" role="menuitem" onClick={onInsertQuote}><Icon name="quote" /><span>引用块</span></button>
          <button className="block-row__menu-item" type="button" role="menuitem" onClick={onInsertCallout}><Icon name="highlight" /><span>高亮内容块</span></button>
          <button className="block-row__menu-item" type="button" role="menuitem" onClick={onInsertTodo}><Icon name="todo" /><span>待办事项</span></button>
          <button className="block-row__menu-item" type="button" role="menuitem" onClick={onInsertCode}><Icon name="code" /><span>代码块</span></button>
        </div>
      </section>
      <input
        ref={imageInputRef}
        className="sr-only"
        type="file"
        accept="image/*"
        tabIndex={-1}
        onChange={(event) => {
          const file = event.currentTarget.files?.[0];
          event.currentTarget.value = "";
          if (!file) return;
          onClose();
          void onInsertImage(file);
        }}
      />
      <div className="block-row__menu-sep" />
      <section className="block-row__menu-section" aria-label="结构操作">
        <div className="block-row__menu-title">结构</div>
        <div className="block-row__menu-list">
          <button className="block-row__menu-item" type="button" role="menuitem" onClick={onDivider}><Icon name="divider" /><span>分隔线</span></button>
          <button className="block-row__menu-item" type="button" role="menuitem" onClick={onInsert}><Icon name="insert" /><span>在下方插入块</span></button>
          <button className="block-row__menu-item block-row__menu-item--danger" type="button" role="menuitem" onClick={onDelete}><Icon name="delete" /><span>删除块</span></button>
        </div>
      </section>
      <span className="sr-only">当前类型：{block.kind.type}</span>
    </div>
  );
}
