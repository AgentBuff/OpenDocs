# open-office 开发任务路线图（2026-09 基线）

> 状态：实施中（P0、P1、P2A、P2B、P2C、P2D-01、P2D-02 已完成，当前阶段 P2D-03）  
> 基线：2026-09-10 当前工作区静态审计与全量门禁实测  
> 上位约束：`AGENTS.md`、`docs/architecture.md`、`docs/roadmap/core-platform-plan.md`

## 1. 目标与完成定义

在不引入第二套模型、在线双读双写或 renderer-side mutation 的前提下，把当前五类 Artifact
开发版收敛为：门禁可信、写入边界唯一、能力声明诚实、可安全部署、可持续发布的平台。

每个任务必须同时具备：所属 engine 的 semantic command、Rust/TS 边界同步、正向与拒绝测试、
撤销/恢复与 revision/idempotency 证据、真实 Chromium 证据、capability/文档同步和全量门禁结果。
本计划不包含 LLM、Agent Runtime、embedding、知识库或多 Agent 编排。

## 2. 当前基线

已确认可用：Rust format、355 项测试、Clippy、TypeScript、生成协议、Web 单测和生产构建；五类
Artifact 均有独立 schema/engine；统一 `/api/artifacts/**` 已具备 revision、幂等、历史、事件、资产和协作者基础设施。

当前阻断：

1. Architecture gate 因 Spreadsheet clipboard 的 `structuredClone` 失败；
2. Chromium 68/69，通过项之外有一个 Document toolbar 视觉差异；
3. `print.css` 存在无效 `.presentation-*`；
4. Vitest 重复扫描 workspace 内嵌 `node_modules`，测试数量失真；
5. 普通 owner 可通过公开 `PUT /snapshot` 直接替换完整 DocumentModel；
6. Whiteboard 的 capability、首页入口和实际成熟度不一致；
7. 认证为开发身份，部署一致性只由单进程 mutex 保证。

### 实施状态（2026-09-10）

- P0-01 已完成：clipboard projection 与坐标索引落地，100k sparse cells 的 10×10 复制只查询 100 个坐标；architecture gate 通过。
- P0-02 已完成：差异确认来自资产字体选择器的“默认字体”新语义，更新浅/深色基线后视觉组 2/2 连续通过。
- P0-03 已完成：打印 CSS 使用明确的 Document/Presentation/Spreadsheet chrome selector，生产构建无 CSS syntax warning。
- P0-04 已完成：Vitest 发现收敛为 41 个唯一源码文件、283 项测试，无嵌套 workspace 重复扫描。
- P0-05 已完成：当前证据索引已更新，既有未提交改动按八个审查/验证组登记在 `docs/plans/worktree-change-map-2026-09.md`。
- P0 汇合门禁已完成：Rust fmt、355 tests（另 1 ignored perf harness）、Clippy；generated contract、dependency audit、Web typecheck/architecture/contract、41 files/283 unit tests、production build、Chromium 69/69 全部通过。
- 上面的“当前阻断”保留为本轮开始时的输入快照；当前进入 P1-01。

## 3. 执行依赖

```text
P0 可信基线
 └─ P1 平台边界收口
     ├─ P2A Document
     ├─ P2B Spreadsheet
     ├─ P2C Presentation
     ├─ P2D Mindmap
     └─ P2E Whiteboard
          └─ P3 认证、协作、部署
               └─ P4 性能与发布
```

P2A～P2E 可并行，但共享 schema/protocol/API 的修改必须先在 P1 冻结。P0、P1 未完成前不增加新
Artifact 类型或大范围装饰性 UI。

## P0：恢复可信质量基线

### P0-01 Spreadsheet clipboard 投影

- 定义只读 `SpreadsheetClipboardProjection`，只复制选区必要字段；粘贴编译为 typed commands。
- 禁止复制 workbook/sheet；门禁应能区分选区值复制与整模型复制。
- 覆盖 values/formats/all、公式、样式、合并区域、越界、撤销和刷新。
- 验收：architecture gate 通过；100k cell sheet 复制 10×10 选区不遍历全表。

