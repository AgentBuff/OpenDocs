# Security policy

## 报告漏洞

请不要在公开 issue 粘贴凭据、用户文档或可复现攻击数据。生产漏洞请通过仓库配置的私密
security advisory / 维护者私下渠道提交，并提供影响范围、复现步骤、版本和修复建议。
维护者会确认收到报告、评估严重性并在修复后发布公告。

## 安全边界

- 当前版本未实现认证和授权；部署必须放在可信网络和反向代理之后，不能把本地服务直接暴露
  到公网。Principal/ACL 是后续独立里程碑，不要在业务代码中伪造安全判断。
- API 使用 `If-Match` revision 和 `x-transaction-id` 幂等键；服务端必须校验 artifact kind、
  schema、大小限制和事务边界。
- 导入 DOCX/XLSX/PPTX 时限制 ZIP/XML 解压大小、路径穿越和外部关系；未知能力必须进入结构化
  Unsupported/loss report，禁止静默执行宏或外链。
- 上传资源使用 checksum、原子写入和孤儿回收；日志中不得输出访问令牌、原文内容或个人信息。

## 依赖与披露

运行 `node scripts/dependency-audit.mjs` 生成依赖清单，并在发布前执行高危漏洞与许可证扫描。
发现依赖漏洞时优先升级或隔离；无法立即修复时在 CHANGELOG 和安全公告中记录缓解措施。
