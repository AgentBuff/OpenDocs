# open-office 目标架构

open-office 是一个多 Artifact 文档平台，而不是把所有产品能力塞进一个“万能文档模型”。

```text
Workspace
└── Artifact
    ├── Document      Block Tree + RichText
    ├── Spreadsheet   sparse Grid + Formula Graph
    ├── Presentation  Slide Scene Graph
    ├── Mindmap       Graph + Layout
    └── Whiteboard    Infinite Scene Graph + Spatial Index
```

## 当前分层

通用客户端（包括 AI work agent、知识库 projector、SDK 和 CLI）的 API 设计与执行顺序见
[`docs/architecture/generic-client-api-architecture-plan.md`](architecture/generic-client-api-architecture-plan.md)。
该方案只完善 REST、projection、capability、幂等和事件契约，不引入 Agent Runtime 或 LLM 能力。

当前工程主线与未来 AI 能力已分开管理：基础平台、领域引擎和专业编辑器按
[`docs/roadmap/core-platform-plan.md`](roadmap/core-platform-plan.md) 实施；AI 原生协作层仅保留为满足
基础前置条件后的未来计划，见 [`docs/roadmap/ai-native-future-plan.md`](roadmap/ai-native-future-plan.md)。

```text
Platform Kernel
├── oo-schema       持久化模型、结构校验、版本字段
├── oo-protocol     ArtifactCommandEnvelope / CommitResult / event envelope
├── oo-document     Document Block Tree engine
├── oo-docx         .docx → DocumentModel importer
├── oo-xlsx         .xlsx ↔ SpreadsheetModel adapter
├── oo-pptx         .pptx ↔ PresentationModel adapter
├── oo-presentation Presentation Scene Graph engine
├── oo-spreadsheet  Spreadsheet sparse Grid engine
├── oo-mindmap      Mindmap Graph engine
├── oo-mindmap-wasm canonical Mindmap engine 的浏览器薄绑定
├── oo-whiteboard   Whiteboard Scene Graph engine
└── oo-server       axum + SQLite + object store

Browser
├── @open-office/schema  Artifact 消费类型与网络边界校验
├── @open-office/document-engine  `oo-document` 的 typed WASM adapter（只返回 ChangeSet）
├── @open-office/mindmap-engine   `oo-mindmap` 的 typed WASM adapter（只返回 projection/ChangeSet）
├── @open-office/ui       主题 Token、Toolbar/Menu/Popover 等无业务 UI 原语
└── apps/editor          Document DOM 编辑器与各 Artifact 专业编辑器
```

### UI 基础设施边界

`@open-office/ui` 是浏览器壳层的设计系统，不持有 Artifact、Document Block 或编辑命令。它分为两层：

- `theme.css` 只声明语义 Token（颜色、表面、边界、文字、主色、阴影和工具栏尺寸），当前提供
  `office-light` 与 `office-dark` 两套主题；切换主题只改变 `ThemeProvider` 的主题名，不改业务 JSX。
- `components.css` 与 `primitives.tsx` 提供 Button、Toolbar、ToolbarGroup、MenuPanel、MenuItem、
  Surface 等可组合原语，只负责交互语义、键盘焦点和视觉层级。

编辑器领域组件可以组合这些原语，但不能把 DocumentCommand、BlockProjectionStore 或 Artifact API
塞进 UI 包。`apps/editor/src/styles/` 只保留编辑器专属布局（页面、Block、代码块、表格和业务工具），
共享控件样式不得回流到这里。这样未来切换腾讯文档、深色或品牌主题时只替换 Token，而不是复制一套
Toolbar/Menu JSX。

`oo-schema::DocumentModel` 是持久化模型的唯一真相；`oo-document::DocumentEngine` 是
Document 写入的唯一入口，`oo-document-wasm` 只是该 engine 的浏览器薄绑定。React 只负责视图、输入和命令触发，服务端只接受
`ArtifactCommandEnvelope`，并返回 typed `CommitResult`。

## 编辑与持久化流

