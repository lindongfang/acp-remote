<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。

     本仓库的执行原语是 Oh My Pi（omp），不是外部调度器：
     - 工作包由主 Agent 用 `task` 工具批量派发（batch 形状：共享 `context` + `tasks[]`）；
     - 每个工作包跑在隔离工作区（`task.isolation.enabled: true`、`merge: patch`、`apply: false`）；
     - 交付物是 `<id>.patch`（改动）+ `agent://<id>`（报告）+ `history://<id>`（过程），
       不是共享目录，也不是 branch；
     - 合入由主 Agent 串行执行（`git apply --check` → `git apply` → 主检出 Project Verify → commit）。

     填写时机：生成 tasks 之前定完 WP/写范围/依赖/合入顺序/验证策略；用例 ID 与执行分配在 apply 阶段补齐。
     表单里的每一列都必须可核对，不能写“按需”“视情况”这类无法验证的描述。 -->

## Execution Model

<!-- 复述本仓库固定的执行原语，供 reviewer 与 apply 阶段核对。实现方式变化时同步更新本节与 docs/WORKFLOW.md。 -->

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

<!-- 一个工作包 = 一次解耦、可独立验证的交付。不得让两个工作包写同一处代码。
     Write Scope 用仓库相对路径前缀；重叠的写范围必须登记到 ## Merge Order 的串行序列。
     Agent 填 task 工具的 `agent` 名（如 wp-coder / wp-tester / reviewer）。
     Dependencies 只写真实的 code / contract 依赖；没有依赖写 none。 -->

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- WP1 --> | <!-- 需求/scenario 引用 --> | <!-- none / code:WPn / contract:WPn --> | <!-- wp-coder --> | <!-- 文件或目录前缀，逗号分隔 --> | <!-- patch + 报告要点 --> | <!-- 主检出执行的命令/检查 --> |

## Merge Order

<!-- 主 Agent 的串行合入序列。写范围重叠的工作包必须在此排定先后；
     后合入者需重跑其 Verify（因为前一个 patch 已经改变了工作区）。 -->

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| <!-- 1 --> | <!-- WP1 --> | <!-- 无重叠 / 与 WPn 写范围重叠 --> | <!-- 合入后需重跑的命令 --> |

## Project Verify

<!-- 只在主检出执行。隔离工作区跑不了这些命令（没有 target/ 与 node_modules/）。 -->

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| <!-- PV1 --> | <!-- 完整命令 --> | <!-- 通过条件 --> |

## Code Review

<!-- 每个工作包一行；Reviewer 用内置 reviewer agent（不自建），不能与作者是同一个 spawn。
     检视契约在派发时用 per-call outputSchema 覆盖（verdict: pass|changes-requested|blocked + findings）。
     必须把 patch 的**绝对路径**写进 task 文本：内置 reviewer 默认读 git diff / gh pr diff，
     而 patch 在会话产物目录里，不在仓库也不由 git 命令产出。
     verdict 决定下一步：pass → 合入队列；changes-requested → 派发新 spawn 修复（prompt 必须自带
     工作包目标、写范围、上一版 patch、findings 原文）；blocked → 补证据后重新检视。
     修复与复核各用新 spawn（隔离 spawn 结束后无法再被唤起）；同一工作包最多修 3 轮，
     超过 3 轮说明工作包拆得不对，回到本文件重新拆分。合入只应用最终通过的那一版。 -->

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| <!-- WP1 --> | <!-- reviewer --> | <!-- 契约符合性/边界/错误处理/回归风险 --> | <!-- 无 CRITICAL/MAJOR 未闭环项 --> |

## Main E2E

```yaml
mode: not-applicable # required | not-applicable
reason: "" # not-applicable 时必填
basis: "" # not-applicable 时必填：判断依据
alternative_checks: [] # not-applicable 时必填：覆盖风险的替代检查
```

<!-- Web/PWA 层用 browser prelude（默认可开）；桌面/原生层用 computer prelude（需显式开启，
     见 .omp/e2e.yml）。只启动成功、退出码为零、截图都不算通过，必须断言可观察行为。 -->

## Risks

<!-- 已知风险与缓解措施。与 design 的 Risks 不同：这里只写执行层面的。 -->