### P0-02 视觉差异定性

- 比较 Document toolbar expected/actual/diff，确认字体光栅化、布局偏移或样式回归。
- 只有人工确认设计正确后才能更新 snapshot。
- 验收：69/69 Chromium 通过，light/dark 无溢出、裁切或焦点噪声。

### P0-03 Print CSS

- 用合法容器/class 替换 `.presentation-*`；分别定义 Document、Presentation、Spreadsheet 打印边界。
- 验收：生产构建无 CSS syntax warning；打印输出无编辑 chrome。

### P0-04 Vitest 发现范围

- 排除 `**/node_modules/**`、`dist/**` 和 generated WASM；显式 include unit test 目录。
- 验收：每个源码测试文件只执行一次，本地与 CI 数量一致。

### P0-05 工作区与证据治理

- 按 schema/protocol、Spreadsheet、Mindmap、Whiteboard、字体、Presentation、测试/文档拆分当前大改动。
- 原子切换可作为一组，但每个提交必须可审查、可回滚且说明依赖。
- 更新旧代码库分析、I00/I01/I02 和能力矩阵中的旧状态与测试数量。

### P0 汇合门禁

```bash
cargo fmt --all -- --check
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
node scripts/check-generated-api-contract.mjs
node scripts/dependency-audit.mjs
cd web
pnpm typecheck
pnpm architecture:check
pnpm contract:check
pnpm test
pnpm build
pnpm test:e2e --project=chromium
```

## P1：平台写入、能力和资产边界

### P1-01 Snapshot 写入口收口

- 普通客户端只允许 semantic transaction。
- 推荐从公共 capability/OpenAPI 移除 snapshot PUT；迁移/import 使用内部 service/CLI。
- 如必须保留管理 API，要求 system principal、显式 import intent 和完整审计，不接受浏览器 owner 调用。
- 验收：普通 owner snapshot PUT 被拒绝；import/restore 仍原子产生 revision、history、event。

实施结果：已完成。公共 snapshot 路由只保留 GET，浏览器整体保存 API 与绕过 engine 的 handler 已删除；
owner PUT 返回 405，语义事务、历史恢复和 durable event 定向测试通过。

### P1-02 Capability 分项状态

- 将 Artifact 级 `stable/planned` 拆为 `edit/history/projection/import/export/assets/presence`。
- Document、Presentation、Whiteboard command catalog 改为由 engine registry 派生。
- 验收：Whiteboard 能表达“基础编辑可用、无历史、无交换格式”；广告能力不会运行时 unsupported。

实施结果：已完成。Capability contract v2 已拆成七项 feature status；五类 Artifact 的语义命令目录均由各自 engine registry 派生，server 仅追加 history intent。Whiteboard 明确为 edit preview、history planned、import/export unsupported；Rust、生成契约、Web 边界和 API 定向门禁通过。

### P1-03 统一资产引用闭包

- 五种 payload 使用统一只读 asset reference 提取接口。
- commit 同事务校验资产存在、checksum/MIME 并更新 ref_count。
- Mindmap JSON 跨文件导入必须重映射二进制资产或明确拒绝。
- 覆盖悬空引用、重复引用、删除保护、undo/redo、失败清理、GC 和损坏对象。

实施结果：已完成。`oo-schema` 提供五类 payload 的统一只读 asset-reference projection；normal/history commit 在同一数据库事务校验存在性与声明的 checksum/MIME，并精确重建 ref_count。Mindmap 裸 JSON 外部引用明确拒绝，Presentation undo/redo、删除保护、失败回滚、损坏对象和带宽限期的 opt-in 离线 GC 均有回归测试。

### P1-04 Server transaction 公共内核

- 提取协议解析、授权、幂等、409、candidate blob cleanup、事件 stamp 和 DB commit 模板。
- 领域层只提供 command decode/execute/invalidation/assets；禁止形成通用 JSON engine。
- 验收：五类适配器共享同一套幂等、冲突和失败清理 contract tests。

