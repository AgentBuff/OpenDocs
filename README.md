# OpenDocs

OpenDocs 是一个多 Artifact 在线办公平台，统一支持文档、表格、演示文稿、思维导图与白板。

核心原则是以严格的领域模型、语义事务与不可变快照作为唯一真相；DOM、Canvas、SVG 和 WebGL
仅承担渲染与交互职责。

## 本地开发

```bash
cargo run -p oo-server
cd web && pnpm --filter @open-office/editor dev -- --host 127.0.0.1 --port 5174
```

- 前端：`http://127.0.0.1:5174`
- 后端：`http://127.0.0.1:8787`

详细架构和贡献约定见 [AGENTS.md](AGENTS.md) 与 [CONTRIBUTING.md](CONTRIBUTING.md)。
