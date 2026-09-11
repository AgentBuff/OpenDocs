# open-office 剩余开发任务施工图（2026-09-10）

> 执行基线：P0、P1、P2A、P2B、P2C、P2D-01、P2D-02 已完成。  
> 当前入口：P2D-03 Mindmap 交换格式。  
> 上位约束：`AGENTS.md`、`docs/architecture.md`、`docs/plans/development-roadmap-2026-09.md`。

## 1. 使用方式

每个编号是一项可独立审查、可回滚的纵向工作包，不是单纯前端页面任务。一个工作包只有在
schema/engine/server/Web/浏览器/文档所需层次同时闭合后才能标记完成。共享协议或数据库变更必须先于
消费方合并；renderer 不得持有可写领域状态。

统一完成定义：

- 所有持久化写入通过领域 semantic command 和不可变 Artifact snapshot；
- revision、transactionId、history、event、asset reference 在同一提交内一致；
- capability 只声明真实可调用能力，缺失能力 fail closed；
- 正向、拒绝、原子失败、undo/redo、reload 和跨版本迁移均有证据；
- Rust format/test/strict Clippy，Web typecheck/unit/architecture/contract/build，相关 Chromium 与
  `git diff --check` 全绿。

## 2. 关键路径与并行边界

```text
P2D-03a 合同/fixture
 ├─ P2D-03b JSON/Markdown ─┐
 ├─ P2D-03c FreeMind      ├─ P2D-03g 交换格式汇合
 ├─ P2D-03d XMind         │
 └─ P2D-03e SVG ─ P2D-03f PDF
                           └─ P2D-04 大图/协作

P2E-01 状态/入口 ─ P2E-02 history/session ─ P2E-03 typed objects ─ P2E-04 camera/render
                                                        └─────────────── P2E-05 assets/export

P2D/P2E 完成 ─ P3-01 identity ─ P3-02 realtime ─ P3-03 conflict ADR/fixture ─ P3-04 multi-instance
                                                                    └──── P3-05 operations
P3 完成 ─ P4-01 loading ─ P4-02 perf ─ P4-03 browser/a11y/i18n ─ P4-04 release ─ P4-05 AI readiness
```

可并行：P2D-03b/c/d 在 03a 后可并行；P2E-01 可与 P2D-03 并行；P4 的测量工具可提前建设，但发布
阈值只能在 P3 的部署模型冻结后签署。不可并行：同一 schema version、同一 migration 或同一 capability
contract 的修改必须串行。

## 3. P2D-03 Mindmap 交换格式

### P2D-03a 能力矩阵与安全合同

- 用户结果：导入/导出界面能准确说明“保留、降级、拒绝”的内容。
- 代码落点：`docs/architecture/`、`fixtures/mindmap/`、`oo-schema` exchange DTO。
- 任务：冻结 XMind、FreeMind、canonical JSON、Markdown、SVG、PDF 的字段映射；定义
  `unsupportedFeature/malformedPackage/externalResource/assetMismatch/resourceLimit` loss taxonomy；限制
  ZIP entries、解压总量、XML/JSON 深度、节点数、文本长度和图片大小。
- 明确不做：不把外部格式字段塞入 node attrs；不把导入兼容分支放进在线编辑 engine。
- 验收：能力矩阵逐项对应 fixture；恶意 ZIP/XML/JSON 全部 fail closed；strict/audit 语义有 contract test。

### P2D-03b Canonical JSON 与 Markdown

- 用户结果：JSON 可完整备份/恢复高级结构和资产 manifest；Markdown 可稳定交换树、note 与 link。
- 代码落点：`oo-mindmap` exchange service、`oo-server` import/export、Web/SDK 下载上传。
- 任务：版本化 JSON envelope；asset id/checksum/MIME manifest；跨 Artifact 导入重建 id；Markdown frontmatter
  与 extension block 保存无法表达的 Summary/Boundary/Formula loss；统一文件名与 MIME。
- 验收：JSON semantic diff 为空；Markdown 支持子集往返为空，其他能力产生确定 loss；悬空或伪造资产拒绝。

### P2D-03c FreeMind `.mm`

- 用户结果：可导入常见 FreeMind 层级、文本、note、link，并收到明确损失报告。
- 代码落点：新建独立 importer crate/module，不进入 `oo-mindmap` transaction path。
- 任务：流式 XML 解析；深度/实体/尺寸限制；稳定 preorder id；HTML/text note 归一化；link 与基础颜色映射；
  icon、cloud、hook、富 HTML 等未支持项进入 loss report。
- 验收：最小、Unicode、深树、note/link、未知字段、XXE/billion-laughs、超限 fixture；导入结果通过 schema validate。

### P2D-03d XMind `.xmind`