实施结果：已完成。新增仅负责 transport/persistence invariant 的 `transaction_kernel`；五类 adapter 共用协议解析、授权/kind、幂等 replay、409、actor stamp、normal/history commit 和候选 blob cleanup，同时保留各领域独立的 command decode、engine、invalidation 与 asset projection。server 全量契约及严格 Clippy 通过。

### P1-05 Actor、错误与审计

- 区分 authenticated principal、client actorId、origin；持久化作者只信任 server principal。
- 错误统一返回 `code/requestId/retryable/details`；409 尽可能返回 changed entities。
- 验收：客户端不能伪造审计作者；SDK 按稳定错误码刷新、重试或停止。

实施结果：已完成。事务日志分别持久化 authenticated author、client actor 与 origin，domain event 作者只信任认证 principal；错误统一包含 code/requestId/retryable，409 汇总 requested revision 之后的 changed entities。SDK 提供冲突与 retryable 分类器，防伪审计、错误解析、Rust/Web 门禁均通过。

## P2A：Document 专业闭环

### P2A-01 搜索与导航

- engine/query：find/replace、heading outline、TOC projection；replace-all 是一个可撤销批次。
- UI：查找栏、上下匹配、替换、目录侧栏。
- 覆盖 CJK、Emoji、跨 run/block、大小写、刷新和撤销。

实施结果：已完成。canonical Document engine 提供树序 Unicode scalar 搜索、stable block/table-cell
目标、单项过期校验替换和单 journal 批次的全部替换；WASM 只读 query、REST `/toc`、SDK、编辑器查找栏与
目录侧栏已贯通。Rust 48 项 Document 测试、Server 25+16 项 API、Web 42 files/285 tests、production
build、生成契约及真实 Chromium 查找→替换→撤销→刷新回归均通过。

### P2A-02 页面语义

- schema：section、header/footer、page number、footnote/endnote 强类型模型。
- renderer：新增分页/打印 projection，不把分页坐标写入 snapshot。
- DOCX：逐能力导入导出并报告 loss。

实施结果：已完成。schema v7 增加 section、header/footer、page number、footnote/endnote 及稳定文本锚点，v1～v6 仅通过离线迁移逐版本升级；Document engine 新增 4 个页面语义命令并以单一 `SetPageSemantics` mutation 支持原子回滚、撤销和重做。projection contract v3、WASM、REST `/projection/documentPrint`、SDK 与 DOM/print renderer 只传递逻辑节范围，不持久化分页坐标。编辑器可维护页眉页脚、起始页码、脚注和尾注；DOCX section/page numbering 可往返，尚未映射的 header/footer/footnote/endnote 在导入和导出均进入结构化 loss report。全仓 Rust 373 tests（1 ignored perf harness）、strict Clippy、Web 42 files/285 tests、typecheck、architecture/contract/build 与 Chromium 71/71 全部通过。

### P2A-03 评论、建议和 presence

- comment thread、mention、suggestion 使用稳定 entity/revision 引用，不塞进 RichText attrs。
- Document 远端 cursor/selection 保持 ephemeral；删除/移动后引用不悬挂。

实施结果：已完成。评论线程、显式 mention 和 suggestion 持久化在独立 review 表中，不进入 RichText attrs 或 Artifact snapshot；锚点使用稳定 block/table-cell id、Unicode scalar range 与创建 revision，并在每次读取时对当前不可变 snapshot 重解析，移动保持有效、版本变化标记 stale、删除或越界返回 detached 且隐藏失效 anchor。接受建议通过既有 `replaceTextMatch` semantic command 并保存后才更新 review 状态。Document presence 复用 30 秒 TTL 内存通道，写入要求当前 revision，读取会清除 revision 或实体引用已失效的参与者。Rust、严格 TS parser、SDK、审阅侧栏、远端块标记及真实 Chromium 回归均已接入。

### P2A-04 大文档和无障碍

- 100 页/1000 block 可见区挂载，当前编辑 block 不被虚拟化卸载。
- 完成 IME、屏幕阅读器、键盘表格、图片替代文本和焦点恢复矩阵。
- 验收：输入 P95 <16ms，滚动无持续 >50ms 长任务。

