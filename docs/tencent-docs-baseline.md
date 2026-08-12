# 腾讯文档基础交互对照基线

本文记录通过 CDP 观察到的交互规律，用来约束 open-office 的 Block Tree DOM 实现，不是
功能复制清单。界面只暴露已经有真实 operation 和持久化路径的能力。

## 观察到的结构约束

| 区域 | 腾讯文档观察值 | open-office 约束 |
| --- | --- | --- |
| 顶部工具栏 | 单层功能带，控件命中区稳定；没有卡片阴影、横向滚动条或多余底色 | 工具栏固定在工作台顶部，主编辑区独立滚动，禁止浮层遮挡正文 |
| 主体工作台 | 页面/内容区域与 toolbar 之间有明确留白，左右不出现额外滚动条 | `.editor__surface` 负责背景与滚动，`.block-editor__stage` 负责页面留白 |
| block 行前操作 | 每个 block 的加号和把手固定在正文左侧，不随文字右对齐移动 | gutter 与内容列分离，悬浮按钮定位到 block 行盒左侧 |
| block 菜单 | 菜单从 gutter 向左展开，命中区大于图标本身，格式与插入能力分组 | 使用独立 DOM 浮层，不把菜单插入 contentEditable，不改变正文布局 |
| 空行交互 | 空行左侧可直接唤起插入菜单，插入菜单包含图片、形状、表格、链接、代码等 block | 先落地 block operation，再按能力注册菜单项；未实现能力不显示假按钮 |

## 当前实现

| 类别 | 当前实现 | 验证位置 |
| --- | --- | --- |
| Block 输入 | DOM `contentEditable`、回车新增 block、空 block 退格删除、基础 IME | `web/apps/editor/src/blocks/BlockNode.tsx` |
| Block 类型 | 段落、标题、引用、代码、分割线；稳定 id 与 root/children 校验 | `oo-schema`、`oo-document` |
| 行前操作 | gutter 加号/把手固定在内容列左侧，不随对齐方式移动 | `blocks/BlockNode.tsx`、`styles/blocks.css` |
| 格式工具 | undo/redo、block 类型、粗体/斜体/下划线/删除线 | `Editor.tsx`、Document transaction API |
| 持久化 | Artifact snapshot、revision、transactionId 幂等、冲突重载 | `oo-server` API 集成测试 |

## 后续能力进入条件

1. 图片、表格、链接、代码、外链卡片等能力先定义 `DocumentBlockKind`、attrs 和 operation；
2. engine 单测覆盖原子性、删除子树、撤销/重做和 revision 冲突；
3. DOM renderer 只渲染 block，不在组件内维护第二份文档状态；
4. 服务端只提交 `TransactionEnvelope`，不新增旧式单独写接口；
5. 菜单和 toolbar 只显示已经可执行的能力。

## 回归原则

- gutter、内容和浮层使用同一 block 行坐标系；
- 悬浮命中区不能覆盖正文，右对齐只影响内容列；
- 页面和 toolbar 间留白属于布局层，不用阴影或额外背景制造层次；
- 用 CDP 验证真实点击、输入、保存和刷新，不以静态截图代替交互回归。
