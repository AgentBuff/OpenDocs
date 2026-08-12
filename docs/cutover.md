# DocumentModel cutover（已完成）

项目已经完成一次干净的破坏式切换。开发数据可以直接重建；生产环境在发布前执行离线
导入，不在在线请求路径保留旧模型回退。

## 当前唯一运行路径

- `oo-docx::parse_docx` 直接产出 `oo_schema::DocumentModel`。
- 服务端只持久化 `ArtifactEnvelope`，对象键使用 `artifacts/{revision}.json`。
- 读取使用 `/api/artifacts/{id}/snapshot`；完整快照写入同一路径也必须携带 `If-Match` 与 `x-transaction-id`，返回 typed `CommitResult`，不可绕过事务幂等和 domain event outbox。
- 编辑使用 `/api/artifacts/{id}/transactions`，事务摘要与 snapshot 指针在同一数据库事务内提交。
- 版本历史使用 `/api/artifacts/{id}/revisions`（列表/读取）和
  `/api/artifacts/{id}/revisions/{version}/restore`；恢复只生成新 revision，并要求 `If-Match`、`x-transaction-id`，以 system/import transaction 写入 durable outbox。
- 文件导入统一使用 `/api/artifacts/import`，格式由明确的文件扩展名选择 adapter；原始文件使用 `/api/artifacts/{id}/source`，当前模型导出使用 `/api/artifacts/{id}/export/{format}`。
- `document_snapshots` 登记所有 canonical 版本；Blob GC 只有在完成离线历史登记与备份后，设置
  `OO_ENABLE_BLOB_GC=1` 才执行，默认不猜测未登记对象是否可删除。
- 已有数据先执行 `cargo run -p oo-server --bin reconcile-snapshots -- <data-dir>`；该工具只登记
  通过 Artifact/schema/revision 校验的对象，不删除文件，检查输出和备份后再打开 GC。
- React 使用 Block Tree DOM 编辑器；每个 block 有稳定 id，行首加号/把手固定在正文左侧。
- Canvas/WASM Paragraph 会话不再进入文档编辑器运行时；Canvas 只留给未来白板/演示文稿引擎。
- 旧 Paragraph/layout/editor/WASM 源码与生成物已移入 `freeze/legacy-before-block-cutover`，不在
  活动 workspace 中；该目录只用于历史追溯，禁止被新功能导入。

## 已删除的运行时兼容面

- `/api/docs/**` 整棵旧资源树（当前公共路由已删除）
- `document_operations` 旧 mutation 日志
- 旧 Paragraph Document、旧 WASM session、旧 Canvas 文本编辑器
- DOCX importer 的旧投影入口

开发数据库按 canonical schema 直接创建；切换前的本地数据已归档。生产数据在发布前执行
离线导入，不在在线服务中保留旧快照或旧列名迁移。

## 验收

```text
cargo test --workspace
cd web && pnpm typecheck && pnpm test && pnpm build
```

运行时代码只允许依赖各自 Artifact engine/adapter 的 canonical 路径：Document 使用
`oo-schema`、`oo-document`、`oo-protocol`、`oo-docx`，Spreadsheet/Presentation 分别使用
`oo-xlsx`/`oo-pptx`；不得通过 Document 适配器降级。历史 ADR 可以描述切换原因，但不得再成为产品代码的导入来源。