实施结果：已完成。Document renderer 在稳定根 Block slot 上使用单一 IntersectionObserver 和 ResizeObserver，
只挂载视口前后 1000px 内的 Block 子树；当前 active block 所属根子树强制常驻，结构命令后的焦点恢复允许
跨 React/虚拟列表提交帧重试，打印前同步全量挂载、打印后恢复虚拟化。编辑区增加 document landmark 与隐藏
占位语义；正文和表格单元格均在 IME compositionend 后提交，表格支持 Tab/Shift+Tab 焦点导航及既有
Shift+方向键 stable range。图片 `alt` 已进入 schema/engine typed patch、Unicode 长度校验、工具栏编辑和刷新
恢复。真实 Chromium 以 100 section/1000 block fixture 验证挂载块数受限、当前编辑块不卸载、打印闭包、输入
P95 `<16ms` 且滚动无连续 `>50ms` long task；完整 74/74 回归通过。

## P2B：Spreadsheet 专业闭环

### P2B-01 Projection-driven session

- workbook 结构与 viewport 分离；滚动只请求 bounded projection。
- 提交后按 invalidation 刷新窗口/metadata，不再重读完整 workbook。
- 验收：100k sparse cells 下滚动和单格编辑没有全模型网络响应。

实施结果：已完成。Spreadsheet session 启动、提交和冲突恢复均改为结构投影与 history，结构响应只含
workbook/sheet metadata 且拒绝夹带 cells；Grid 只请求有界窗口并把当前投影单元格提供给 Studio，离屏
merge anchor 仍使用独立有界查询。提交后按新 revision 清空窗口缓存并重取结构/当前视口，不访问完整
snapshot。既有 100k materialized-cell Rust 基准验证窗口最多返回 2500 cells；真实 Chromium 网络测试
验证启动与单格提交均为零 snapshot 请求、投影响应小于 256KB。Server、Web 全量门禁及 Chromium
75/75 回归通过。

### P2B-02 Paste/fill 领域命令

- Clipboard projection 编译为 engine-level paste/fill command，避免浏览器逐格展开大批命令。
- 实现相对/绝对/跨 sheet 公式引用重写和 CSV/TSV 系统剪贴板。
- 验收：10k cell 粘贴为一个 history entry，可撤销、可冲突重试。

实施结果：已完成。新增 engine-owned `pasteRange`/`fillRange`，稀疏 clipboard cells、粘贴模式、
清空、样式和数据校验统一在一次 `RangeChanged` 内执行。公式复制支持相对/绝对轴、范围、引号字符串、
显式跨 sheet 前缀与越界 `#REF!`。浏览器复制按有界 projection 分块读取，系统剪贴板支持带引号的
TSV/CSV、数字/布尔值与公式，内部复制保留 attrs/style。真实 Chromium 注入 409 验证以最新 revision
自动重试一次；100×100 系统粘贴只有一个 command/一个 history entry，撤销完整清空。Rust engine 66、
server 42+28+16、Web 42 files/290 tests、严格 Clippy/构建/架构/契约和 Chromium 78/78 全绿。

### P2B-03 公式与数据能力

- 建立函数矩阵、日期/时区/错误传播规范；扩展 lookup、统计、文本和财务函数。
- 完成数据验证、筛选、条件格式编辑 UI；不支持函数返回 typed error。

### P2B-04 XLSX fidelity

- 分项覆盖 styles、shared formula、named range、merge、freeze、filter、validation 和 conditional format。
- 图表、图片、宏等未支持内容进入结构化 loss report；以 semantic diff 而非“能打开”验收。