- 用户结果：可导入 XMind 8/Zen 受支持子集，嵌入图片安全进入统一资产系统。
- 代码落点：独立 XMind package reader、server staged import、asset store。
- 任务：校验 ZIP 路径和压缩比；读取 `content.json`/必要 manifest；sheet/root/topic/note/link/marker 映射；
  resources 以 checksum/MIME 校验后登记并重写 asset id；多 sheet、relationship、summary/boundary 等逐项映射或报告。
- 验收：真实最小包、图片包、多 sheet、缺 part、重复路径、zip-slip、炸弹、外链、MIME 欺骗 fixture 全覆盖。

### P2D-03e SVG 导出

- 用户结果：当前主题和所有可见思维导图结构可下载为独立、可缩放 SVG。
- 代码落点：`oo-mindmap` export projection、server export route。
- 任务：只消费 canonical layout/edge/advanced projection；XML 转义；font fallback；图片以受控 data/reference 策略；
  collapsed 状态、显式 edge、Summary/Boundary/Formula 均可见；选择/hover/presence 不导出。
- 验收：结构化 SVG parser 测试、恶意文字转义、稳定 viewBox、主题 fixture、浏览器下载与无 view-state 泄漏。

### P2D-03f PDF 导出

- 用户结果：大图可按适合页面或分页策略导出 PDF，文字和线条不裁切。
- 代码落点：独立导出服务；不得把 PDF 页坐标写回 MindmapModel。
- 任务：冻结 paper/orientation/margin/fit/tile 选项；复用 SVG/vector projection；字体嵌入/替代报告；资源上限；
  多页顺序稳定。
- 验收：PDF 结构/页数/MediaBox/文本抽取与渲染截图；CJK/emoji、超宽图、多页、缺字体测试。

### P2D-03g 汇合与产品闭环

- 用户结果：导入和导出从首页/编辑器可发现，失败信息可操作。
- 任务：server routes、capability、OpenAPI、TS parser、SDK、UI；真实上传→编辑→undo→reload→下载→再导入；
  strict/audit 选择和 loss report 展示。
- 验收：所有格式 fixture semantic comparison；完整全仓门禁；支持矩阵和限制文档与运行态一致。

## 4. P2D-04 Mindmap 大图与协作

### P2D-04a 10k 数据集与预算

- 固化宽树、深树、混合树、显式边、图片和高级结构数据集。
- 分别记录 layout、route、projection、hit query、首次渲染、pan/zoom、局部编辑的 p50/p95/p99 与内存。
- 验收：算法不变量与墙钟阈值分离；CI 快速门槛和 release 基准各自稳定。

### P2D-04b 增量布局和投影缓存

- engine invalidation 只描述语义范围；layout service 按受影响祖先/分支增量重排。
- Summary/Boundary 只重算引用区间/子树；formula source 更新不重排整图。
- 验收：局部文字、折叠、移动、高级结构更新的重算节点数有硬断言；结果与全量布局等价。

### P2D-04c Worker 与取消协议

- 大图 layout/route 放入可版本化 worker request；结果带 artifactId/revision/requestId。
- 新 revision 到达时取消或丢弃旧结果；不得把 worker cache 当 canonical state。
- 验收：乱序、取消、worker crash、快速连续编辑和刷新恢复测试。

### P2D-04d 视口裁剪与空间索引

- SVG/DOM 只挂载可见节点、边和高级实体；编辑节点与焦点目标强制常驻。
- hit-test 使用 projection 空间索引，不扫描 DOM；pan/zoom 不推进 revision。
- 验收：10k 节点 mounted count 有界、hit p95 预算、IME/selection 不因裁剪丢失。

### P2D-04e 推送协作

- SSE/WebSocket 收到 durable revision event 后按 invalidation 拉取受影响 projection；presence 仅为 ephemeral。
- 验收：断线续读、乱序、重复事件、过期 revision、远端删除当前选择和 10k 图局部刷新。

## 5. P2E Whiteboard 最小完整产品

### P2E-01 入口与能力诚实化

- 首页创建后进入 WhiteboardStudio；移除占位提示。
- capability 按 edit/history/projection/import/export/assets/presence 分项，与真实命令 registry 一致。
- 验收：创建→编辑→刷新 Chromium；所有可见按钮均有实现或明确禁用。

### P2E-02 Server-authoritative history 与 session

- 新增 `whiteboard.history`，持久 journal 支持重启后 undo/redo。
- Web session 使用 projection + invalidation，不在每次命令前下载并合并完整 scene attrs。
- 验收：幂等、409、redo branch、restart、candidate cleanup、零公开 snapshot 写入。

### P2E-03 Typed 核心对象与交互

- schema/engine：rectangle、ellipse、diamond、line、arrow、text、sticky、image、group 与 connector anchor。
- UI：多选、框选、移动、八向缩放、旋转、层级、组合、复制粘贴、吸附；手势结束单事务。
- 验收：每类对象 CRUD/history/reload；非法 group/cycle/connector 原子拒绝；键盘替代和 ARIA。

### P2E-04 Camera、空间索引与 renderer

