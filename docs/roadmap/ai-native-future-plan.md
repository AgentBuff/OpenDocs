# AI 原生协作层未来计划

> 状态：明确延期，不在当前主线实现或验收  
> 前置计划：[`core-platform-plan.md`](core-platform-plan.md)

## 1. 为什么单独拆出

AI 能力不应作为编辑器功能的捷径。若在基础模型、事务、权限、审计和资产体系未稳定时加入 Agent runtime，最终会形成第二套写入路径、不可审计的隐式状态和无法回滚的模型副作用。

因此，当前仓库先完成通用 Artifact 平台。未来的 AI 层只通过同一份 REST、capability、projection、semantic command 和 event 契约与平台交互；它不会让模型模拟点击 UI，也不会直接改 React/DOM/Canvas 状态。

## 2. 严格边界

| 基础层当前允许 | AI 层未来才允许 |
|---|---|
| capability discovery、projection、event、revision、通用 Principal | model provider、prompt、tool policy、Agent run、memory |
| semantic command 与 engine 校验 | 基于 command 的提案、执行、审批与回滚体验 |
| 通用审计 actor、权限和资产引用 | AI provenance、token/cost、模型与工具调用审计 |
| 外部系统消费 event 后自行建索引 | 知识图、embedding、检索、组织记忆 |

禁止新增 `/api/agent/**` 作为绕开 Artifact engine 的写入接口。未来即使有 MCP、SDK、Skill 或后台 worker，写入仍必须构造并提交原有的 `ArtifactCommandEnvelope`。

## 3. 启动前置条件

以下条件全部满足前，AI 计划只保留为文档，不进入代码：

1. C0 的交互和测试内核关闭，关键编辑动作有浏览器回归；
2. C1 的 capability、局部 projection、冲突、幂等、事件和通用 audit seam 稳定；
3. 至少一个 Artifact（Document）完成 C2 的编辑、资产、历史和导出闭环；
4. 具备 workspace/member/role、访问审计、数据保留和删除策略；
5. 定义清楚产品对象：面向个人知识、团队协作还是企业私有化，不能以“通用 Agent”替代目标用户。

## 4. 未来阶段

> 任务级展开见 [`../architecture/agent-integration-task-plan.md`](../architecture/agent-integration-task-plan.md)：
> A2 → T1（MCP server）、A1 → T2（检索）、A0 → T3（知识对象）、A3 → T4（AgentRun）。

### A0 — 可追溯的知识对象（不接模型）

在不调用 LLM 的前提下，定义可复用的 `Claim`、`Evidence`、`Decision`、`Task`、`Citation` 等工作对象及关系。它们要么是独立 Artifact，要么是明确的结构化 block extension；不能以散落 metadata 或富文本约定实现。

验收：每条关系指向稳定 Artifact/entity/revision；删除、移动、导出和权限变化不会产生悬挂引用。

### A1 — 检索与项目记忆投影

在 Artifact 事件的外部消费者中构建索引、检索和项目级上下文投影。索引是可重建的派生数据，不是真相来源；citation 必须回链到 artifact、entity 与 revision。

验收：索引落后、重建、权限收回和原文删除均可处理；没有任何检索结果能绕过 source 权限。

### A2 — MCP/SDK/Skill 适配层

将基础层的 capability、JSON Schema、projection 和 command 封装为机器可发现的工具。工具按“读取投影、生成提案、执行已批准命令、获取事件”分类，不设计模拟浏览器操作的工具。

验收：一个无 UI 的外部客户端可用 stable schema 完成读取、dry-run、提交、冲突重试和 citation；与普通 REST 客户端产生相同的 engine 结果。

### A3 — Agent Run 与人类审批

在平台之外或作为受控应用服务建立 `AgentRun`、工具调用日志、预算、暂停/恢复、审批点与回滚入口。AI 修改默认先形成命令提案；高风险写入必须经策略和用户审批后进入 canonical engine。

验收：每一次修改可以追溯到 run、输入来源、工具结果、命令批次和提交 revision；撤销的是语义命令，不是“重新提示模型”。

### A4 — 多 Agent 协作与组织记忆

在 A0–A3 稳定后再实现角色分工、任务租约、冲突协商、共享上下文、长期记忆与企业策略。多 Agent 不是共享一个聊天记录，而是对同一份版本化工作对象执行受约束的并发任务。

验收：可解释任务归属、停机/超时恢复、权限最小化、跨 Agent 冲突与审计导出；任何 Agent 的权限都可即时撤销。

## 5. 技术原则

- **事实与推理分离**：Artifact snapshot 是事实；检索、embedding、摘要、建议和模型输出都是可重建派生物。
- **命令优先**：AI 只能提交与人类客户端相同的 semantic command，不能持有万能 patch 权限。
- **引用优先**：生成的结论、摘要和决策必须保存 source revision/citation，而不是只保留自然语言答案。
- **权限优先**：读取、索引、工具调用、缓存、日志和导出均使用同一个 Principal/Policy 判定。
- **模型可替换**：模型供应商适配器位于应用服务边缘；schema、engine、protocol 与 renderer 不依赖任何模型 SDK。
- **可拒绝自动化**：每一步都支持 dry-run、审批、预算上限、停止、回滚和人工接管。

## 6. 明确不做的事情

- 不把聊天记录、prompt 或 chain-of-thought 写进 Artifact canonical snapshot；
- 不将向量数据库当作权限或业务真相；
- 不让 Agent 通过 CSS selector、模拟点击、浏览器截图来写入文档；
- 不为了 AI 预留一套未验证的 `agent_*` 数据库表、路由或 engine 分支；
- 不在基础迭代的验收中以“未来可供 AI 使用”替代当前功能和测试证据。
