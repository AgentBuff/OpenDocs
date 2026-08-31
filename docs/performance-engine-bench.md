# 引擎性能测量（自动采集）

> 这是 `docs/performance.md` 中三个 R7 引擎预算的**实测数据**，而非文档声明。
> 测量为发布模式的引擎基准，运行方式：

```bash
# 公式依赖图：10k 节点单节点变更
cargo test -p oo-spreadsheet --release -- --ignored perf_formula --nocapture
# 稀疏表格投影：100k cell / 50×50 窗口
cargo test -p oo-spreadsheet --release -- --ignored perf_viewport --nocapture
# 白板空间查询：10k 元素视口命中
cargo test -p oo-whiteboard --release -- --ignored perf_spatial --nocapture
```

采集于 2026-08-26（macOS，Apple Silicon，release 构建；墙钟时间为单机样本，
仅用于确认数量级，不构成跨机器的一致性契约）。

## 预算对照表

| 预算 | 目标 | 实测 | 结论 |
|---|---|---|---|
| 公式依赖图 update | 10k 节点单点变更 P95 < 2ms | **0.009ms**（build 15.2ms；受影响子图 1 节点） | ✅ 远超预算，无全图退化 |
| 稀疏表格 viewport project | 100k cell 投影 P95 < 4ms | **2.1ms**（50×50 窗口 → 2500 cells） | ✅ 达标 |
| 白板空间查询 | 10k 元素视口命中不扫描全量 | **0.001ms**（examined 3 cells） | ✅ 不扫描全量，候选规模受限 |

## 测量要点

- **公式**：`FormulaDependencyIndex::update` 只替换被修改公式的边，返回的
  `FormulaDependencySubgraph` 仅含受影响拓扑（实测单节点改动的 affected_nodes=1），
  没有退化为全图 clone/diff。构建 10k 依赖图约 15ms，属一次性成本。
- **表格**：`SparseGridViewport::project` 只返回窗口内 `viewport.contains` 的已物化 cell，
  半开区间隔离空区域；50×50 窗口稳定投影 2500 cells，不随全 sheet 规模增长。
- **白板**：`WhiteboardSpatialIndex::query_with_stats` 返回 `candidate_count` 与
  `cells_examined`，均被空间网格限定（10k 元素下 examined ≤ 3），视口命中不扫描全图。

## 回归锁定

以上三个预算各自配了一个 `#[ignore]` 的 Rust 测量测试，显式跑 `--release -- --ignored perf_`
即可复测并输出真实 p50 量级数据。这些测试有宽松的墙钟上限断言（例如 viewport `< 10ms`、
formula `< 40ms`、spatial `< 10ms`），用于在人为引入 O(N) 退化时给出信号，但因其依赖墙钟，
**不进入常规 `cargo test` 门禁**（避免 CI 机器抖动导致的假失败）。算法级不变量
（候选数/受影响子图规模/us 零全量扫描）由常规确定性测试锁定。
