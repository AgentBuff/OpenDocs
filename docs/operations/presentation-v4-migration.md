# Presentation v4 → v5 离线迁移演练

> 此工具只为破坏式 v5 cutover 准备候选 Deck。它不写入在线 Artifact snapshot，运行中的
> HTTP 服务、WASM、浏览器与 SDK 都不会读取其产物。这样在 P1-C 之前不会引入 v4/v5 双读。

## 迁移边界

输入必须是 schema v4 的 `kind: presentation` Artifact JSON。迁移器先完整校验旧 scene graph，
再生成严格的 v5 Deck 和机器可读的 loss report：

- `shape`、`text`、`group` 映射为对应的 typed node；
- 仅认识的形状几何（矩形、椭圆、线、箭头）才映射；无法表示的 `shapeKind` 降为保留原始数据的
  `extension`；
- 未知 `typeId` 完整保留为 `legacy.presentation` extension；
- v4 不具备 page spec、layout/master、完整 theme、animation trigger 等信息时，默认值或丢失都会
  写入 report，绝不静默吞掉；
- 目标 Deck 在写文件前再次严格校验；任何输入失败都会使整批演练不激活。

## 演练与激活

```bash
# 只读取 blobs，报告候选数和 loss；不写任何文件。
cargo run -p oo-server --bin migrate-presentation-v4 -- ./data

# 写入独立的 v5 staging run，并原子更新 staging 的 current 指针。
# 运行 API 必须停机；此命令不会替换 data/blobs 中仍供 v4 服务读取的 snapshot。
cargo run -p oo-server --bin migrate-presentation-v4 -- ./data --apply

# 只演练一个 artifact。
cargo run -p oo-server --bin migrate-presentation-v4 -- ./data --artifact-id <id> --apply
```

`presentation-v5-stage/runs/<run-id>/` 是不可变的候选输出，包含每个 Deck、每个 report 以及
manifest。所有文件先写入 `.staging/<run-id>`，完成并 fsync 后再将目录改名为 run；最后通过同目录
`rename` 原子替换 `current.json`。替换前旧 `current.json` 会复制到 `backups/`。因此失败时旧活跃
run 保持可读，不会看到半份新 run。

## P1-C 的唯一接入方式

P1-C 将读取由 `current.json` 指向的候选 run，生成并验证真正的 schema v5 Artifact envelope，先备份
原 v4 snapshot，再以存储层原子写替换。该步骤只能与 schema、engine、PPTX adapter、server 与浏览器
parser 的 v5 切换一起执行；不要把 staging 文件直接放进 `data/blobs` 或让线上解析器接受它。
