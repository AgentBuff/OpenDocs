# UI 贡献指南

## 提交前检查

每个组件或主题变更必须说明：

- 公共 API 和所属分层；
- default、hover、active、focus-visible、disabled、selected/error 等状态；
- 键盘路径和 ARIA 语义；
- light/dark、density 和 reduced-motion 行为；
- 是否有 Artifact/Document 依赖（基础 UI 必须为否）；
- 迁移影响和可回滚/删除边界。

## 测试门禁

在仓库 `web` 目录运行：

```bash
pnpm typecheck
CI=true pnpm test
pnpm build
pnpm architecture:check
```

组件测试优先使用 React SSR/DOM 语义断言和键盘事件，不把截图当作唯一证据。无障碍测试必须
验证可访问名称、role、状态属性、焦点顺序和键盘行为；禁止为了“通过 axe”伪造 `aria-*` 或
隐藏真实交互问题。本仓库当前不强制引入 axe 运行时，结构契约测试是轻量的第一道门禁，真实
浏览器回归负责浮层、焦点和布局证据。

性能测试应使用稳定输入并记录预算：Toolbar descriptor 解析是纯函数，不能 clone 整个模型；
Document 输入性能使用 `web/scripts/bench-editor.mjs` 的 Chromium 探针，报告 P95、long task、
layout 和 DOM mutation。性能预算失败时先修基础设施，不通过放宽阈值掩盖回归。

## 代码边界

- 公共消费者只从 `@open-office/ui` 根入口导入。
- Overlay 统一复用 `overlay/`；业务组件不得自行注册全局 outside-click/Escape/scroll 监听器。
- 图标来自 Icon Registry；Toolbar descriptor 不携带 React 节点或 SVG 字符串。
- UI action 只返回能力 ID/semantic command，不能直接写 Artifact snapshot。
- 不新增旧 class、旧变量 alias、双读双写或在线兼容分支。

## Pull Request 审阅清单

```text
[ ] API/Token/迁移文档已更新
[ ] 状态、键盘、ARIA、主题和密度测试已补齐
[ ] 无新 Artifact/Document/网络依赖
[ ] 无重复浮层生命周期或全局监听器
[ ] 性能预算与架构门禁通过
[ ] 删除旧实现后已检查无消费者
```
