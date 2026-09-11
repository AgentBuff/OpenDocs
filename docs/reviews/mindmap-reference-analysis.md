# Mindmap 参考项目与 open-office 实现分析

## 结论

`freeze/mind-map` 的优势是功能覆盖和浏览器交互成熟，最有价值的资产是布局策略、插件化功能拆分、
节点内容测量与大图渲染经验。它本质上仍是以 renderer 为运行时中心的单机 JS 库，不适合直接成为
open-office 的持久化与协作内核。

open-office 采用相反的主从关系：强类型 Graph 是真相，engine 负责事务和历史，projection 负责几何，
React/SVG 只呈现并产生意图。当前五阶段实现已经吸收参考项目的主要产品能力，同时保留 Artifact 平台的
revision、幂等、资产、事件和重启恢复语义。

## `freeze/mind-map` 的架构

参考代码由两部分组成：

- `simple-mind-map`：无框架 JS 库，入口 `MindMap` 创建 SVG 画布并组合核心对象；
- `web`：Vue 2 + Element UI 产品壳，负责菜单、配置、本地文件和外围页面。

库内主要关系如下：

```text
MindMap facade
├── Event / KeyCommand
├── Command ── command handlers + JSON snapshot history
├── Render ─── renderTree + node instances + selection + text editor
│   └── Layout strategy ── geometry + line drawing
├── View ───── pan / zoom / fit
└── Plugins ── drag, search, minimap, rich text, export, cooperate, ...
```

### 数据和命令

输入数据是嵌套树，每个节点的 `data` 同时容纳内容、样式和功能字段。初始化时补 `uid`，随后复制到
`Render.renderTree`。大量命令注册在 Render 或插件上，通过字符串名称执行；节点实例的 `setData()`、
父子数组以及 renderer cache 在操作中被直接修改。

`Command` 的优点是扩展简单，一个命令名可注册多个处理器，插件能快速接入。代价是命令参数没有稳定的
wire schema，插件可以触达共享实例，边界主要依赖约定。

### 历史和协作

历史从 `renderTree` 复制整棵树，序列化为 JSON 字符串并按数量截断。实现对单机编辑直观可靠，但大图下
复制成本随整图增长，也无法自然表达 Artifact revision、事务幂等或服务端重启后的历史。

Cooperate 插件把树展平成 uid map 后同步到 Yjs，并用 awareness 表达用户选择。这验证了“持久化图状态”
与“临时感知状态”需要分开，但同步入口仍然回到 renderer 的 `updateData()`，权限、schema 和服务端事务
并不是这个库的职责。

### 渲染、布局和插件

Render 同时持有数据树、节点实例缓存、当前选择、文本编辑器和布局对象。布局以类策略拆分，覆盖逻辑图、
双向脑图、目录、组织图、时间轴、垂直时间轴和鱼骨图；每个布局既算几何也绘制线条。LRU 节点缓存、
可视区域判断和性能模式是成熟的大图优化手段。

插件面很丰富：Drag、Select、Search、MiniMap、RichText、AssociativeLine、Export、XMind、PDF、公式、
协同等。`full.js` 通过集中注册生成全功能构建，基础入口也允许按需加载。这种“能力模块化”值得保留，
但插件不能在 open-office 绕过 canonical engine。

## open-office 当前实现

```text
oo-schema MindmapModel
        ↓ validate / migrate
oo-mindmap semantic command → typed mutation + inverse + invalidation
        ↓                                      ↓
server immutable snapshot + durable history   pure layout/edge projection
        ↓                                      ↓
REST revision/events/assets/presence     oo-mindmap-wasm
        └──────────────────────────────→ React + DOM/SVG renderer
```

### 模型与协议

schema v6 将设置、节点样式、边样式以及备注、链接、标记、图片引用显式建模，并提供 v5→v6 迁移。
节点用字符串 `id`/`parentId` 组成有根图，显式 edge 独立存在。Rust 与 TypeScript 都在网络边界校验；
未知 Document block 的兼容策略不会污染 Mindmap Graph。

