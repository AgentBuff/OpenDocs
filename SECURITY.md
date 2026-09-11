# Security policy

## 报告漏洞

请不要在公开 issue 粘贴凭据、用户文档或可复现攻击数据。生产漏洞请通过仓库配置的私密
security advisory / 维护者私下渠道提交，并提供影响范围、复现步骤、版本和修复建议。
维护者会确认收到报告、评估严重性并在修复后发布公告。

## 安全边界

- 当前版本未实现认证。授权（owner / editor / viewer 的角色矩阵）已实现，但它的信任锚点
  是请求所声明的身份，所以**部署必须放在可信网络和反向代理之后**，不能把本地服务直接
  暴露到公网。
- 身份声明头 `X-OO-User` **默认不被信任**。只有显式设置 `OO_TRUST_USER_HEADER=1` 时才
  会决定 Principal；未设置时携带该头的请求会被 400 拒绝，而不是静默降级成开发用户——
  静默降级会让 ACL 演练得出错误结论。该开关仅用于本地演练协作权限，服务启动时会打 warn。
  接入真实认证（JWT 校验）时只需替换 `crates/oo-server/src/auth.rs` 的 `authenticate`
  一处，处理器不用改。
- 不要在业务代码中伪造安全判断。所有取用户身份的路径都必须走 `auth::authenticate`。
- API 使用 `If-Match` revision 和 `x-transaction-id` 幂等键；服务端必须校验 artifact kind、
  schema、大小限制和事务边界。
- 导入 DOCX/XLSX/PPTX 时限制 ZIP/XML 解压大小、路径穿越和外部关系；未知能力必须进入结构化
  Unsupported/loss report，禁止静默执行宏或外链。
- 上传资源使用 checksum、原子写入和孤儿回收；日志中不得输出访问令牌、原文内容或个人信息。

## 依赖与披露

运行 `node scripts/dependency-audit.mjs` 执行依赖清单、许可证复核与漏洞扫描。该脚本覆盖**完整
的第三方依赖图**（Cargo.lock 解析出的全部 crate，以及 pnpm-lock.yaml），而不是仅 workspace
成员：

- 许可证：workspace crate 与第三方 crate 都必须声明 license；不在允许列表内的许可证会列出
  供人工复核。
- Rust 漏洞：委托 `cargo audit`（RustSec）。本地需先 `cargo install cargo-audit`。
- JS 漏洞：委托 `pnpm audit`。生产依赖的漏洞会阻断；仅 devDependencies 可达的漏洞只作为警告
  列出，不阻断构建。

CI 以 `OO_REQUIRE_DEPENDENCY_SCANNERS=1` 运行该脚本，此时缺少任一扫描器即视为失败。发现依赖
漏洞时优先升级或隔离；无法立即修复时在 CHANGELOG 和安全公告中记录缓解措施。
