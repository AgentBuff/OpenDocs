# Changelog

所有对外可见的破坏性变更和重要性能/安全修复都记录在这里。版本尚未正式发布，当前条目
描述主干阶段性能力。

## Unreleased

### Architecture and release gates

- 增加 R10-C 发布清单、Chromium 9222/CDP 冒烟、视觉截图 manifest、故障注入和依赖审计入口。
- 明确多 Artifact engine 边界、性能预算、Unsupported 能力可见性和未实现认证/鉴权的部署要求。
- Presentation 首批补充文本/样式、group、notes、animation、theme 的 semantic command 与
  PPTX loss report（详见对应 ADR/阶段记录）。

### Breaking changes

- `/api/docs/**`、`/content`、`/operations` 和旧 generic block patch 不属于在线协议；迁移请使用
  canonical `/api/artifacts` 和版本化 snapshot/transactions。