实施结果：已完成。schema v8 引入 workbook-level typed named range 与显式 v7→v8 离线迁移；
普通/共享公式、active sheet、命名区域、typed cell style、merge/freeze、filter/sort、validation 和
conditional format（含 `dxfs/dxfId` differential style）均具备导入导出与 canonical semantic diff
回归。命名区域会随行列结构编辑重映射并随 sheet 删除清理，active sheet 删除也在同一可逆事务中
切换。图表、图片、宏、pivot、外链、comments、theme/indexed color 等进入完整结构化 loss report，
严格读写不会静默丢失。全仓 Rust 392 passed/1 ignored、strict Clippy、Web 42 files/291 tests、
typecheck/architecture/contract/build 和 Chromium 80/80 全绿；能力矩阵见
`docs/architecture/spreadsheet-xlsx-fidelity.md`。

## P2C：Presentation 专业闭环

### P2C-01 拆分 PresentationStudio

- 拆为 session、stage、selection/gesture、text editing、inspectors、timeline、playback。
- 容器建议 <500 行；helper/registry 独立测试；拆分不得改变 transaction/selection 语义。

状态：完成。`PresentationStudio` 拆分完成时缩至 494 行，P2C-02 接入对象交互后仍保持 499 行；session、selection、gesture、actions、stage、
text editing、inspectors、timeline 和 playback 形成独立边界；拖拽仍采用 preview→pointer-up 单批
semantic commit，selection/playback 保持纯 view state。Vitest 已修正 Presentation 同目录测试发现，
全量 50 files/333 tests、Rust 392 passed/1 ignored、strict Clippy、typecheck、architecture、
contract、build、diff check 与 Chromium 80/80 全绿。模块边界见
`docs/architecture/presentation-editor-modules.md`。

### P2C-02 文本与对象交互

- composition-aware rich text、段落、列表、auto-fit、对齐和范围格式。
- 多选、组合、锁定、对齐、分布、吸附、旋转、层级、键盘微移使用 preview→单次 commit。

状态：完成。schema v9 将 Presentation RichText 段落、对齐、列表与缩进提升为严格 canonical
结构，v8→v9 只迁移已知 RichText 位置并保留 extension opaque data；PPTX 段落/列表/CJK/Emoji
往返已覆盖。文本编辑在 compositionend 后按 Unicode scalar range 单次提交；舞台、缩略图和播放层
共同消费 vertical alignment 与真实 shrink-text auto-fit。对象侧完成八向缩放、旋转、参考线、键盘
微移、多选、层级、锁定、组合/取消组合与 canonical group transform；connector 不可被错误纳入组变换。
`PresentationStudio` 保持 499 行。全仓 Rust 397 passed/1 ignored、strict Clippy，Web 52 files /
344 tests、typecheck、architecture、contract、build、diff check 和 Chromium 83/83 全绿。

### P2C-03 Timeline 与演讲者模式

- 时间线编辑、切换效果、seek-safe 播放、备注和双窗口状态。
- 播放进度属于 view operation，不推进 Artifact revision。

实施拆分：

1. 先以纯函数把按 `orderKey` 排序的 animation entries 编译为 cue graph，冻结三种 trigger、
   delay/duration、零时长、空时间线和跨 slide 前后导航语义；clock 只保存 slide/cue/elapsed 游标。
2. TimelinePanel 增加从选中对象创建、字段编辑、拖动/键盘排序和删除；每次结束动作只发一个既有
   `upsertAnimation`、`moveAnimation` 或 `deleteAnimation`，不回写整个 timeline。
3. 播放 renderer 按可寻址时间计算 transition 与对象效果，支持 play/pause/seek/restart、快速跳页及
   reduced-motion；相同游标在任意刷新帧必须产生相同画面。
4. speaker notes 继续是 Slide typed field，编辑需要 IME-safe 明确提交和 history/reload；播放 projection
   通过 `include=notes,timeline,nodes` 读取，观众舞台不渲染 notes。
5. presenter 控制台与观众窗口共享短期同源 session，只同步 sessionId、revision 与 view cursor；提供
   当前/下一页、备注、计时、导航、断线恢复和 popup-blocked 单窗口降级，绝不提交 Artifact transaction。
6. 验收覆盖 pure-state、engine history、UI capability、Chromium 双窗口/键盘/ARIA/网络断言；所有播放
   和 presenter 操作的 transaction 请求数必须为 0。

