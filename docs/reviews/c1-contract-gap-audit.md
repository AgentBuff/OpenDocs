# C1 平台契约收口——差距审计（2026-08）

> 依据 [`docs/roadmap/core-platform-plan.md`](../roadmap/core-platform-plan.md) §C1 逐项核对
> 服务端与协议代码；修复动作见文末"本次收口"。完成门槛：公开路由仅 `/api/artifacts`；
> 读投影有界分页；写入可安全重试；客户端无需解析 DOM/全量快照即可局部读、语义写。

## 已达标项（逐条证据）

| 计划要求 | 现状 | 证据 |
| --- | --- | --- |
| 能力发现 | `/api/capabilities` 返回版本化 catalog(protocol v1 / contract v1 / projection v2) | `oo-protocol/src/lib.rs`;`api.rs::capability_catalog_locks_the_engine_command_surface` |
| outline/block/entity projection + cursor 分页 | blocks limit∈[1,1000]、maxBytes∈[256B,4MB];presentation slides 同界;cursor 绑定 revision | `oo-server/src/projection.rs:32-60,510-628` |
| 稳定错误 envelope | `{error,code,requestId,details?}` + `x-request-id` 头;内部错误不外泄;conflict 携带机器可读 details | `oo-server/src/error.rs`(含 6 个单测) |
| If-Match/baseRevision/transaction id 与重复提交统一 | 幂等重放返回原结果且不产生新事件;stale revision → 409 | `tests/api.rs::transactions_are_idempotent_and_reject_stale_revisions`;history 重试幂等 |
| artifact event feed | `GET …/events?sinceRevision=&cursor=&limit=`;cursor/sinceRevision 互斥;limit∈[1,1000];outbox at-least-once + lease(0006) | `oo-server/src/events.rs`;migrations 0004/0006 |
| 历史查询 | `GET …/history` undo/redo affordances;server-authoritative history 命令(document/presentation.history) | `artifact_routes.rs:927`;`routes.rs` |
| 资产引用与 GC | blob 完整性表(0009)+ `remove_unreferenced` 对账删除 + `reconcile-snapshots` bin | `store.rs:78`;`db.rs:432` |
| 通用 Principal seam | `CurrentUser` 单点提取(换 JWT 只改 `authenticate`);事务落库绑定 `author_id=user.id`,含 `commands_json` 审计摘要 | `auth.rs`;migrations 0008 `artifact_transactions.author_id`;`routes.rs:856` |
| 客户端无需 DOM/全量快照 | outline/blocks/history/events/capabilities 全部为 REST JSON 投影 | 同上 |

## 本次收口（2026-08）

1. **服务端 catalog 漂移锁**：新增集成测试断言 HTTP 返回的 document(33)/presentation(47)
   命令集合与引擎面精确一致，spreadsheet/mindmap/whiteboard 必须保持空命令(不虚报)。
   此前 protocol 层 contract test 只锁定协议结构，服务端手写清单无任何防护。
2. **事件流 actor 归属**：所有 DomainEvent payload(document.block*/artifact.created/
   *historyApplied/presentation.* )统一注入 `actorId=认证用户`。外部消费者此前只能拿
   transaction_id 反查。payload 为自由 JSON,增量字段向后兼容协议 v1。
   测试:`api.rs::domain_events_carry_the_authenticated_actor`。

## 未关闭项（记录在案，属后续工作）

1. **OpenAPI/JSON Schema 生成的单源方向**：第一阶段已于 2026-08-26 落地——
   [ADR-0010](../adr/0010-contract-single-source-generation.md) 采纳 schemars 派生路线，
   `oo_protocol::generate_contract_schemas()` 输出 13 个协议类型的 JSON Schema,
   golden 快照(`tests/snapshots/contract_schemas.json`)锁定形状。剩余为第二/三阶段接线：
   `generate-api-contract.mjs` 的 components 改为消费生成物、`@open-office/schema` 的
   协议 DTO 改为代码生成。
2. **权限模型**：现仅 owner 级(`owned_meta`);workspace/member/role 属 C4 范围，但
   Principal seam 与 actor 贯穿已就绪，C4 只需替换授权判定与扩展角色表。
3. **事件 outbox worker**:投递接口已有(attempts/lease),外部消息系统接入留待部署层。

## 结论

除"生成式契约单源"需 ADR 外,C1 的其余验收口径已具备代码与测试证据;完成门槛四句话中
前三句成立，第四句(局部读+语义写)由 capabilities/projection/transactions 三面共同满足。