```text
DOM input
  → semantic DocumentCommandBatch
  → oo-document::DocumentEngine::execute
  → typed ChangeSet / CommitResult
  → immutable Artifact snapshot
```

每次事务带 `baseRevision` 和 `transactionId`。文档快照指针与事务幂等日志在同一个
SQLite 事务中提交，重试不会重复插入 block。`/api/artifacts/{id}/snapshot` 是只读投影；
所有在线编辑只使用 `/api/artifacts/{id}/transactions`。文件导入和历史恢复由服务端受控入口
原子生成新的不可变 snapshot，不向浏览器暴露完整模型覆盖能力。

服务端元数据、revision 指针、快照登记、事务幂等、历史和 durable event outbox 统一存储在
`artifacts`、`artifact_snapshots`、`artifact_transactions`、`artifact_history` 和
`artifact_event_outbox`。`ArtifactKind` 直接复用 `oo-schema` 的定义；资源路由按 kind 选择领域适配器。
迁移会一次性复制旧 Document 表后删除旧表，不保留兼容视图、旧表双读或双写；公共 HTTP 只保留
`/api/artifacts` 资源树。

成功提交的 `DomainEventRecord` 同时写入 `artifact_event_outbox`，与 Artifact snapshot、事务幂等记录
处于同一个 SQLite 事务中；任一事件写入失败，整个提交回滚。事件消费采用 at-least-once 语义，
`eventId` 是下游幂等键，pending 事件可记录尝试次数、退避时间和最后错误。当前 outbox 由服务端
数据库 adapter 提供轮询/确认接口，worker 与外部消息系统属于后续部署层，不进入 Document engine。

事务日志只保留语义 command batch 的审计摘要，持久化列为 `commands_json`。迁移
`0005_command_log.sql` 破坏式移除旧 operation-log 列名；运行时不做双读/双写，后续 schema
演进必须继续通过显式 SQL migration 完成。

## Renderer 选择

Renderer 是可替换的视图层，不得成为领域模型或持久化格式：

- Document：DOM-first 编辑，后续可增加 Canvas 页面预览/打印；
- Spreadsheet：虚拟 Grid，当前单元格用 DOM overlay；
- Presentation：Canvas/SVG/WebGL，文本编辑用 DOM overlay；
- Mindmap：SVG/Canvas，节点文本用 DOM overlay；
- Whiteboard：Canvas/WebGL，使用视口裁剪和空间索引。

Canvas 的职责是高效绘制图形和分页预览，不承担文档状态、选区、输入法或事务逻辑。

Spreadsheet 浏览器会话把 workbook 结构和 cell viewport 明确分开：无 viewport 参数的
`/projection/spreadsheet` 只返回 sheet/metadata 结构，所有 sheet 的 `cells` 必须为空；Grid 按
`sheetId + row/column window` 请求有界单元格投影。提交后客户端只用新 revision 失效并重取结构与
当前窗口，不读取完整 snapshot，也不在 renderer 保存第二份 workbook。离屏合并锚点通过额外的有界
投影补齐。十万稀疏单元格的 engine 基准只构造窗口结果，浏览器网络回归同时禁止启动和提交后的
`/snapshot` 请求。

Spreadsheet 剪贴板与填充同样是 engine-owned range transaction。浏览器复制时按最多 1000×200 的
有界窗口分块读取 canonical cells（含 adapter attrs），内部投影携带源坐标；`pasteRange` 一次提交稀疏
矩阵、模式和可选源坐标，`fillRange` 在引擎内从 canonical source 重复填充。相对 A1 引用按目标位移，
`$` 锁定轴保持不动，显式跨 sheet 前缀保留；越界引用变成 `#REF!`。CSV/TSV 系统剪贴板没有可信
源坐标，公式按原文写入。无论 1 格还是 10,000 格，提交只生成一条 `RangeChanged` mutation 和一个
history entry；409 只用最新 revision 重试一次同一语义命令。

