# 项目代码库全面分析（2026-08）

> 范围：main 分支全量静态审读 + 实测验证。
> 验证命令与结果见 §5；结论基于当日工作区状态（I00/I01 代码已提交：`e298015`、`9336013`）。

## 1. 定位

多 Artifact 办公平台：同一套平台基础设施承载 Document、Spreadsheet、Presentation、
Mindmap、Whiteboard 五种内容形态。核心设计哲学是**反"万能文档模型"**——每种 Artifact
拥有独立的 schema、engine 与 renderer，只共享身份、资源、事务、历史、导入导出等平台能力。

## 2. 技术栈与规模

| 层 | 技术 | 规模 |
|---|---|---|
| Rust workspace | 11 crates，Edition 2021 | ~42,800 行 |
| 服务端 | axum 0.8 + sqlx(SQLite) + tokio + tower-http | 10,600 行（最大 crate） |
| 前端 | pnpm monorepo：8 packages + editor app | 195 个 TS/TSX 文件 |
| 浏览器绑定 | wasm-bindgen / wasm-pack | 仅 Document（薄绑定） |

## 3. 架构分层

```text
oo-schema → oo-protocol → 各领域 engine → immutable snapshot → renderer
                               ↕
            oo-server（REST + SQLite + event outbox）
                               ↕
@open-office/schema → document-engine(WASM) → apps/editor(DOM-first)
```

- **oo-schema**：唯一持久化真相。`ArtifactEnvelope` v5 + 5 种 `ArtifactKind`；迁移链
  v1→v5 显式可查；Presentation 已完成 v4→v5 破坏式切换。Block 类型含
  Paragraph/Heading/Quote/Code/Image/Table/Callout/Todo/Divider/Page/Columns/Column/Link，
  Extension/Unknown block 保留原始 JSON 保证向前兼容 round-trip。
- **oo-protocol**：capability catalog（面向 agent/SDK/MCP 的 URI-template 自描述契约）、
  幂等事务信封（baseRevision + transactionId）、projection 契约 v2。
- **领域 engines**（单一写入口 `Engine::execute(CommandBatch)`）：
  - oo-document（5.9k）：typed mutation + ChangeSet + MutationJournal(undo/redo)、
    tri-state patch、表格 Grid projection；
  - oo-presentation（5.6k）：Scene Graph、duplicateSlide id 映射、connector 端点原子更新、
    master/layout 引用完整性、纯几何投影 `project_layout`；
  - oo-spreadsheet（3k）：稀疏 Grid + 公式依赖图（环检测/受影响子图拓扑/错误码）、viewport；
  - oo-mindmap（1.2k）/ oo-whiteboard（1.5k）：早期阶段，规模最小。
- **导入导出**：oo-docx 仅导入；oo-xlsx / oo-pptx 双向 adapter 且与 engine 单向依赖；
  显式损失报告（`PptxLossReport`、`ignored_parts`），禁止静默丢数据。
- **oo-server**：`/api/artifacts/**` 资源树；快照指针、事务幂等日志、历史、event outbox
  在同一 SQLite 事务提交；outbox at-least-once + lease；auth 为开发期固定用户但身份
  获取路径已收敛到单点（换 JWT 只改一处）。
- **前端**：editor 内部为 interaction kernel（pointerRouter/keyboardRouter/selectionReducer/
  OverlayCoordinator）+ 注册式 block behaviors；`blockProjectionStore` 按 Block 订阅；
  runtime 含 autosaveOutbox / commitApplier / conflictRebaser；Presentation Studio/
  Playback/Thumbnails 已有完整 UI。

## 4. 设计亮点

1. 单一写入口 + 强类型命令，无通用 patch/attrs 逃生口；
2. 破坏式演进纪律：迁移一次性切换，不留双读双写；
3. 对 AI/agent 友好：capability catalog 自描述，REST URI template，只读 projection；
4. 可靠性模式落地：outbox at-least-once、事务幂等表、同事务原子提交、客户端 conflict rebaser;
5. 诚实边界：损失显式报告而非伪造成功。

## 5. 工程健康度（2026-08-26 实测）

| 检查 | 结果 |
|---|---|
| `cargo fmt --all --check` | 通过 |
| `cargo test --workspace` | 272 个测试全部通过 |
| `pnpm typecheck`（含 wasm 重建） | 通过 |
| `pnpm test` | 548 个测试（78 文件）全部通过 |
| CI `quality.yml` | fmt/test/clippy -D warnings + typecheck/architecture:check/contract:check/test/build + 浏览器持久化与视觉回归 |

工程化配套齐全：9 份 ADR、release-checklist、API contract 生成校验脚本、dependency-audit、
fault-injection、browser/visual smoke、bench-editor。

## 6. 主要风险

1. **C0 关键路径悬空**：interaction kernel 代码已在，但 I00/I01 acceptance 清单
   （7+17 项）当时全部未勾选，是后续功能的前置条件（详见 core-platform-plan-review）。
2. **实时协同缺席**：现有的是 revision 乐观锁 + presence + event feed，五个阶段均未提及
   OT/CRDT；目标句中的"协作"语义需要明确。
3. **WASM 绑定只有 Document**：Spreadsheet/Presentation 在浏览器内无本地会话，虚拟渲染
   只能走 REST 往返；JSON 字符串边界在大 ChangeSet 下有序列化开销。
4. **Mindmap/Whiteboard 滞后**：引擎最小化，且无前端编辑器 UI。
5. **细节卫生**：migrations 编号跳过 0007；性能预算尚未 CI 化为自动回归门禁。
