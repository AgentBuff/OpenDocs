# open-office 开发约定

本项目使用 `oo-schema::DocumentModel` 和 `oo-document::DocumentEngine` 作为 Document 的
唯一模型与写入入口。服务端只保存 Artifact snapshot，前端使用 DOM-first Block Tree 编辑器。

## 常用命令

```bash
cargo fmt --all
cargo test --workspace
cd web && pnpm typecheck && pnpm test && pnpm build
```

服务端监听 8787，前端监听 5174，`/api` 由 Vite 代理。

## 设计原则

- Document 是 Block Tree；Spreadsheet 是 Grid；Presentation/Mindmap/Whiteboard 使用各自
  的 Scene Graph/Graph；不要建立跨产品的万能 block；
- schema、engine、protocol、renderer 分层，renderer 不持有业务状态；
- Document command 必须经 `DocumentEngine::execute(DocumentCommandBatch)`，网络层使用
  `ArtifactCommandEnvelope`，提交结果使用 typed `CommitResult`；
- snapshot 采用不可变对象键，revision 与事务幂等日志原子更新；
- Canvas 只作为图形或预览 renderer，文本输入和可访问性优先使用 DOM；
- 不恢复旧 `/content`、`/operations`、Paragraph/WASM/Canvas 兼容路径。

详细架构见 `docs/architecture.md`，切换验收见 `docs/cutover.md`。