Spreadsheet 公式支持面、日期系统、UTC 时钟、typed error 传播以及筛选/条件格式/数据验证的求值边界
由 [`spreadsheet-formula-contract.md`](architecture/spreadsheet-formula-contract.md) 固化。公式结果始终是
只读 projection；数据规则只能通过 engine-owned semantic command 修改。

## 可扩展边界

Block 只属于 Document。表格、幻灯片、脑图和白板拥有各自的模型与 engine，但共享
Artifact 身份、资源、权限、历史、协同和导入导出基础设施。新增能力时：

1. 在对应 schema 中增加模型字段并保持 `schemaVersion` 明确；
2. 在对应 engine 中增加 semantic command 与 typed mutation，不把业务分支塞进平台 protocol；
3. 通过 capability registry 注册 renderer、editor、menu、command 和生命周期；
4. 让 renderer 消费模型和几何结果，不复制业务状态。

### Presentation Scene Graph（N6 首批）

Presentation 不复用 Document Block，也不把 Canvas 状态写回模型。`oo-presentation` 的唯一写入口是
`PresentationEngine::execute(PresentationCommandBatch)`：命令只表达 slide/scene element 的语义意图，
提交结果携带可逆的 `PresentationMutation` 和协议层 `Invalidation`。scene node 的父子关系只能由结构命令
演进；对象更新必须选择具体的强类型命令（例如 `setTextFrame`、`setShapeStyle`、`setShapeGeometry`），不存在
`attrs`、通用 `patch` 或 renderer-owned property bag。删除子树时同步解除父引用，避免悬挂引用和父子双写。

`PresentationChangeSet.invalidation` 使用 `presentation.slide`、`presentation.element` 两类实体引用，
渲染器可以仅重绘变更 slide/element；属性更新不会错误地标记 `structureChanged`。事务失败或最终 schema 校验
失败时由 mutation 逆操作回滚，revision 不前进。Scene Graph 的 typed mutation 可转换为通用
`oo_protocol::MutationRecord`，但 protocol 不依赖 Presentation 的内部结构。

复制幻灯片使用 `presentation.duplicateSlide`，而不是客户端读取、改写再提交整份 Deck。命令必须携带
目标 slide id、顺序键，以及 source→target 的完整 node / animation id 映射；engine 在同一事务中改写父子、
connector、timeline 引用并保留既有 asset 引用，因此 undo/redo、缩略图失效与事件订阅仍然是局部且可验证的。

Presentation Table 是 Scene Node 内的独立严格 Grid。行列增删、合并与拆分必须使用具体
`presentation.*Table*` 命令；引擎保持 top-left anchor、完整覆盖与非重叠 merged spans，并只记录受影响
`TableNode` 的逆操作。UI 不得通过序列化整个 table 或向 renderer 写入临时 cell patch 来实现结构编辑。

Connector 同样是独立的 Scene Node 领域边界。端点更新只能通过
`presentation.setConnectorEndpoints` 同时提交 start/end；engine 在写入前验证 target 非空、非自身且位于
同一 slide，随后由 schema 复核 anchor 与引用完整性。端点、Canvas path、hover handle 都不能被拆成 renderer
侧 patch：失败事务会以同一条 inverse 恢复两端，并局部失效 connector 所在 slide。

Master 与 Layout 也是 Deck 的一等领域实体。`create/update/deleteMaster` 与
`create/update/deleteLayout` 分别接收完整的强类型实体，不存在局部 JSON patch；engine 在同一原子事务内
校验 master→layout、layout→slide、layout placeholder→master placeholder 的引用。仍被 layout 或 slide
使用的实体不能删除；更新后若破坏既有引用，最终 schema 校验会回滚整个批次。Master/Layout 变更通过
`DeckProjection` 的反向索引只失效依赖它们的 slide 与缩略图。

Presentation 的几何查询由同一 crate 提供纯 projection：`project_layout` 返回 slide 内所有 element 的
world-space oriented rect，`world_rect` 查询单个 element，`project_dirty_layout` 只返回 invalidation 命中的
element 及其后代。父矩阵只作为计算前置条件，不会扩大 dirty 输出；rotation 按父子矩阵组合，projection 不产生
屏幕坐标，也不保存 Canvas/DOM 状态。