状态：完成。timeline entries 会编译为确定性 step-local cue，播放 cursor 仅保存稳定
`slideId/cueId/elapsedMs/status`；play/pause/seek/restart/前后导航可从任意游标重建。renderer 实际消费
transition、delay、duration 与四类 entrance preset，并支持 reduced-motion。TimelinePanel 支持新增、字段
编辑、专用拖动把手、键盘上下移和删除；备注保留原始空白并具备 IME、undo/redo、reload 证据。
演讲者控制台提供当前页、下一页预览、备注、计时与导航，观众窗口通过严格 version/session/artifact/
revision cursor 协议同步，刷新可重连，popup blocked 会降级单窗口；消息不含 Deck，播放期间 transaction
与 revision 增量均为 0。Presentation engine 28/28、Web 54 files/351 tests、architecture/contract/build、
strict Clippy、定向 presenter 3/3 和完整 Chromium 86/86 全绿；Studio 保持 495 行。

### P2C-04 PPTX fidelity 与 E2E

- text/shape/image/table/chart/connector/media/master/layout/theme/notes/animation 分项能力矩阵。
- 增加 create/edit/undo/reload/play/import/export 浏览器场景。
- 支持子集 semantic diff 为空；不支持项严格拒绝或报告。

状态：完成。能力矩阵冻结在 `docs/architecture/presentation-pptx-fidelity.md`。notesSlide/
notesMaster、Fade/Push/Wipe/Cut transition、四类入口 animation timing、text bodyPr 与 slide name 均已双向
映射；chart/media/master/layout/theme 等未支持项保持结构化 strict/audit 拒绝。package traversal、dangling
relationship、宏/外链及 entry/XML/media 超限均 fail closed。`oo-pptx` 25/25、Rust workspace、strict
Clippy、Web 54 files/351 tests、architecture/contract/typecheck/build、LibreOffice 消费验证、定向 fidelity
1/1 与完整 Chromium 87/87 全绿。

## P2D：Mindmap 专业闭环

### P2D-01 关联线 UI

- 创建、选择、改端点、标签、样式和删除；命中几何只来自 projection。
- 覆盖 history、刷新、删除节点清理引用和键盘操作。

状态：完成。`updateEdge` 已支持 omitted/Some/null label 三态，端点/标签更新保持原子可逆；显式关联线
只使用 projection route 生成可点击 hit path、标签与拖动端点。拖动期间仅保存候选节点和预览线，pointerup
提交一个 typed command；检查器提供起终点、标签、线型、颜色、宽度、虚线及删除的键盘替代，并按
engine capability 分项禁用。删除端点节点会原子清理 edge，undo 可恢复。Mindmap engine 15/15、Web
54 files/351 tests、Mindmap Chromium 12/12、完整 Chromium 88/88、strict Clippy、architecture/contract/
typecheck/build 与 fmt/diff check 全绿。

### P2D-02 富文本与高级结构

- 节点内 composition-aware RichText editor；概要、外框和公式定义为 typed graph entities。
- 禁止继续扩张 node attrs 保存一等能力。

实施拆分与状态：

1. **02a 合同冻结（完成）**：Unicode scalar range、IME 单次提交、Summary 同父正向兄弟区间、
   Boundary 子树根、Formula 节点锚点、v9→v10 离线迁移、清理/剪贴板/投影规则已写入架构合同。
2. **02b 节点富文本（完成）**：`replaceNodeText`/`patchNodeTextRange`、真正 tri-state patch、
   composition-aware contenteditable、DOM↔scalar selection 与 selection-aware 格式工具已贯通。
3. **02c canonical entity（完成）**：schema v10、Rust/TS 严格校验、三类 entity CRUD、typed
   mutation/inverse、结构删除/移动清理、clipboard v2 重映射与 v1 显式升级已完成。
4. **02d projection 与 UI（完成）**：summary bracket、boundary rect、formula anchor 均由 layout
   派生；SVG/DOM 命中、inspector、键盘选择/删除和 capability gating 已接入。服务端投影已统一消费
   canonical `MindmapProjection`，不再拼装旧 DTO。