- camera 默认迁为本地 view state；跟随演示者单独走 ephemeral channel。
- Canvas/WebGL 只渲染空间索引可见对象，DOM overlay 编辑文字。
- 验收：10k 元素 hit <8ms 的 release harness；pan/zoom transaction=0；context loss 可恢复。

### P2E-05 资产与导出

- 图片走统一 asset closure/checksum/MIME/ref-count；JSON/SVG/PDF/PNG 由 export projection 生成。
- 验收：缺失/损坏/伪造资产拒绝；undo/redo ref-count 正确；导出不含 selection/camera/hover/presence。

## 6. P3 认证、协作与部署可靠性

### P3-01 身份与授权

- JWT/OIDC、workspace/member/role/share-link；生产禁用 `X-OO-User`，dev identity 仅显式 profile。
- 验收：owner/editor/viewer/share/anonymous 的读、写、导出、协作者、review、asset 越权矩阵。

### P3-02 Realtime event 与 presence

- durable revision event 使用 cursor 续读；presence 有 TTL/heartbeat 和 Artifact/user/session 配额。
- 验收：断线、重复、乱序、slow consumer、限流和多标签页恢复；ephemeral 数据不进入 snapshot/history。

### P3-03 共编冲突 ADR 与 fixture

- 分别为 Document、Spreadsheet、Mindmap/Scene Graph 比较服务器串行命令、OT、CRDT。
- 先冻结 concurrent insert/delete/move/range edit/structure edit 期望，再实施所选策略。
- 验收：不能以 last-write-wins 冒充共编；冲突或重基结果可解释、可重放。

### P3-04 多实例与存储

- 用数据库 compare-and-swap/事务锁替代进程 mutex；明确 SQLite 单节点边界或 Postgres 迁移。
- BlobStore 对接对象存储；candidate upload、DB commit、orphan reconciliation 可恢复。
- 验收：双实例竞态、进程崩溃、对象存储超时、重复提交、主从切换演练。

### P3-05 运维能力

- 备份/恢复、snapshot/event retention、quota、GC、metrics、structured log、trace、alert。
- 验收：恢复创建新 revision/审计，不改写旧 snapshot；恢复点、RPO/RTO 和故障手册有实测记录。

## 7. P4 性能与发布

### P4-01 加载与包体预算

- 按 Artifact 动态加载 Studio；Document/Mindmap WASM 延迟加载和缓存。
- 当前基线 JS 约 238.56KB gzip、Document WASM 约 571.26KB、Mindmap WASM 约 546.01KB，先冻结预算再优化。
- 验收：首屏不下载未进入的 editor/WASM；CI 超预算失败并输出增量来源。

### P4-02 性能自动化

- 归档 Document 100页、Presentation 2k、Spreadsheet 100k、Mindmap 10k、Whiteboard 10k 的时间/内存。
- 验收：固定机器 release 基准、CI 算法门槛、可比较 JSON 结果和趋势图。

### P4-03 浏览器、无障碍与国际化

- Chromium/Firefox/WebKit 基础矩阵；键盘、焦点、对比度、screen-reader semantics。
- 文案抽离；时区、日期、数字、RTL、CJK 字体/换行 fixture。
- 验收：关键纵切片三浏览器全绿；axe/手工屏幕阅读器清单签署。

### P4-04 发布签署

- migration、backup/restore、fault injection、依赖/许可证/安全扫描、CHANGELOG、breaking changes、运维手册。
- 验收：`docs/architecture/release-checklist.md` 全部关闭；unsupported capability 与 UI/文档一致。

### P4-05 AI readiness 复核

- 仅复核 capability、projection、revision、citation、permission、event 是否足够支持未来 AI 客户端。
- 明确不做：Core platform 发布门禁完成前不启动模型、Agent runtime、embedding 或向量索引。

## 8. 推荐迭代切片

| 迭代 | 工作包 | 退出条件 |
|---|---|---|
| M1 | P2D-03a～03d | 三种可编辑交换格式和安全 fixture 完成 |
| M2 | P2D-03e～03g | Mindmap 导入导出产品闭环与全仓门禁 |
| M3 | P2D-04a～04d + P2E-01 | 10k Mindmap 可交互，Whiteboard 状态诚实 |
| M4 | P2D-04e + P2E-02～03 | Mindmap 推送协作、Whiteboard typed/history 闭环 |
| M5 | P2E-04～05 | Whiteboard 10k renderer、资产和导出闭环 |
| M6 | P3-01～03 | 身份、推送与共编语义冻结 |
| M7 | P3-04～05 | 多实例、存储和运维可恢复 |
| M8 | P4-01～03 | 包体、性能、浏览器/a11y/i18n 门槛 |
| RC | P4-04～05 | 发布签署与 AI readiness 复核 |

任一迭代若需要公开 snapshot 覆盖、在线双读双写、通用 JSON patch、renderer-owned model 或恢复旧
`/api/docs/**`，必须停止实施并先提交 ADR；不得作为“临时兼容”混入普通任务。
