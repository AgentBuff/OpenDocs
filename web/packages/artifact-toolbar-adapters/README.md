# `@open-office/artifact-toolbar-adapters`

Spreadsheet、Presentation、Mindmap、Whiteboard 的 toolbar adapter 契约层。

这个包只负责把 Artifact capability registry 提供的 descriptor 转成
`@open-office/toolbar-core` descriptor，并按 `availableCapabilities` 解析可见性。它不导入
Document engine、任何 Artifact engine、React 或持久化模型，因此不会形成第二套领域状态。

## 分层

```text
Artifact command/capability registry
            ↓ (只提供真实 capability key + semantic action)
artifact-toolbar-adapters
            ↓ (core descriptor + resolved state)
@open-office/toolbar-react → @open-office/ui Token / Overlay / Icon Registry
```

每个 Artifact 使用独立命名空间：`spreadsheet.*`、`presentation.*`、`mindmap.*`、
`whiteboard.*`。适配器默认没有任何 descriptor；未被真实 registry 宣告的能力不会渲染。
这样可以先建立稳定的 UI 合约，再由各自 engine/command 通过单独 package 接入，不把未来能力伪装成
已实现按钮。

## 用法

```ts
const adapter = createSpreadsheetToolbarAdapter(spreadsheetCapabilities);
const items = adapter.resolve({
  artifactId,
  revision,
  availableCapabilities: commandRegistry.keys(),
  selection,
});
```

React 产品壳将 `adapter.toolbar` 和同一个 context 交给 `ToolbarRenderer`，事件仍由 Artifact
command dispatcher 处理。adapter 不执行命令、不持有模型、不负责 Overlay 生命周期；这些职责分别属于
Artifact runtime、`toolbar-react` 和 `@open-office/ui`。

### Context 设计

- Spreadsheet：cell/range/sheet selection；模型、公式依赖图和 Grid engine 不进入 context。
- Presentation：slide/element selection；Scene Graph 和 layout projection 留在 Presentation runtime。
- Mindmap：canvas/node selection；Graph layout 和 edge routing 留在 Mindmap runtime。
- Whiteboard：canvas/element selection；Scene Graph、camera 和 spatial index 留在 Whiteboard runtime。

新增能力必须先有真实 command、schema、持久化和 registry key；只添加 UI descriptor 不会让能力出现。
