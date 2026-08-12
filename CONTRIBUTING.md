# Contributing

open-office 是多 Artifact 平台。提交代码前请先阅读 `AGENTS.md`、`docs/architecture.md` 和
相关 ADR。

## 领域边界

- Document 使用 Block Tree；Spreadsheet 使用 Grid；Presentation/Whiteboard 使用 Scene Graph；
  Mindmap 使用 Graph。不要把其他 Artifact 伪装成 Document block。
- 所有持久化写入都通过 semantic command → engine → immutable snapshot；React、Canvas、SVG、
  WebGL 只负责 view/renderer，不持有第二份模型。
- 新能力要同步 Rust schema、typed protocol/TS parser、capability descriptor、测试和文档。
  在线路径不保留兼容双读/双写；旧数据只能走明确的离线迁移。

## 提交前检查

```bash
node scripts/release-check.mjs
cargo test --workspace --no-fail-fast
cd web && pnpm typecheck && pnpm test && pnpm build
```

真实 UI 变更还需使用一次性 fixture 执行 `browser:smoke`、`browser:table-smoke` 和
`browser:visual-smoke`。性能优化必须提交可重复基准和 p95 数据，不能只描述“感觉更快”。

## Pull Request 要求

PR 描述包括：领域边界、semantic commands、协议/API 影响、迁移策略、测试命令、性能预算和
已知 Unsupported。破坏性 schema/API 变更必须单独写 ADR，并在 CHANGELOG 标记。不要提交
真实用户文档、凭据、浏览器 profile、构建产物或未审计第三方素材。
