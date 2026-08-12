import type { ToolbarDescriptor } from "@open-office/toolbar-core";
import type { IconName } from "@open-office/ui";
import type { ActionId } from "../actions/registry.js";

/**
 * Document adapter actions are deliberately opaque to toolbar-core. Only
 * this adapter knows that `insert-menu` and `kind-select` map to editor UI;
 * the shared renderer never imports DocumentCommand or BlockStore.
 */
export type DocumentToolbarAction = ActionId | "insert-menu" | "kind-select";
export type ToolbarItem = ToolbarDescriptor<DocumentToolbarAction> & { icon?: IconName };

export interface ToolbarSection {
  id: "history" | "insert" | "text" | "paragraph" | "more";
  label: string;
  items: ToolbarItem[];
}

/**
 * Toolbar order and grouping are data. Runtime enablement stays in the
 * editor action registry; this file contains no dispatch or model mutation.
 */
export const blockToolbarSections: ToolbarSection[] = [
  {
    id: "history",
    label: "历史操作",
    items: [
      { id: "undo", group: "history", kind: "button", action: "undo", label: "撤销", icon: "undo" },
      { id: "redo", group: "history", kind: "button", action: "redo", label: "重做", icon: "redo" },
      { id: "format-painter", group: "history", kind: "button", action: "formatPainter", label: "格式刷", icon: "brush" },
      { id: "clear-format", group: "history", kind: "button", action: "clearFormat", label: "清除格式", icon: "eraser" },
    ],
  },
  {
    id: "insert",
    label: "插入",
    items: [
      { id: "insert-menu", group: "insert", kind: "menu", action: "insert-menu", label: "插入", icon: "insert" },
    ],
  },
  {
    id: "text",
    label: "文字格式",
    items: [
      { id: "kind-select", group: "text", kind: "select", action: "kind-select", label: "块样式" },
      { id: "increase-font-size", group: "text", kind: "button", action: "increaseFontSize", label: "增大字号", icon: "font-increase" },
      { id: "decrease-font-size", group: "text", kind: "button", action: "decreaseFontSize", label: "减小字号", icon: "font-decrease" },
      { id: "bold", group: "text", kind: "toggle", action: "bold", label: "加粗", icon: "bold" },
      { id: "italic", group: "text", kind: "toggle", action: "italic", label: "斜体", icon: "italic" },
      { id: "underline", group: "text", kind: "toggle", action: "underline", label: "下划线", icon: "underline" },
      { id: "strike", group: "text", kind: "toggle", action: "strike", label: "删除线", icon: "strikethrough" },
    ],
  },
  {
    id: "paragraph",
    label: "段落格式",
    items: [
      { id: "align-left", group: "paragraph", kind: "button", action: "alignLeft", label: "左对齐", icon: "align-left" },
      { id: "align-center", group: "paragraph", kind: "button", action: "alignCenter", label: "居中对齐", icon: "align-center" },
      { id: "align-right", group: "paragraph", kind: "button", action: "alignRight", label: "右对齐", icon: "align-right" },
      { id: "align-justify", group: "paragraph", kind: "button", action: "alignJustify", label: "两端对齐", icon: "align-justify" },
      { id: "bullet-list", group: "paragraph", kind: "toggle", action: "bulletList", label: "项目符号", icon: "bullet-list" },
      { id: "ordered-list", group: "paragraph", kind: "toggle", action: "orderedList", label: "编号列表", icon: "ordered-list" },
      { id: "todo", group: "paragraph", kind: "toggle", action: "todo", label: "待办事项", icon: "todo" },
      { id: "indent-decrease", group: "paragraph", kind: "button", action: "indentDecrease", label: "减少缩进", icon: "arrow-left" },
      { id: "indent-increase", group: "paragraph", kind: "button", action: "indentIncrease", label: "增加缩进", icon: "arrow-right" },
    ],
  },
  {
    id: "more",
    label: "插入与段落设置",
    items: [],
  },
];

/** Flat view is useful for capability filtering and contract tests. */
export const blockToolbarItems: ToolbarItem[] = blockToolbarSections.flatMap((section) => section.items);
