# core-platform-plan.md 评审（2026-08）

> 对 [`docs/roadmap/core-platform-plan.md`](../roadmap/core-platform-plan.md) 的逐条核实与评审。
> 核实方式：对照 I00/I01 acceptance 清单、CI workflow、interaction kernel 源码与测试实测。

## 1. 计划的三个立场

- **反 scope creep**：明确排除 LLM/prompt/embedding/Agent run/memory 等一切 AI 概念，
  并禁止以"预留接口"名义绕过；对 AI 计划保持单向依赖（AI 只消费中立前置物）。
- **反万能抽象**：schema → engine → snapshot 分层，禁止通用 JSON patch、整份快照回写、
  renderer-side mutation。
- **反演示式完成**："UI 演示或空菜单不算能力完成"，每项完成需代码 + 测试 + 可复现证据。

这是带完成门槛的执行契约，不是愿景文档。

## 2. 阶段 × 代码现实对照

| 阶段 | 计划内容 | 代码证据 | 状态 |
|---|---|---|---|
| C0 交互与质量内核 | 关闭 I00/I01 验收项 | interaction kernel 已落地（pointerRouter/keyboardRouter/selectionReducer/OverlayCoordinator）；I00 acceptance 7 项、I01 17 项，评审时全部未勾选 | 🔴 名义前置，未闭环 |
| C1 平台契约收口 | capability/projection/幂等/OpenAPI 单源/Principal seam | catalog、projection v2、outbox+lease 已落地；contract 生成校验脚本与 sdk 包存在；auth 仍为 dev 用户 | 🟡 大部分已落地 |
| C2 Document 编辑闭环 | 图片变换/表格合并拆分/DOCX 媒体/打印分页/冲突恢复 | block 类型齐全；conflictRebaser/autosaveOutbox 存在；图片 crop/resize/move 控制器已提交；DOCX 无导出、打印分页未见 | 🟡 中段 |
| C3 多 Artifact 能力 | 四引擎独立 | Spreadsheet/Presentation 引擎成熟，XLSX/PPTX 双向 adapter 带 loss report；Mindmap/Whiteboard 薄且无前端 UI | 🟢 两强两弱，跑在计划前面 |
| C4 发布可靠性 | access control/备份/性能预算自动化/a11y/i18n | release-checklist 与脚本存在；CI 含 architecture:check/contract:check/视觉回归；workspace/role/审计为零 | 🟠 工具链先行，产品能力空白 |

## 3. 优点

1. **基线诚实**：§3 直接点名 I00 视觉回归证据、I01 table 收敛未关闭——与 acceptance
   文件的实际勾选状态一致，没有粉饰；
2. **门槛可证伪**：C3"每种 Artifact 至少一条完整路径"、C4"性能预算不是文档声明而是
   自动化数据"；
3. **并行策略明确**：C2/C3 在 C1 后可并行，但不得越过 C0/C1 边界；
4. **AI 接口克制**：只交出 id/projection/revision/command/capability/event/权限审计。

## 4. 风险与缺口

1. **核心矛盾：计划顺序 vs 代码现实倒挂**。依赖图要求 C0→C1→(C2∥C3)，现实中 C3 的
   Presentation Studio、Spreadsheet 引擎已大量建成而 C0 的 24 项验收一项未关。无论
   解读为追认式重组还是有意让引擎先行，最大交付风险都是 C0 长期悬空导致新 UI 继续绕开
   behavior registry。计划预言了该风险但未给出纠偏机制（replan 触发条件）。
2. **协作语义缺席**：目标句写"协作处理 Artifact"，五阶段却无实时协同（OT/CRDT）安排；
   若只指 revision 乐观锁 + presence，应写明。
3. **WASM 战略未回答**：C3 要求 Spreadsheet 虚拟渲染、Whiteboard 视口操作，隐含客户端
   本地引擎会话；目前仅 Document 有 WASM 绑定。这是 C3 的隐藏架构决策点。
4. **a11y/i18n 整体压到 C4**：CJK/Emoji 测量已在 C0 提及是正确的，但 i18n 回改成本高，
   至少文本测量/换行应提前。
5. **门槛锚点不明确**："既定代码规模"未定义出处；全计划无时间/容量估算。
6. **卫生细节**：migrations 缺 0007 编号，侵蚀"稳定契约"的可信度。

## 5. 结论与建议动作

计划质量高于平均水平，且仓库工具链证明团队有能力兑现"证据文化"。当前真正的关键路径
是关闭 C0：

1. **集中关闭 I00/I01**：把 24 项 acceptance 逐项对应到具体测试文件并补齐缺口，
   未达标项显式记录 exception；
2. 给 Mindmap/Whiteboard 补齐"最小完整路径"（CLI/REST 级创建→编辑→导入导出）以满足
   C3 门槛下限；
3. 用一句话明确"实时协同是否在本计划范围内"，消除目标句歧义；
4. 在 C1 内顺手清理 migration 编号跳档等契约卫生问题。
