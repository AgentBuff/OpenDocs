# 性能设计与验证

性能不靠“Canvas 一定快”或“DOM 一定慢”推断，而靠可重复的基准和真实交互 P95 数据。

## 统一原则

1. 权威文档只更新一次，layout、render、overlay 都消费同一个 immutable/版本化快照。
2. 编辑事务只携带实际修改的 operation，不在每次按键时传输整份文档。
3. layout 采用 block/container 级失效：修改文本只重排当前 block，修改列宽才使列容器和分页失效。
4. renderer 只绘制视口内对象；滚动和缩放只改变 camera，不重新计算业务模型。
5. 任何优化必须有 trace 标记和基准样本，避免凭单次开发机体感判断。

## 文档

- BlockStore 维护 `BlockId -> Block` 索引，避免每次通过 `Vec` 线性查找。
- `LayoutKey` 至少包含 block content revision、style revision、container constraint 和 font revision。
- 分页缓存分为 block layout、container layout、page placement 三层；文本变更不应重算无关 block。
- DOM editor 只挂载可见 block 和当前编辑 block；未来的 Canvas preview 对页面做视口裁剪。
- 文字度量按字体和文本片段缓存，字体加载完成只使相关字体 revision 失效。

## 表格

- cells 采用稀疏存储，空白区域不创建对象。
- 单元格公式建立依赖图，修改一个 cell 只重新计算受影响的拓扑子图。
- Grid renderer 只创建可见行列，编辑单元格使用单个 DOM overlay。
- 大范围粘贴、填充和排序必须合并成一个 transaction，避免逐格通知。
- `SparseGridViewport::project` 只返回窗口内已物化 cell；窗口协议使用半开区间，避免为十万行
  空白区域分配 DOM 或 JSON 对象。R7 基准目标：10 万稀疏 cell、200×50 viewport 的投影 P95 < 4ms。
- FormulaDependencyIndex 的 `update` 只替换被修改公式的边并返回受影响拓扑；R7 基准目标：1 万节点依赖图
  单节点变更 P95 < 2ms，不能退化为全图 clone/diff。
- XLSX round-trip 基准必须比较 canonical model，而不是 XML 字节；支持的 shared strings、number format、
  freeze/merge/worksheet metadata 必须等价，不支持的 fonts/fills/borders/media 等进入 structured loss report。

## 幻灯片、思维导图、白板

- slide/layout 以 slide 或 frame 为缓存边界。
- mindmap 只在树结构或主题变化时触发布局；拖动节点只更新局部连接线。
- mindmap edge routing 只消费 layout projection，生成稳定的局部 connector geometry，不把边
  坐标写回 graph。
- whiteboard 维护 quadtree/R-tree 空间索引，命中测试和视口渲染不扫描所有元素。
- 当前 uniform-grid index 只保存 element bounds/id，viewport query 先按网格筛候选再做精确
  相交测试；查询统计锁定候选规模相对全量元素的边界。
- whiteboard 命令在 live SceneGraph 上执行，只记录 touched elements 的 typed inverse；失败批次
  按逆序回滚；schema 校验走 borrowed `WhiteboardModel::validate`，不为每次拖动克隆整张
  SceneGraph。未触及 element 的引用稳定性由回归测试锁定。
- Canvas/WebGL 使用 layer、dirty rectangle、实例化绘制和视口裁剪；复杂文本通过 DOM overlay 编辑。

## 主线程与渲染边界

- 所有高频交互合并到一个 `requestAnimationFrame` 批次。
- React 不保存第二份 Paragraph 文档；Block session 只维护经过校验的 snapshot 和待提交
  operation。
- DOM 负责输入法、可访问性和文本编辑；图形 renderer 只接收模型与几何结果。
- 当 profiler 证明 JSON 成为瓶颈后，再增加二进制协议；不能在没有数据前维护两套协议。

## 必须建立的基准

| 基准 | 样本 | 目标 |
|---|---|---|
| text insert | 100 页、CJK/Latin 混排 | P95 < 16ms |
| block insert/move | 1,000 blocks、5 层嵌套 | P95 < 16ms |
| scroll | 100 页文档 | 主线程无持续长任务 |
| sheet recalc | 100,000 稀疏 cells | 只计算依赖闭包 |
| whiteboard hit-test | 10,000 elements | 视口命中不扫描全量 |
| collaboration update | 1,000 operations | 更新合并、回放可重复 |

每项基准都应记录 p50、p95、p99、内存峰值和布局缓存命中率。
