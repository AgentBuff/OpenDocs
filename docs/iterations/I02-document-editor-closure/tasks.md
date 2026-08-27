# I02 tasks

## I02-01 — 交叉核对与缺口清单 — complete

- 对照 §C2 七组代码重点逐项审计现状(见 acceptance 的证据列)。

## I02-02 — undo/redo 与离线恢复的浏览器证据 — complete (2026-08-26)

- 新增 `e2e/document/history.spec.ts`:
  - Mod+Z 撤销经服务端 journal、revision 只前进;Mod+Shift+Z 重做恢复;
  - offline→输入→online 的 autosave outbox 投递与 reload 持久;
- **顺带修复真实缺陷**:undo/redo 后 projection 重建 DOM 使焦点落入 body,
  连续历史快捷键被静默吞掉;submitHistory 完成后恢复首个可编辑块焦点。

## I02-03 — 跨块文本选区语义操作统一(实现+证据) — complete (2026-08-26)

- 实现早已存在:`useBlockSession.deleteTextSelection`(跨块合并删除、同批原子)、
  `toggleMark/setInlineAttrs`(跨块逐块 patchInlineRange 批量);本次补齐浏览器证据:
- 新增 `e2e/document/selection.spec.ts`:三块区间 Backspace 合并为单块("尾部余文并入
  首块、中间/尾块直接删除");跨首尾两块的 Range 加粗在三个持久化块上生效,且选区
  存活支持继续输入。
- 发现并记录:pointer Shift+Click 的原生扩展选区会被编辑器 pointer 管理路径截断,
  仅影响测试注入方式(evaluate 构造 Range),不阻塞语义链路——待后续交互切片复核。

## I02-04 — 打印 / 分页基线 — complete (2026-08-26)

- `styles/print.css`:@media print 隐藏全部 chrome(gutter/菜单/工具栏/状态),
  白底黑字、A4 边距、block 防断页;true pagination preview 仍归未来 Canvas
  renderer(ADR-0002),本切片只保证打印输出可用且无交互残留。

## I02-05 — 未关闭项(留档)

- DOCX fidelity matrix 与视觉 round-trip(matrix 行保持 partial);
- 嵌套表格浮层回归;附件 block;可访问性深化(焦点环已覆盖部分);
- 分页预览 Canvas renderer;
- Pointer Shift 扩展选区与 overlay 管理的冲突复核(I02-03 发现)。
