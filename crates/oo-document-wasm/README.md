# oo-document-wasm

`oo-document-wasm` 是 `oo-document::DocumentEngine` 的浏览器薄绑定。它不实现 Document
业务、不维护第二份模型，也不接受旧 Paragraph/Canvas 会话格式。

## API

生成的 JS 模块暴露：

```ts
const session = loadSnapshot(snapshotJson);
const changeSetJson = session.dispatch(transactionJson);
const blockJson = session.readBlock(blockId);
const latestChangeSetJson = session.readChangeSet();
const snapshotJson = session.readSnapshot(); // 显式完整快照读取
```

`dispatch` 只返回增量 `ChangeSet`；完整 snapshot 仅用于加载、保存、导出和调试。前端
适配层位于 `web/packages/document-engine`，不会把 Rust engine 的领域逻辑复制到 React。

适配层的加载入口是异步且可注入的：

```ts
import {
  createDocumentEngine,
  loadWasmDocumentEngine,
} from "@open-office/document-engine";

const engine = await createDocumentEngine(loadWasmDocumentEngine);
const session = engine.loadSnapshot(snapshot);
```

测试和 SSR 可以注入同样接口的 fake binding，不需要把二进制静态引入编辑器 bundle。

## 构建

在仓库根目录执行：

```bash
wasm-pack build crates/oo-document-wasm \
  --target web \
  --out-dir ../../web/packages/document-engine/wasm \
  --no-pack
```

同一命令已通过 `cd web && pnpm build:document-engine-wasm` 提供。只改 TypeScript 适配器
时不需要重新生成 WASM；改动 `oo-document` 或本 crate 后再构建即可。
