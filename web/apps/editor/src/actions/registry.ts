import type { DocumentBlockKind } from "@open-office/schema/artifact";

/** 工具栏、快捷键和块菜单共享的能力注册表。能力只声明一次，渲染器不创建业务命令。 */
export interface EditorActionContext {
  hasActiveBlock: boolean;
  /** Inline range commands require a non-empty native text selection. */
  hasTextSelection: boolean;
  canUndo: boolean;
  canRedo: boolean;
  onBold: () => void;
  onItalic: () => void;
  onUnderline: () => void;
  onStrike: () => void;
  onFormatPainter: () => void;
  onClearFormat: () => void;
  onFontSizeAdjust: (delta: -1 | 1) => void;
  onKind: (kind: DocumentBlockKind) => void;
  onAlignment: (align: "left" | "center" | "right" | "justify") => void;
  onList: (type: "bullet" | "ordered") => void;
  onTodo: () => void;
  onIndent: (delta: -1 | 1) => void;
  onLink: () => void;
  onInsertTable: (rows?: number, columns?: number) => void;
  onInsert: () => void;
  onDelete: () => void;
  onSelectAll: () => void;
  onDeleteSelection: () => void;
  onUndo: () => void;
  onRedo: () => void;
}

interface EditorAction {
  id: string;
  title: string;
  shortcut?: string;
  isEnabled: (context: EditorActionContext) => boolean;
  run: (context: EditorActionContext) => void;
}

const activeBlock = (context: EditorActionContext) => context.hasActiveBlock;
const textSelection = (context: EditorActionContext) => context.hasActiveBlock && context.hasTextSelection;

export const actionRegistry = {
  undo: {
    id: "undo",
    title: "撤销",
    shortcut: "Mod+Z",
    isEnabled: (context) => context.canUndo,
    run: (context) => context.onUndo(),
  },
  redo: {
    id: "redo",
    title: "重做",
    shortcut: "Mod+Shift+Z",
    isEnabled: (context) => context.canRedo,
    run: (context) => context.onRedo(),
  },
  bold: {
    id: "bold",
    title: "加粗",
    shortcut: "Mod+B",
    isEnabled: textSelection,
    run: (context) => context.onBold(),
  },
  italic: {
    id: "italic",
    title: "斜体",
    shortcut: "Mod+I",
    isEnabled: textSelection,
    run: (context) => context.onItalic(),
  },
  underline: {
    id: "underline",
    title: "下划线",
    shortcut: "Mod+U",
    isEnabled: textSelection,
    run: (context) => context.onUnderline(),
  },
  strike: {
    id: "strike",
    title: "删除线",
    isEnabled: textSelection,
    run: (context) => context.onStrike(),
  },
  clearFormat: {
    id: "clearFormat",
    title: "清除格式",
    isEnabled: textSelection,
    run: (context) => context.onClearFormat(),
  },
  formatPainter: {
    id: "formatPainter",
    title: "格式刷（再次点击应用）",
    isEnabled: textSelection,
    run: (context) => context.onFormatPainter(),
  },
  increaseFontSize: {
    id: "increaseFontSize",
    title: "增大字号",
    isEnabled: textSelection,
    run: (context) => context.onFontSizeAdjust(1),
  },
  decreaseFontSize: {
    id: "decreaseFontSize",
    title: "减小字号",
    isEnabled: textSelection,
    run: (context) => context.onFontSizeAdjust(-1),
  },
  selectAll: {
    id: "selectAll",
    title: "全选",
    shortcut: "Mod+A",
    isEnabled: activeBlock,
    run: (context) => context.onSelectAll(),
  },
  deleteSelection: {
    id: "deleteSelection",
    title: "删除选中内容",
    isEnabled: activeBlock,
    run: (context) => context.onDeleteSelection(),
  },
  insert: {
    id: "insert",
    title: "在下方插入块",
    isEnabled: activeBlock,
    run: (context) => context.onInsert(),
  },
  divider: {
    id: "divider",
    title: "插入分割线",
    isEnabled: activeBlock,
    run: (context) => context.onKind({ type: "divider" }),
  },
  alignLeft: {
    id: "alignLeft",
    title: "左对齐",
    isEnabled: activeBlock,
    run: (context) => context.onAlignment("left"),
  },
  alignCenter: {
    id: "alignCenter",
    title: "居中对齐",
    isEnabled: activeBlock,
    run: (context) => context.onAlignment("center"),
  },
  alignRight: {
    id: "alignRight",
    title: "右对齐",
    isEnabled: activeBlock,
    run: (context) => context.onAlignment("right"),
  },
  alignJustify: {
    id: "alignJustify",
    title: "两端对齐",
    isEnabled: activeBlock,
    run: (context) => context.onAlignment("justify"),
  },
  bulletList: {
    id: "bulletList",
    title: "项目符号",
    isEnabled: activeBlock,
    run: (context) => context.onList("bullet"),
  },
  orderedList: {
    id: "orderedList",
    title: "编号列表",
    isEnabled: activeBlock,
    run: (context) => context.onList("ordered"),
  },
  todo: {
    id: "todo",
    title: "待办事项",
    isEnabled: activeBlock,
    run: (context) => context.onTodo(),
  },
  indentDecrease: {
    id: "indentDecrease",
    title: "减少缩进",
    isEnabled: activeBlock,
    run: (context) => context.onIndent(-1),
  },
  indentIncrease: {
    id: "indentIncrease",
    title: "增加缩进",
    isEnabled: activeBlock,
    run: (context) => context.onIndent(1),
  },
  link: {
    id: "link",
    title: "插入链接块",
    isEnabled: activeBlock,
    run: (context) => context.onLink(),
  },
  insertTable: {
    id: "insertTable",
    title: "插入表格块",
    isEnabled: activeBlock,
    run: (context) => context.onInsertTable(2, 2),
  },
  delete: {
    id: "delete",
    title: "删除当前块",
    isEnabled: activeBlock,
    run: (context) => context.onDelete(),
  },
} satisfies Record<string, EditorAction>;

export type ActionId = keyof typeof actionRegistry;

export function action(id: ActionId) {
  return actionRegistry[id] as EditorAction;
}

/** 将平台修饰键统一解析成注册表里的 Mod 字符串，业务层不再判断 meta/ctrl。 */
export function resolveShortcut(event: KeyboardEvent): ActionId | null {
  const usesMeta = /Mac|iPhone|iPad/.test(navigator.platform);
  const key = event.key.toUpperCase();
  // macOS users commonly press Control+A out of habit. Treat it as the same document
  // selection intent as Command+A; text inputs are handled by the native control path.
  if (key === "A" && (event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey) {
    return "selectAll";
  }
  const hasMod = usesMeta ? event.metaKey : event.ctrlKey;
  if (!hasMod || event.altKey) return null;
  const shortcut = `Mod${event.shiftKey ? "+Shift" : ""}+${key}`;
  return (Object.keys(actionRegistry) as ActionId[]).find(
    (id) => action(id).shortcut === shortcut,
  ) ?? null;
}
