# Document editor iteration specifications

This directory turns architecture decisions into execution-ready work packages.
Each iteration is independently reviewable and must contain:

- `scope.md`: intent, non-goals, dependencies and public invariants;
- `design.md`: state model, transitions and ownership boundaries;
- `commands.md`: semantic command/API impact, including explicit non-changes;
- `tasks.md`: ordered file-level tasks with ownership and dependencies;
- `acceptance.md`: unit, browser, visual and performance evidence required to close it.

## Completion rule

A checkbox in `tasks.md` is not completion. An iteration is complete only when every acceptance item has reproducible evidence, the architecture boundary check passes, and removed compatibility/event paths are deleted rather than left live.

## 当前实施顺序

1. `I00-quality-baseline` establishes the evidence and test harness.
2. `I01-interaction-kernel` creates the common selection, overlay and object behavior runtime.
3. Table and image work may begin only after I01 exposes the relevant extension points. Their feature work is tracked by later iterations.

`I00` 与 `I01` 是当前必须先关闭的基础实施项，不能因新功能而跳过验收。其后的基础平台、专业
Document 和多 Artifact 工作按 [`docs/roadmap/core-platform-plan.md`](../roadmap/core-platform-plan.md) 推进。

AI、知识图、Agent runtime、MCP/Skill 适配与组织记忆不属于这些迭代的当前范围；它们被明确延期到
[`docs/roadmap/ai-native-future-plan.md`](../roadmap/ai-native-future-plan.md)，并以基础平台的完成门槛为前置。
