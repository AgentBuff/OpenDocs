# `@open-office/toolbar-react`

React renderer for `@open-office/toolbar-core` descriptors. 本包只负责把 resolved descriptor
映射为 `@open-office/ui` 的 Toolbar primitives；它不读取 Artifact state，也不直接调用命令、
HTTP 或 WASM。

## Usage

```tsx
<ToolbarRenderer
  items={documentToolbar}
  context={toolbarContext}
  onAction={(action, item) => dispatchToolbarAction(action, item)}
  iconMap={iconMap}
  groupLabels={{ history: "历史操作", insert: "插入" }}
/>
```

`items` 和 `context` 应由 adapter 保持稳定引用；只有 selection/capability 改变时才更新 context。
`onAction` 接收 opaque action ID 和 resolved descriptor，adapter 再将其转换为 semantic command。

## DOM and accessibility

- 根节点使用 `role="toolbar"`，每个 descriptor group 使用 `role="group"`。
- `groupLabels` 将稳定 group ID 映射为可读的 ARIA 标签；未提供时使用 group ID，避免产生无标签分组。
- 每个操作元素带 `data-toolbar-id`/`data-toolbar-kind`，用于跨 Artifact 自动化回归，不作为业务状态。
- button/toggle 暴露 `aria-label`、`aria-keyshortcuts`、`aria-pressed`；menu/select 暴露对应的
  `aria-haspopup`，快捷键同时显示在 title 中。
- separator 使用 `role="separator"`，不创建可聚焦元素。
- 所有焦点、禁用、hover 和密度视觉由 `@open-office/ui` Token/组件承担；本包不写 product CSS。

## Renderer boundary

`ToolbarRenderer` 是通用 renderer，不承载 overflow 测量、Document block 菜单或 Artifact command。
后续 Spreadsheet/PPT/Mindmap/Whiteboard adapter 只替换 descriptors、context、icon map 和 action
handler；不得复制 Document toolbar JSX，也不得在 renderer 中添加 Artifact kind 分支。
