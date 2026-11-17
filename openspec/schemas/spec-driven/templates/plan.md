<!-- 计划仅在变更进入实施准备时生成，原地维护当前有效状态（不堆叠历史横幅或微测日记）。遵循 openspec/config.yaml。 -->

## Execution Model

- 执行：主执行者统筹，`coder` 实现并完成子包自测闭环，集成后由独立 `tester` 验收、独立 `reviewer` 检视。
- 隔离：`tester`/`reviewer` 不继承 `coder` 开发对话；仅在真正并行写入时隔离工作区与 `CARGO_TARGET_DIR`。
- 门禁：只设两道独立门禁——① 编码前（`npm run check:spec` 通过后一次性合审方案与用例）；② 集成交付（本地分支固定单一 Git `commit SHA` 后并行验收与检视）。

## Parallel Assessment

<!-- 列出可并行的工作包；串行部分说明依赖或写范围重叠原因。 -->

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- WP1 --> | <!-- 需求与可观察结果 --> | <!-- none / WPn --> | <!-- 执行角色 --> | <!-- 真实相对路径 --> | <!-- 提交与实现说明 --> | <!-- 定向或集成检查命令 --> |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| <!-- 1 --> | <!-- WP1 --> | <!-- 依赖或写范围重叠 --> | <!-- 合入后需跑的检查 --> |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| <!-- PV1 --> | <!-- 引用 tests.md 用例 ID、命令与目录 --> | <!-- 真实通过条件 --> |

## Code Review

<!-- 两道独立门禁：① 编码前合审方案与用例；② 集成 commit SHA 交付检视。机械结构交由 npm run check:spec 校验。 -->

| Gate / Scope | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| <!-- 编码前门禁 --> | <!-- 独立 reviewer --> | <!-- 需求依据、协议安全边界、测试充分性 --> | <!-- npm run check:spec 通过且无阻断问题 --> |
| <!-- 集成交付门禁 --> | <!-- 独立 reviewer --> | <!-- 集成 commit diff、协议语义、资源清理与证据 --> | <!-- 全部阻断问题在新 commit 复查闭环 --> |

## Main E2E

<!-- 二选一：
1) 需要端到端验收时，代码块内只写 `mode: required`（不得保留 reason / basis / alternative_checks 键），并在代码块下方写明真实入口、数据隔离与清理验证。
2) 不适用端到端验收时，使用下方 `mode: not-applicable`，reason 只能取 pure-refactor | docs-only | no-user-surface | isolated-internal-module 之一，basis 为非空字符串，alternative_checks 为非空字符串列表。 -->

```yaml
mode: required
```

## Risks

<!-- 关键技术风险、环境限制与缓解措施。 -->

## Handoff

<!-- 交接本地工作分支受测 Git commit SHA（git status 干净）、规则与产物路径、读写范围与检查入口；不传开发对话。 -->

## Findings

| ID | Source | Severity | Commit SHA | Finding | Fix Commit | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |

<!-- 问题由独立提出者针对修复 commit 增量复核关闭，coder 不得代关。 -->

## Verification Results

<!-- 仅维护当前有效集成版本的精简结果摘要；详细原始日志保存在本地被忽略的 testing/runs/<change>/ 下。门禁通过后仅回填本节、勾选 tasks.md 及同步归档的纯文档提交直接继承受测 commit SHA 结论。 -->

- 受测版本：<!-- 本地工作分支 Git commit SHA（受跟踪文件工作区干净） -->
- 执行者与环境：<!-- tester / reviewer、OS 及关键工具版本 -->
- 验证命令及退出码：<!-- 实际执行的检查入口与结果 -->

| Case ID | Executor | Commit SHA | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| <!-- 用例ID --> | <!-- 执行者 --> | <!-- commit SHA --> | <!-- passed / failed / not-run --> | <!-- 核心断言结论或受阻原因 --> |

- 未执行项与已知限制：<!-- 明确 none 或如实列出 not-run 原因 -->
- 问题关闭依据：<!-- 稳定 ID、修复 commit 与独立复验者；无问题写 none -->
