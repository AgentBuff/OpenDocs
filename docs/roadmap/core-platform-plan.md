# 基础能力实施总计划

> 状态：当前主线  
> 范围：不依赖 AI 的多 Artifact 平台、领域引擎与专业编辑体验  
> 配套未来计划：[`ai-native-future-plan.md`](ai-native-future-plan.md)

## 1. 目标与边界

本计划的目标不是先做一个“AI 文档”，而是先把一个可长期承载 Document、Spreadsheet、Presentation、Mindmap 和 Whiteboard 的产品内核做扎实。完成标准是：人类用户可以可靠地创建、编辑、导入、导出、恢复和协作处理 Artifact；任何普通客户端都可以通过稳定协议读写它。

基础能力包含：

- Artifact 身份、快照、revision、事务幂等、历史和事件；
- 各 Artifact 的强类型 schema、semantic command、engine 和投影；
- 编辑器选择、输入、快捷键、菜单、浮层、对象和表格等可测试交互；
- 资产、导入导出、错误恢复、性能、无障碍和发布质量。

本计划明确不包含 LLM、prompt、embedding、向量检索、Agent run、跨会话 memory、多 Agent 编排、AI 审批或模型供应商接入。它们被隔离到未来计划，不能以“预留接口”为名绕过本计划的领域边界。

## 2. 不可妥协的工程规则

```text
schema → domain engine / semantic transaction → immutable snapshot
                                                ↘ renderer / client projection
```

1. Artifact 的写入只走所属 engine 的 semantic command；Document 固定为 `DocumentEngine::execute(DocumentCommandBatch)`。
2. DOM、Canvas、SVG、WebGL、React store 都是视图或交互投影，不能持有可持久化领域状态。
3. 不引入通用 JSON patch、客户端整份快照回写或 renderer-side mutation。
4. 所有跨端写入携带 revision 和 transaction id；服务端原子提交 snapshot、历史、幂等记录和事件。
5. 每一项“完成”都要有代码、自动化测试和可复现验收证据；UI 演示或空菜单不算能力完成。
6. 公共 REST 面向所有客户端保持中立；AI 将来只是其中一种客户端，不获得隐藏写入路径。

## 3. 当前基线

现有已实施的主线是：

- `I00-quality-baseline`：质量门禁、浏览器基础设施和缺陷工作流；
- `I01-interaction-kernel`：Document 的 selection/overlay/behavior registry 拆分。

这两轮中仍存在未关闭验收项，尤其是 I00 的视觉回归证据、I01 的 table behavior 收敛、overlay/renderer 清理与旧路径删除。它们是后续功能的前置条件，不应被新的大功能越过。

## 4. 实施顺序与交付物

### C0 — 关闭质量与交互内核

**目的**：让 Document 编辑器具备可扩展且可验证的交互底座。

**代码重点**：

- 完成 `I00-quality-baseline` 的视觉基线、浏览器回归覆盖与 release evidence；
- 完成 `I01-interaction-kernel` 的 pointer/keyboard router、overlay coordinator、block behavior registry；
- 将 `TableBlockView` 剩余选区、几何、拖拽和上下文命令收敛为注册式 table behavior；
- 清除重复的全局 listener、焦点推断式选择状态和无语义 `z-index`；
- 补齐 CJK/Emoji 原生选区、图片移动/缩放/裁剪/键盘前后插入、表格拖拽、合并拆分和 portal 叠层的浏览器测试。

**完成门槛**：`I00` 与 `I01` 的 acceptance 逐项有可重复证据；`BlockNode` 与 renderer 满足既定代码规模和行为边界；任何交互状态都可由 interaction store 和 typed command 解释。

### C1 — Artifact 平台契约收口

**目的**：让 Web、CLI、SDK、同步服务和未来外部客户端共享一个稳定平台契约。

**代码重点**：