浏览器的 `BlockProjectionStore` 只保留 Block 引用和 root/pageSetup/revision 索引，不持有可写的
`SnapshotEnvelope` 或 `DocumentModel`；Session 仍是唯一 command 入口，React 通过按 Block 的
`useSyncExternalStore` 订阅局部更新。

Document 大文档渲染只在根 Block 边界做视口虚拟化：页面与稳定 slot 保持完整顺序，slot 通过共享的
`IntersectionObserver` 挂载视口前后预取区内的 `BlockNode`，并用测量高度维持离屏布局。嵌套子树跟随根
Block 一起挂载；包含当前 active block 的根子树无条件常驻，避免输入法、原生 selection 或对象键盘导航
因卸载丢失焦点。打印事件会同步切换到全量 Block projection，打印结束再恢复视口挂载。该机制只管理
renderer 生命周期，不缓存或修改领域数据。

无障碍输入仍由 DOM adapter 转译为 semantic command：正文和表格单元格在 composition 期间不提交，
compositionend 后一次读取 DOM RichText；表格 Tab/Shift+Tab 依稳定 cell 元素顺序恢复焦点，Shift+方向键
继续扩展 stable-id range。图片替代文本是 `ImageBlockPatch` 的强类型持久字段，具有 2048 Unicode scalar
上限；屏幕阅读器读取 canonical `alt`，而不是题注或临时 DOM 属性。

### Mindmap Graph（完整编辑链路）

Mindmap 的持久化真相是 `oo-schema::MindmapModel`，包含 parent graph、显式关联边、布局与连线设置、
节点样式以及备注、链接、标记、图片资产引用。结构和属性只能通过
`MindmapEngine::execute(MindmapCommandBatch)` 修改；每个命令产生可逆的 typed mutation、局部
invalidation 和新 revision。服务端把 inverse journal 与 Artifact revision 一起持久化，因此撤销/重做
跨进程重启仍然成立，且不会把 React、SVG 或浏览器内存当作历史真相。

`MindmapIndex` 为节点、children、深度和结构查询建立 O(n) 索引。八种布局均由 canonical Rust engine
生成 world-space projection，节点尺寸由文本和富内容计算，边路由只包含字符串 id 与几何点；屏幕 pan、
zoom、主题、选区、拖拽预览和框选均为本地 view state，不进入 snapshot。大图只渲染视口附近节点，
projection 本身保持完整且确定。

浏览器通过 `oo-mindmap-wasm` 和 `@open-office/mindmap-engine` 使用同一份 Rust projection；WASM 层只做
版本化 snapshot/command JSON 的解析与序列化，不复制 graph、布局或历史算法。编辑器将键盘、拖拽、
剪贴板、右键菜单、属性面板等交互翻译为 semantic command，经服务端 revision/transactionId 校验后再
读取不可变 snapshot。保存期间写控件被冻结，避免并发 UI 意图基于同一旧 revision 提交。

图片先写入通用 Artifact asset store，节点只保存已验证的 `assetId`；五类 payload 共用只读
asset-reference projection，提交 snapshot 时在同一数据库事务内校验存在性、可用的 checksum/MIME
并维护精确引用计数。在线 snapshot JSON 仍不内嵌二进制；只有离线交换包携带封闭资产集合。
版本化 `.mindmap.json` 交换包携带完整资产 manifest、checksum、MIME 和受限 base64 数据；导入时重建
Artifact-local asset id，悬空、伪造或多余资产均拒绝。Markdown、FreeMind 与 XMind 通过独立 importer
进入同一当前 schema，SVG/PDF 只消费 canonical projection。详细交换合同见
[`mindmap-exchange-contract.md`](architecture/mindmap-exchange-contract.md)。协作者选区和光标复用通用
presence 通道，属于有过期时间的临时状态；远端持久化变化通过带 revision cursor 的 SSE 推送，并按
invalidation 选择 canonical WASM 增量刷新或可取消 Worker 全量刷新。10k 空间索引、裁剪和性能预算见
[`mindmap-large-map-collaboration.md`](architecture/mindmap-large-map-collaboration.md)。

