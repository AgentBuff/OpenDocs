# open-office

open-office 是面向 Document、Spreadsheet、Presentation、Mindmap、Whiteboard 的多 Artifact
平台。当前 Document 已完成从 Paragraph + Canvas 到 Block Tree + DOM-first 的破坏式切换。

## 当前唯一真相

- `crates/oo-schema`：持久化 Artifact/Document schema 与结构校验；
- `crates/oo-document`：Document Block Tree 的事务 engine；
- `crates/oo-protocol`：版本化 Snapshot/Transaction envelope；
- `crates/oo-document-wasm`：`oo-document` 的浏览器薄绑定，不含第二套业务逻辑；
- `crates/oo-docx`：`.docx` 直接解析为 `DocumentModel`；
- `crates/oo-server`：Artifact snapshot 存储和事务提交；
- `web/packages/schema`：浏览器消费类型与网络边界校验；
- `web/packages/document-engine`：typed WASM adapter 和增量 ChangeSet 边界；
- `web/apps/editor`：DOM-first Block Tree 编辑器。

旧 Paragraph、Canvas 文本编辑器、旧 WASM session、旧操作日志不属于 workspace，也没有运行时
消费者。`oo-document-wasm` 是 canonical engine 的薄绑定，不得演变成第二套文档模型。

## 分层约束

```text
Artifact schema → engine/transaction → immutable snapshot
                                      ↘ renderer/view
```

Document 使用 Block Tree；表格使用 Grid；幻灯片、脑图和白板使用各自的 Scene Graph/Graph。
DOM、Canvas、SVG、WebGL 都是 renderer，不得持有领域状态。Canvas 只用于未来的分页预览、
幻灯片、脑图或白板绘制。

`DocumentEngine::execute(DocumentCommandBatch)` 是 Document 写入的唯一入口。编辑器组件只产生
semantic command，服务端通过 `ArtifactCommandEnvelope` 校验 revision、幂等并提交不可变
Artifact snapshot。

## HTTP API

```text
GET    /api/health
GET    /api/capabilities
GET    /api/artifacts
POST   /api/artifacts
POST   /api/artifacts/import   multipart 字段名 file
GET    /api/artifacts/{id}
PATCH  /api/artifacts/{id}
DELETE /api/artifacts/{id}
GET    /api/artifacts/{id}/snapshot
PUT    /api/artifacts/{id}/snapshot  If-Match + x-transaction-id
POST   /api/artifacts/{id}/transactions  If-Match + x-transaction-id
GET    /api/artifacts/{id}/outline
GET    /api/artifacts/{id}/blocks?parentId=&cursor=&limit=&include=&maxBytes=
GET    /api/artifacts/{id}/blocks/{blockId}
GET    /api/artifacts/{id}/events?sinceRevision=&cursor=&limit=
GET    /api/artifacts/{id}/source
GET    /api/artifacts/{id}/export/{format}
```

`/api/artifacts` 是唯一在线资源树；`/api/docs/**`、`/content`、`/operations` 和旧 Paragraph JSON
均不得恢复。schema 演进通过明确的版本迁移
完成，协议和业务逻辑不做双写双读。

## 开发与验证

```bash
cargo fmt --all
cargo test --workspace
cd web && pnpm build:document-engine-wasm && pnpm typecheck && pnpm test && pnpm build
```

后端默认 `http://127.0.0.1:8787`，前端默认 `http://localhost:5174`，Vite 将 `/api` 代理到
后端。Rust crate 改动后必须重新构建 canonical WASM binding；前端不加载旧 Canvas 文本会话。

## 改代码时注意

- 修改 `oo-schema` 后同步 `web/packages/schema/src/artifact.ts`，并补充边界校验测试；
- 新增 Document 能力时增加 semantic command、typed mutation 与 engine 单测，不在 React 中直接改模型；
- 所有持久化写入必须通过 Artifact snapshot，revision 与 transactionId 必须保持一致；
- block id、root/children 关系必须由 schema 校验，未知 block 应保留原始数据；
- 新增 Spreadsheet/PPT/脑图/白板能力时使用各自模型和 typeId，不污染 Document engine；
- 兼容迁移只允许存在于数据库 schema 演进或离线导入脚本，不得成为在线编辑路径。

架构背景与决策见 `docs/architecture.md`、`docs/adr/` 和 `docs/cutover.md`。
