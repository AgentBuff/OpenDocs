# `@open-office/toolbar-core`

`toolbar-core` 是跨 Artifact 的无框架工具栏协议。它只处理 descriptor 的结构、能力可见性和
分组，不导入 React、DOM、UI 组件、Document schema 或任何领域命令。

## Descriptor contract

```ts
type ToolbarDescriptor<ActionId, Context> = {
  id: string;                  // 在整棵 descriptor tree 中唯一
  group: string;               // 稳定分组 ID，不是显示文本
  kind: "button" | "toggle" | "select" | "menu" | "separator";
  label?: string;
  ariaLabel?: string;          // icon-only 项必须提供
  icon?: string;               // Icon Registry 的稳定名称，不携带 React 节点
  shortcut?: string;           // 仅用于呈现和 aria-keyshortcuts
  action?: ActionId;           // Artifact adapter 自己定义的 opaque action ID
  visible?: boolean | ((context: Context) => boolean);
  enabled?: boolean | ((context: Context) => boolean);
  active?: boolean | ((context: Context) => boolean);
  priority?: number;
  overflowPriority?: number;
  children?: readonly ToolbarDescriptor<ActionId, Context>[]; // 仅 menu
};
```

约束由 `validateToolbar` 在开发/测试阶段检查：ID 全树唯一、group 非空、交互项有 action 或
子项、separator 不持有 action/children、children 只出现在 menu、优先级为非负有限数值。
`resolveToolbar` 会在每一层过滤 `visible`，因此隐藏菜单子项不会泄露给 renderer；它返回新的
resolved tree，不修改 adapter 的 descriptor 输入。

## Artifact adapter boundary

每个 Artifact 只提供自己的 `ActionId`、descriptor 和 `ToolbarResolutionContext`：

```text
Spreadsheet adapter ─┐
Presentation adapter ─┼─> toolbar-core ─> toolbar-react / another renderer
Document adapter ────┘
```

action ID 不得是 `DocumentCommand`、HTTP 请求或 WASM 对象；renderer 只把 opaque ID 回调给
adapter。这样同一个 renderer 可以服务 Word、Spreadsheet、PPT、Mindmap 和 Whiteboard，且不会
污染各自的领域模型。

## Performance contract

- descriptor 应在 Artifact adapter 模块级定义并保持引用稳定；运行时只把 context 作为状态输入。
- `resolveToolbar` 和 `groupToolbar` 是纯函数，不产生副作用，也不修改输入 tree。
- 可见性与 enabled/active 计算发生在渲染边界之前；大文档编辑路径不得把 Document snapshot
  放进 toolbar context。
- renderer 负责 memoization 和溢出策略；core 不创建 DOM、测量布局或执行副作用。

## Accessibility contract

所有非 separator 项必须有 `label` 或 `ariaLabel`；icon-only 项使用 `ariaLabel`。快捷键通过
`shortcut` 暴露给 renderer，由 renderer 输出 `aria-keyshortcuts` 与可读的 title。toggle 使用
`active` 映射为 `aria-pressed`；menu/select 使用正确的 `aria-haspopup`。
