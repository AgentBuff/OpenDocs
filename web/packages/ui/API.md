# `@open-office/ui` 公共 API

`@open-office/ui` 是 Open Office 的无领域 UI 基础设施。它的唯一稳定入口是包根路径：

```tsx
import { Button, ThemeProvider, ToolbarButton } from "@open-office/ui";
```

不要从 `@open-office/ui/src/...` 或 CSS 子路径导入。内部目录会继续重组，根入口才是可维护的
公共边界。

## API 分层

| 分层 | 公共内容 | 允许依赖 |
| --- | --- | --- |
| foundation | `ThemeProvider`、`ThemeRuntime`、`useTheme`、`useThemeRuntime` | React、Token |
| primitives | `Button`、`IconButton`、`Surface` | React、Token |
| controls | `Input`、`Textarea`、`Select`、`Checkbox`、`Switch` | React、Token |
| navigation | `Toolbar*`、`Menu*` | React、Overlay、Token |
| overlay | `Popover`、`Dropdown`、`Tooltip`、`FocusScope`、dismiss/position 原语 | React、DOM |
| feedback | `Spinner`、`Badge`、`Empty`、`Divider` | React、Token |
| icons | `Icon`、`IconProvider`、`IconRegistry` | React、注册图标 |

基础组件不能导入 Artifact、Document schema、Document engine、command、网络 API 或编辑器
session。Document、Spreadsheet、Presentation、Mindmap、Whiteboard 只能在产品层组合这些
API，并通过自己的 adapter 提供领域状态和 action。

## 受控契约

- 输入值优先采用 `value/defaultValue/onChange`；自有 ToolbarSelect 使用
  `value/defaultValue/options/onValueChange`；浮层优先采用
  `open/defaultOpen/onOpenChange`。
- `size`、`status`、`disabled`、`readOnly`、`active` 等状态由组件 API 表达，不在产品 CSS
  中重新解释同一状态。
- `<button>` 默认 `type="button"`，提交按钮必须由调用者显式传入 `type="submit"`。
- icon-only 操作必须提供 `aria-label`；装饰性图标不承担可访问名称。
- Toolbar 的 toggle/menu/select 使用 `aria-pressed`、`aria-haspopup`、`aria-expanded` 和
  `aria-controls` 等原生语义，业务组件不复制一套属性协议。
- `ToolbarSplitGroup` 用于“主操作 + 独立下拉箭头”结构；箭头应作为真实 `ToolbarButton` 或
  `Popover` trigger 传入，不能使用 Unicode 字符伪造箭头。

## Token 与主题

组件只消费 `--oo-*` 语义和组件 Token。请先阅读 [TOKENS.md](./TOKENS.md)，再通过
`ThemeProvider` 或根节点作用域覆盖主题；不要在 JSX 中把主题色、阴影、动效时长写成散落的
产品常量。主题切换由 `ThemeRuntime` 管理，不能进入 Artifact snapshot。

## 变更稳定性

- 新增导出必须同时补 API 文档、状态/键盘测试和变更说明。
- 破坏性删除遵循 [MIGRATION.md](./MIGRATION.md)，在线路径不保留旧别名或双渲染。
- 包的实现目录不是扩展点；若需要新的 Artifact 能力，应新建 adapter，而不是向通用组件
  增加 `kind` 分支。
