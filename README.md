# OpenDocs

OpenDocs is a multi-artifact online productivity platform for documents, spreadsheets,
presentations, mind maps, and whiteboards.

> **Project status**: This is a pure AI Vibe Coding project. It is still at an early stage of
> development and is not ready for production or everyday use. Do not use it for important data,
> critical workflows, or any scenario that requires reliability guarantees.

Its core principle is that strict domain models, semantic transactions, and immutable snapshots
are the single source of truth. DOM, Canvas, SVG, and WebGL are renderers and interaction layers
only.

## License

This project is licensed under the [Apache License 2.0](LICENSE).

## Local development

```bash
cargo run -p oo-server
cd web && pnpm --filter @open-office/editor dev -- --host 127.0.0.1 --port 5174
```

- Frontend: `http://127.0.0.1:5174`
- Backend: `http://127.0.0.1:8787`

All configuration is read from the environment; every variable and its default is listed in
[`.env.example`](.env.example). To exercise collaboration roles (owner / editor / viewer),
presence and audit trails before real authentication exists, start the server with
`OO_TRUST_USER_HEADER=1` and send `X-OO-User: <id>`. That header is untrusted by default and a
request carrying it is rejected, so the switch has to be explicit.

See [AGENTS.md](AGENTS.md) and [CONTRIBUTING.md](CONTRIBUTING.md) for architecture and
contribution conventions.

中文版本：[README.zh-CN.md](README.zh-CN.md)
