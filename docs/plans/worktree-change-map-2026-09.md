# 工作区变更分组（2026-09-10）

> 用途：为当前大规模未提交工作区建立审查和验证边界。该清单不声明既有改动归属，也不授权自动提交、丢弃或重写用户改动。

## A. Schema / Protocol / Migration

- `Cargo.toml`、`Cargo.lock`
- `crates/oo-schema/**`
- `crates/oo-protocol/**`
- `crates/oo-server/src/bin/migrate-artifact-schema.rs`
- `web/packages/schema/**`
- `web/packages/sdk/**`

汇合门禁：schema Rust/TS 边界测试、generated contract freshness、版本拒绝与 migration fixtures。

## B. Server / Projection / Presence

- `crates/oo-server/src/artifact_routes.rs`
- `crates/oo-server/src/projection.rs`
- `crates/oo-server/src/presence.rs`
- `crates/oo-server/src/*_support.rs`
- `crates/oo-server/tests/**`
- `web/apps/editor/src/api.ts`

汇合门禁：revision/idempotency/auth/history/event/asset contract tests 与 API 浏览器 smoke。

## C. Spreadsheet / XLSX

- `crates/oo-spreadsheet/**`
- `crates/oo-xlsx/**`
- `web/packages/spreadsheet-ui/**`
- `web/apps/editor/src/spreadsheet/**`
- `web/apps/editor/e2e/spreadsheet/**`
- `docs/reviews/spreadsheet-*.md`

汇合门禁：engine、formula、viewport、XLSX fixtures、clipboard/ribbon/unit/E2E。

## D. Mindmap

- `crates/oo-mindmap/**`
- `crates/oo-mindmap-wasm/**`
- `crates/oo-server/src/mindmap_support.rs`
- `web/packages/mindmap-engine/**`
- `web/apps/editor/src/mindmap/**`
- `web/apps/editor/e2e/mindmap/**`
- `docs/reviews/mindmap-*.md`

汇合门禁：engine/WASM parity、projection、layout、browser interaction。

## E. Presentation / PPTX

- `crates/oo-pptx/**`
- `web/packages/presentation-ui/**`
- `web/apps/editor/src/presentation/**`
- `web/apps/editor/src/styles/presentation.css`

汇合门禁：schema/engine/PPTX、registry/selection、create-edit-undo-reload-play E2E。

## F. Document / Typography / Visual

- `crates/oo-docx/**`
- `web/packages/document-engine/**`
- `web/apps/editor/src/chrome/BlockToolbar.tsx`
- `web/apps/editor/src/typography/**`
- `web/apps/editor/public/**`
- `web/apps/editor/e2e/document/**`
- `web/apps/editor/e2e/typography/**`
- `web/apps/editor/e2e/visual/**`
- `web/apps/editor/src/styles/print.css`

汇合门禁：Document engine/WASM adapter、font assets、visual snapshots、DOCX fixtures、打印 smoke。

## G. Whiteboard / App Entry

- `crates/oo-whiteboard/**`
- `web/apps/editor/src/whiteboard/**`
- `web/apps/editor/src/App.tsx`
- `web/apps/editor/src/Home.tsx`

汇合门禁：创建→进入 Studio→编辑→刷新、history/capability 与 scene contract。

## H. Tooling / Evidence

- `.gitignore`
- `web/package.json`、`web/pnpm-lock.yaml`
- `web/vitest.config.ts`
- `web/scripts/**`
- `docs/architecture.md`
- `docs/roadmap/**`
- `docs/plans/**`

汇合门禁：format、Rust workspace tests/clippy、generated/dependency/architecture/contract、Web typecheck/test/build、Chromium E2E。

## 合并纪律

1. 不跨组复制领域模型或 command registry。
2. Schema/Protocol 改动必须先于依赖它的产品组，并在同一审查链提供 Rust/TS contract 证据。
3. 用户既有未提交修改不自动提交；真正整理提交前必须按本表逐组确认 diff 归属。
4. P0 只新增或修改修复门禁所需文件；后续阶段在对应组内继续，避免扩大无关 diff。