所有写入是 `ArtifactCommandEnvelope` 内的 `mindmap.*` semantic command。engine 生成可逆 mutation，
失败批次逆序回滚，最终再次校验模型；服务端再校验 base revision、transactionId 和资产引用并提交
不可变 snapshot。capability 从 command registry 派生。

### 投影与 UI

canonical engine 提供八种布局、内容驱动节点尺寸和 edge routes。浏览器按需加载同一 Rust 实现的 WASM，
所以前后端不会因两套布局代码产生漂移。route 的 `parentId`、`childId` 是稳定字符串，SVG 只消费 points。

编辑器已覆盖主题创建、同级/子级插入、重命名、删除、折叠、拖拽 before/child/after、多选框选、复制剪切
粘贴、快捷键、上下文菜单、搜索、大纲、小地图、缩放/适配、八布局、三种连线、节点形状和颜色、整段
粗体/斜体/下划线、备注、链接、标记与图片。写操作保存期间冻结，防止相邻 UI 事件用同一个旧 revision。

当图超过 600 个节点时，主画布只挂载视口附近节点；索引、完整 projection 和小地图仍覆盖全图。
5,001 节点基准验证 engine 的索引、布局和路由热路径，而不是用 DOM 体感代替性能证据。

### 导入导出与协作

服务端支持 Markdown 和版本化 Mindmap JSON 的导入导出。Markdown 保留层级文本与备注，JSON 保留完整
canonical model。图片通过通用资产 API 上传，节点只持有经验证的 asset id，snapshot 提交同步维护引用。

presence 保存协作者选区与 world-space 光标，具有独立生命周期，不增加 Artifact revision。持久化远端变化
通过 Artifact event/revision 通知并重载 canonical snapshot。当前是服务端权威、乐观 revision 的协同模型；
若未来加入 CRDT，它应位于命令传输/合并层，不能让 renderer 成为另一份数据真相。

## 关键取舍对照

| 维度 | `freeze/mind-map` | open-office |
| --- | --- | --- |
| 数据真相 | Render 持有的嵌套树和 node 实例 | 版本化 `MindmapModel` snapshot |
| 写入口 | 字符串命令、插件与节点实例方法 | 强类型 semantic command batch |
| 历史 | 浏览器整树 JSON 快照 | typed inverse journal + durable revision |
| 布局 | JS 布局类直接操作 renderer/node | Rust pure projection，浏览器同源 WASM |
| renderer | 数据、选择、编辑、几何高度耦合 | React/SVG 只消费 snapshot/projection |
| 扩展 | 插件可访问共享 `MindMap` 实例 | capability + schema/engine/UI 分层扩展 |
| 协作 | Yjs map + WebRTC awareness 插件 | 服务端事务/event + ephemeral presence |
| 资产 | 节点数据可带 URL/base64 | 通用 asset store + 已验证 asset id |
| 并发 | 单实例命令与本地历史为主 | base revision + transactionId + 冲突重载 |

## 五阶段落地映射

1. 领域基础：强类型 schema、Graph engine、O(n) 索引、typed mutation、能力注册、持久化撤销/重做。
2. 专业编辑：键盘、拖拽、多选、剪贴板、右键菜单、搜索、大纲、小地图与画布导航。
3. 布局渲染：八种布局、内容尺寸、边路由、多主题、同源 WASM projection。
4. 丰富内容：节点样式、富文本、备注、链接、标记、图片资产、Markdown/JSON 导入导出。
5. 平台化：presence、远端 revision event、大图视口裁剪、5k 节点性能测试和真实 Chromium E2E。

## 后续扩展规则

- 新增 XMind、FreeMind、SVG/PDF 等格式时，放进独立 adapter，并返回明确的 loss report；不要在 renderer
  解析文件或把原始文件塞入 canonical snapshot。
- 新增概要、外框、公式或任务状态时，先判断是持久化领域能力还是纯视图能力；前者必须走
  schema→migration→command→mutation→server→TS parser 的完整链路。
- 需要更强实时协同时，在 revision/event 之上设计有测试的合并协议；presence 仍保持临时，资产与权限仍
  由服务端验证。
- 布局和路由只输出 world-space 几何；Canvas/WebGL 优化可以替换 renderer，但不得改变模型或命令语义。