5. **02e 验收（完成）**：10k 节点含高级结构的完整投影在 debug test 中约 0.33 秒；纯公式 source/
   概要标签更新保持局部失效，概要区间更新触发结构失效。Rust workspace 411 passed/1 ignored、strict
   Clippy、Web 54 files/351 tests、architecture/contract/typecheck/build、Mindmap 14/14、完整 Chromium
   90/90 和 diff check 全绿。

### P2D-03 交换格式

- XMind/FreeMind 导入；JSON/Markdown/SVG/PDF 导出；图片资产重映射。
- 每种格式都有 loss report 和 fixture semantic comparison。

### P2D-04 大图与协作

- 10k 节点 layout/route、视口裁剪、局部重排和 worker 策略。
- presence 改为推送；远端 revision 只刷新受影响 graph projection。

## P2E：Whiteboard 最小完整产品

### P2E-01 入口与状态诚实化

- 首页创建后进入 WhiteboardStudio，删除“正在接入”提示。
- capability 精确列出当前支持项；增加创建→编辑→刷新 E2E。

### P2E-02 历史和 session

- 增加 server-authoritative `whiteboard.history`，重启后 undo/redo 有效。
- UI 使用统一 session，去掉每次命令前手工下载并合并完整 scene attrs。

### P2E-03 核心对象

- typed rectangle/ellipse/diamond/line/arrow/text/sticky/image/group，避免扩张自由 attrs。
- 多选、移动、缩放、旋转、层级、复制粘贴、框选、吸附和 connector anchor。
- 手势只更新 preview，结束时提交一个事务。

### P2E-04 Camera 与渲染

- 默认将 pan/zoom 迁为本地 view state；“跟随演示者”另建 ephemeral presence operation。
- Canvas/WebGL 只渲染空间索引返回的可见对象；DOM overlay 编辑文字。
- 验收：10k 元素命中 <8ms，视口移动不推进 revision。

### P2E-05 资产与导出

- 图片接入统一资产闭包；提供 JSON、SVG/PDF/PNG export projection。
- 缺失资产拒绝提交；导出不包含 selection/camera/hover。

## P3：认证、协作与部署可靠性

### P3-01 身份与授权

- JWT/OIDC、用户/workspace/member/role、分享链接和最小权限。
- 生产删除 `X-OO-User`；开发身份仅在显式 dev profile 开启。
- 覆盖 owner/editor/viewer/share-link/匿名越权矩阵。

### P3-02 实时事件和 presence

- SSE/WebSocket 推送 durable revision event 与 ephemeral presence；断线按 cursor/revision 续读。
- presence 增加 TTL、心跳和每 Artifact/用户/会话上限。

### P3-03 共编策略 ADR

- 分别评估 Document、Spreadsheet、Scene Graph 的 OT、CRDT 或服务器序列化命令。
- 先冻结冲突语义 fixture，再实现；不得用最后写入覆盖冒充协作。

### P3-04 存储与多实例

- 用数据库级并发控制替代进程 mutex；明确 SQLite 单节点或迁移 Postgres。
- BlobStore 接入对象存储；candidate upload、DB commit、orphan reconcile 可恢复。
- 验收：双实例、进程崩溃、对象存储超时、重复提交和恢复演练。

### P3-05 运维能力

- 备份/恢复、snapshot/event 保留、配额、GC、metrics、structured logs、trace 和告警。
- 恢复生成新 revision 和审计记录，不改写旧 snapshot。

## P4：性能与发布

### P4-01 前端加载预算

- 按 Artifact 动态加载 editor；Document/Mindmap WASM 延迟加载和缓存。
- 建立 JS/CSS/WASM gzip budget，CI 超限失败。

### P4-02 性能自动化

- 归档 Document 100 页、Presentation 2k、Spreadsheet 100k、Mindmap 10k、Whiteboard 10k
  的 p50/p95/p99、内存和 invalidation 范围。
- 墙钟阈值与算法不变量分开，避免 CI 抖动掩盖复杂度退化。

