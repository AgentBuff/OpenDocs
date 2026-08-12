# UI 迁移指南

UI 重构采用破坏式切换。在线编辑路径不保留旧组件别名、旧 CSS class、主题变量 fallback、
双渲染或双事件监听；迁移完成后旧实现应直接删除。数据库 schema 或离线导入的版本迁移不属于
本指南。

## 旧工具栏控件

| 旧做法 | 当前做法 |
| --- | --- |
| `.tbtn` / `.tselect` / `.tcolor` | `ToolbarButton` / `ToolbarSelect` / `ToolbarField` |
| 直接堆叠原生按钮并写产品 CSS | 使用 `@open-office/ui` navigation 原语 |
| 文本“左/中/右/均”作为图标 | Icon Registry 的 `align-*` 图标 |
| 自定义 outside-click/Escape | `Popover`、`Dropdown`、`useDismissableLayer` |
| Toolbar JSX 中直接判断 Artifact kind | `toolbar-core` descriptor + Artifact adapter |

## 迁移步骤

1. 先把消费者改为包根入口导入，并补上显式的 `aria-label`、`type` 和 keyboard 行为。
2. 将硬编码颜色/尺寸映射到 `--oo-*` Token，验证 light/dark 和三种 density。
3. 把弹层改为共享 Overlay 生命周期，再删除页面级监听器和旧定位 CSS。
4. 把 action 提取为 opaque capability ID，由 Artifact adapter 转换为 semantic command。
5. 删除旧 class、alias 和 fallback；用架构 gate 与 `rg` 确认没有在线消费者。
6. 运行 UI 测试、Chromium 回归和性能探针，再提交变更说明。

## 不允许的迁移捷径

- 不在新组件内同时支持旧 prop 名称；
- 不用 `any`、`structuredClone` 或第二份模型来“兼容”旧消费者；
- 不通过隐藏元素保留旧控件；
- 不把旧菜单重新挂回 editor 层以绕过 Overlay contract。
