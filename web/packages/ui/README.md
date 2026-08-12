# `@open-office/ui`

Open Office 的基础 UI 设施。目录按职责拆分，公共消费者只从 `@open-office/ui` 导入，不能依赖
`src` 内部路径。这个边界吸收 Arco Design 的 Token/主题组织和 Ant Design 的受控组件、Provider、
Overlay 与行为抽象，但不引入第三方组件库运行时。

## 目录约定

```text
src/
├── foundation/   # 主题、密度和配置上下文
├── primitives/   # Button、Surface 等最小视觉原语
├── controls/     # Input、Select、Checkbox、Switch 等表单控件
├── navigation/   # Menu、Toolbar 等导航/工具栏视觉原语
├── overlay/      # Portal、dismiss、focus、position 和浮层组件
├── feedback/     # Spinner、Badge、Empty、Divider
├── icons/        # IconName、Registry、内置图标和 Icon 组件
├── styles/       # Token、组件和浮层样式入口
└── index.ts      # 唯一公共 API
```

## 组件边界

- 基础组件只依赖 React、DOM 和 `--oo-*` Token，不依赖 Artifact、Document、schema 或 editor session。
- 受控组件优先使用 `value/defaultValue/onChange` 或 `open/defaultOpen/onOpenChange` 契约。
- 所有浮层复用 `overlay/` 的 Portal、dismiss、focus 和 positioning；业务组件不得自行注册全局监听器。
- 图标通过 `IconRegistry` 访问，Toolbar descriptor 不携带 React 节点或 SVG 字符串。
- Toolbar 控件统一使用 `ToolbarButton`、`ToolbarSelect`、`ToolbarField`、`ToolbarSeparator` 和
  `ToolbarSplitGroup`；split group 只负责主按钮与独立下拉按钮的几何/ARIA 分组，不吞并下拉行为；
  `ToolbarSelect` 只接受显式 `options` 并渲染自有 combobox/listbox；editor 不得恢复
  `.tbtn`、`.tselect` 或直接堆叠原生 toolbar 控件样式。
- 每个新组件先确定归属、状态矩阵和键盘/ARIA 语义，再添加实现和测试。

不要为单个按钮或菜单创建独立 npm 包；只有依赖方向、发布周期或运行时边界真正独立时才新增 package。