### P4-03 浏览器、无障碍和国际化

- Chromium/Firefox/WebKit 基础矩阵；键盘、屏幕阅读器、焦点和对比度。
- UI 文案抽离；时区、日期、数字、RTL、CJK 字体与换行进入 fixture。

### P4-04 发布签署

- 执行 migration、backup/restore、fault injection、依赖/许可证和安全扫描。
- 更新 CHANGELOG、breaking changes、unsupported capability 和运维手册。
- `docs/architecture/release-checklist.md` 最终签署项全部关闭。

### P4-05 AI 前置复核

- 只复核 capability、projection、revision、citation、权限和事件能否支持未来 AI 客户端。
- Core platform 未达到 release gate 前，不启动模型、Agent Runtime 或向量索引。

## 4. 代码落点与责任边界

| 工作流 | 主要代码落点 | 边界要求 |
|---|---|---|
| 持久化模型与校验 | `crates/oo-schema`、`web/packages/schema` | Rust/TS 同步，未知节点 round-trip，版本迁移显式 |
| 协议与服务端提交 | `crates/oo-protocol`、`crates/oo-server` | revision、transactionId、幂等、history、event 原子一致 |
| Document | `crates/oo-document`、`crates/oo-document-wasm`、`web/packages/document-engine`、`web/apps/editor/src/blocks`、`interaction`、`runtime` | `DocumentEngine::execute` 是唯一业务写入口，WASM 保持薄绑定 |
| Spreadsheet | `crates/oo-spreadsheet`、`crates/oo-xlsx`、`web/packages/spreadsheet-ui`、`web/apps/editor/src/spreadsheet` | Grid/公式/范围命令独立，不借用 Document Block |
| Presentation | `crates/oo-presentation`、`crates/oo-pptx`、`web/packages/presentation-ui`、`web/apps/editor/src/presentation` | Scene Graph 与 DOM text overlay 分层，手势只提交语义结果 |
| Mindmap | `crates/oo-mindmap`、`crates/oo-mindmap-wasm`、`web/packages/mindmap-engine`、`web/apps/editor/src/mindmap` | Graph、layout projection 和 view state 分离 |
| Whiteboard | `crates/oo-whiteboard`、`web/apps/editor/src/whiteboard` | Scene state、camera、selection/presence 分离 |
| 门禁与 E2E | `scripts`、`web/scripts`、`web/tests`、CI 配置 | 本地与 CI 使用相同发现范围、浏览器矩阵和证据口径 |
| 架构与发布证据 | `docs/architecture.md`、`docs/adr`、`docs/architecture` | capability、迁移、限制与运行态同步，不保留过期“已完成”表述 |

共享层修改由平台工作流先落契约和 contract test；产品工作流只消费公共接口，不在各 Studio 内复制
revision、幂等、资产引用或错误处理逻辑。

## 5. 建议实施批次

| 批次 | 任务 | 汇合结果 |
|---|---|---|
| 1 | P0-01～P0-04 | 当前工作区全绿 |
| 2 | P0-05、P1-01、P1-02 | 写边界与能力状态冻结 |
| 3 | P1-03～P1-05 | 平台契约 vNext |
| 4 | P2E-01/P2E-02、P2B-01、P2C-01 | 五编辑器状态诚实、前端边界可扩展 |
| 5 | P2A～P2E 剩余纵切片 | 专业编辑闭环 |
| 6 | P3 | 可安全部署和协作 |
| 7 | P4 | 发布候选版本 |

## 6. 单任务模板

```text
任务 ID：
用户结果：
涉及领域：schema / engine / protocol / server / adapter / renderer
明确不做：
前置依赖：
迁移或删除项：
semantic commands：
invalidation/assets/history：
测试：Rust / TS / Browser / Perf / Fault
文档与 capability 更新：
回滚方式：
验收证据：
```

如任务需要恢复兼容路径、通用 JSON patch、公开 snapshot 覆盖或 renderer-owned model，必须暂停并先提交 ADR，
不能在普通功能 PR 中隐式引入。