Document 评论、显式 mention 与 suggestion 是独立协作元数据，不进入 RichText attrs 或 Artifact snapshot。
它们通过稳定 block/table-cell id、Unicode scalar range 和创建 revision 锚定；读取时针对当前不可变
snapshot 重解析，目标移动仍有效，版本变化标记 stale，实体删除或范围越界返回 detached。接受 suggestion
必须重新校验原文本并通过 Document semantic transaction；Document cursor/selection 仅驻留带 TTL 的
presence 内存投影，revision 推进或目标失效后立即清除。

参考项目对照、架构取舍和后续扩展边界见
[`docs/reviews/mindmap-reference-analysis.md`](reviews/mindmap-reference-analysis.md)。

### XLSX adapter 边界

`oo-xlsx` 独立处理 XLSX 的 ZIP/XML package，不依赖也不进入 `oo-spreadsheet` engine。当前通道覆盖
workbook 设置与命名区域、稀疏单元格、普通/共享公式、typed style、合并与冻结、筛选与排序、数据校验
和条件格式；条件格式的 differential style 通过 `dxfs/dxfId` 往返。严格读取拒绝任何已识别但不能
建模的内容；审计读取返回结构化 loss report。导出前同样先检查 canonical model，禁止静默丢失 attrs、
媒体或其他未映射语义。验收使用 canonical `semantic_diff`，不以 ZIP 字节相同或“Office 能打开”为准。
逐项能力和损失分类见
[`spreadsheet-xlsx-fidelity.md`](architecture/spreadsheet-xlsx-fidelity.md)。

### PPTX adapter 边界

`oo-pptx` 与 `oo-presentation` 保持单向依赖：适配器只负责 ECMA-376 ZIP/XML 与
`PresentationModel` 的格式转换，不能调用或复制 Presentation engine。首批导入解析
`ppt/presentation.xml`、presentation relationships、slide 名称、基础 shape/text、几何
transform，并将关系目标归一化为 slide parts；导出生成包含内容类型、根关系、presentation、关系和
slide parts 的最小 PPTX package，同时附带空白 slide master/layout 及关系，避免 Office 打开时依赖隐式
修复。Presentation 的文本/样式、组合、演讲者备注、主题 token 和动画顺序通过独立 semantic
command 写入 Scene Graph；Canvas/SVG/WebGL 只消费 layout projection。主题、媒体、动画、图表、
表格、group/嵌套 shape 和未建模 shape 属性如果超出当前 writer 能力，不再变成无上下文的异常：
`inspect_pptx`、`parse_pptx_with_report` 和 `write_pptx_with_report` 返回稳定的
`PptxLossReport`，列出 capability、part 和 detail，供 UI/agent 展示并在后续 writer 能力上线后
回写。严格 `parse_pptx`/`write_pptx` 仍可由需要零损失的调用方选择。导入导出 round-trip 测试只验证
adapter 边界，不把 PPTX XML、原始 ZIP 或渲染状态写入 canonical snapshot。

发布前必须执行 [`release-checklist.md`](architecture/release-checklist.md)：Rust/Web 静态门禁、
真实 Chromium 9222 CDP 冒烟、视觉 manifest、2k scene/100k grid/10k whiteboard 性能预算、
故障注入和供应链/许可证审计均是独立验收项。

## 性能预算

- 普通文本输入 P95 < 16ms；
- 1000 个 block 首次可交互时间 < 1s；
- 单 block 修改只失效当前 block 及受影响容器；
- 100 页文档滚动不触发全量重绘；
- 白板 10,000 个简单元素保持可交互，超阈值启用空间索引；
- 表格 100,000 个稀疏单元格只渲染视口附近区域。

这些指标要由基准测试验证，不能以体感代替数据。
