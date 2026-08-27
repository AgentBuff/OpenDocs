# I02 acceptance

## Evidence

- [x] undo/redo 是 server-authoritative 事务:revision 单调前进,UI 由 canonical snapshot 重载。
      Evidence: `e2e/document/history.spec.ts` (undo removes typed text…), 断言 revisionAfterUndo > before 且 redo 后持久文本恢复。
- [x] 离线期间的键入经 autosave outbox 在网络恢复后投递,reload 后仍在。
      Evidence: `history.spec.ts` "committed text survives an offline autosave window and reload"。
- [x] undo/redo 后焦点不丢失(产品修复)。
      Evidence: `useBlockSession.submitHistory` 尾部 focus 恢复 + 上两条用例连续快捷键可行。
- [x] 跨块删除为单一原子批次:合并余文落首块,中间/尾块删除,不留空壳。
      Evidence: `selection.spec.ts` "deleting a range spanning three blocks…" 断言持久化 blocks === ["ock"] 且总数 1。
- [x] 跨块加粗为批量 patchInlineRange,全部目标块 runs 生效且选区存活。
      Evidence: `selection.spec.ts` "bold applies to every selected block…" poll=3 块 bold,后续输入仍在原位。
- [x] 打印输出无 chrome 残留。
      Evidence: `styles/print.css` + visual 套件(light/dark)不受影响(`playwright test e2e/visual` 2 passed)。

## Gates (2026-08-26)

cargo fmt/test 281 green · pnpm typecheck/test/architecture:check green · playwright 18/18。

## Exit decision

§C2 的"一致性选择/上下文操作、图片对象、表格操作、损失报告、autosave/undo 浏览器证据、打印基线"
在本迭代有代码+自动化证据;fidelity matrix、嵌套表格浮层、分页预览 renderer、a11y 深化明确留档,
属后续迭代而非本门槛(与其宣称 product parity 不符的诚实口径一致)。