- 完成能力发现、outline/block/entity projection、cursor 分页和稳定错误 envelope；
- 统一 `If-Match`、`baseRevision`、transaction id 与重复提交结果；
- 完成 artifact event feed、历史查询、资产引用计数与垃圾回收策略；
- 将 OpenAPI/JSON Schema/SDK 生成保持在 Rust typed contract 单源之下；
- 将认证、权限、审计 actor 设计为通用 Principal seam，而非 AI 专用字段。

**完成门槛**：所有公开路由只使用 `/api/artifacts`；读投影可在有界大小下分页；写入可安全重试；任何客户端无需解析 DOM 或完整快照即可进行局部读取和语义写入。

### C2 — Document 专业编辑器闭环

**目的**：把 Document 从“可编辑 Demo”推进到可靠的专业编辑器。

**代码重点**：

- 完成段落、标题、列表、引用、代码、待办、图片、表格、附件等 block 的一致选择和上下文操作；
- 完成图片对象的移动、八点缩放、裁剪、删除、键盘前后插入与资产生命周期；
- 完成表格的单元格编辑、选区、行列选择、边框、格式、合并拆分、相邻边界调整和上下文菜单；
- 完成 import/export 损失报告、DOCX 媒体关系、打印/分页预览和可访问性；
- 完成 autosave 节流、离线/冲突恢复、撤销重做和异常恢复。

**完成门槛**：每个 block 有明确 schema、command、behavior、renderer、菜单描述和浏览器用例；关键编辑路径在真实 Chromium 中验证；导入导出能力与损失边界可被用户看见。

### C3 — 多 Artifact 领域能力

**目的**：在同一平台内建立独立而不互相污染的 Spreadsheet、Presentation、Mindmap 和 Whiteboard 引擎。

**代码重点**：

- Spreadsheet：sparse Grid、公式图、选择与虚拟渲染、XLSX adapter；
- Presentation：Slide Scene Graph、master/layout、shape/text/table/connector 命令、局部 layout projection、PPTX adapter；
- Mindmap：Graph、layout projection、节点编辑与导入导出；
- Whiteboard：无限 Scene Graph、空间索引、视口与对象操作；
- 为每类 Artifact 建立相同的 snapshot、transaction、projection、event 与 asset 边界，但绝不复用 Document Block 作为内部模型。

**完成门槛**：每种 Artifact 至少具备一个从创建、编辑、持久化到导入/导出或明确 loss report 的完整路径；没有 renderer 直接写模型或通用 `attrs`/`patch` 逃生口。

### C4 — 产品可靠性与发布准备

**目的**：将功能性原型收敛为可部署、可维护、可回归的产品基础。

**代码重点**：

- 完成 access control、workspace/member/role、共享链接和审计记录的通用产品能力；
- 完成附件 object store、备份/恢复、数据迁移、配额、可观测性与故障注入；
- 完成性能预算：大 Document、2k scene、100k grid、10k whiteboard 的测试与降级；
- 完成 accessibility、国际化、主题和跨浏览器行为基线；
- 将 release checklist、视觉快照、依赖/许可证审计接入 CI。

**完成门槛**：发布检查清单全部通过，故障、迁移和回滚路径都有演练证据；性能预算不是文档声明而是自动化数据。

## 5. 依赖关系

```text
C0 交互与质量内核
 └─ C1 平台契约与可靠写入
     ├─ C2 Document 专业编辑闭环
     └─ C3 多 Artifact 领域能力
          └─ C4 发布可靠性与协作基础
```

`C2` 与 `C3` 可以在 `C1` 的协议、资产和测试契约稳定后按不同 Artifact 并行推进；但它们都不能绕过 C0 的交互/测试边界或 C1 的事务边界。

## 6. 本计划对 AI 计划提供的唯一前置物

基础计划只提供中立的可复用能力：stable id、结构投影、revision、semantic command、capability discovery、事件、通用权限与审计。未来 AI 计划必须建立在这些能力之上，不能反向要求基础层加入 prompt、模型、向量库或 Agent 状态。

详见 [`ai-native-future-plan.md`](ai-native-future-plan.md)。
