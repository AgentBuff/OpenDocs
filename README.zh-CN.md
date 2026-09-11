# OpenDocs

OpenDocs 是一个多 Artifact 在线办公平台，统一支持文档、表格、演示文稿、思维导图与白板。

> **项目状态声明**：本项目是一个纯 AI Vibe Coding 项目，目前仍处于早期开发阶段，
> 不具备生产或日常使用的可用性。请勿将其用于重要数据、关键业务或任何需要可靠性保证的场景。

核心原则是以严格的领域模型、语义事务与不可变快照作为唯一真相；DOM、Canvas、SVG 和 WebGL
仅承担渲染与交互职责。

## 开源协议

本项目采用 [Apache License 2.0](LICENSE) 开源。

## 本地开发

```bash
cargo run -p oo-server
cd web && pnpm --filter @open-office/editor dev -- --host 127.0.0.1 --port 5174
```

- 前端：`http://127.0.0.1:5174`
- 后端：`http://127.0.0.1:8787`

全部配置都从环境变量读取，每一项及其默认值见 [`.env.example`](.env.example)。在真实认证落地
之前要演练协作角色（owner / editor / viewer）、presence 与审计，需要以 `OO_TRUST_USER_HEADER=1`
启动服务并发送 `X-OO-User: <id>`。该头**默认不被信任**，携带它的请求会被拒绝，所以这个开关
必须是显式的。

详细架构和贡献约定见 [AGENTS.md](AGENTS.md) 与 [CONTRIBUTING.md](CONTRIBUTING.md)。

English version: [README.md](README.md)
