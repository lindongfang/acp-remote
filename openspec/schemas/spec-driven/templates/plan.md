<!-- 计划描述执行安排，不重复 specs/design。遵循 openspec/config.yaml 的 rules/operations，不绑定宿主、模型或子代理 API。 -->

## Execution Model

- 执行：主执行者统筹，coder实现、tester验收、reviewer检视；记录基线和用户未提交改动。
- 隔离：tester/reviewer不继承coder对话；并行写入使用独立工作区、构建目录和资源。
- 交付：实际 diff、patch 或提交，加变更说明与验证结果；不要求私有会话 URI。
- 合入：直接修改核对 diff；patch 先校验再应用；提交按依赖合入。
- 验证：写明实际目录和依赖，最终检查覆盖待提交的集成结果。
- 复核：独立方案/用例检视通过后编码；tester/reviewer对同一固定版本独立验收/检视。

## Parallel Assessment

<!-- 主动列出能并行的工作；串行说明依赖、重叠或工具限制。 -->

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- WP1 --> | <!-- 需求与可观察结果 --> | <!-- none / WPn --> | <!-- 实际执行者，不限角色名 --> | <!-- 相对路径 --> | <!-- diff / patch / commit 与报告 --> | <!-- 命令及执行目录 --> |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| <!-- 1 --> | <!-- WP1 --> | <!-- 依赖或范围重叠 --> | <!-- 集成后检查 --> |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| <!-- PV1 --> | <!-- 引用tests.md用例ID、命令/目录/依赖 --> | <!-- 真实通过条件 --> |

## Code Review

<!-- 独立reviewer编码前检视方案与用例，交付后检视真实差异。
     与tester核对同一版本；自查不能代签，工具不足记录未完成。 -->

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| <!-- WP1 --> | <!-- 独立reviewer --> | <!-- 方案/用例/交付风险 --> | <!-- 无未闭环阻断问题 --> |

## Main E2E

```yaml
mode: not-applicable # required | not-applicable
reason: "" # not-applicable 时必填
basis: "" # not-applicable 时必填
alternative_checks: [] # not-applicable 时必填
```

<!-- 使用当前可用的真实浏览器或桌面测试工具，按行为选择真实组件，不绑定宿主接口。
     断言可观察结果，记录真实命令、环境、通过/失败/跳过，不能用截图或启动成功代替验收。 -->

## Risks

<!-- 执行风险、未执行检查、限制及缓解。 -->

## Handoff

<!-- 每角色收到用户确认要求、规则/产物路径、基线或快照、读写范围、工具及交付判据。
不传开发对话；tester独立测试环境，reviewer只读或隔离执行。 -->

## Findings

| ID | Source | Severity | Target Version | Finding | Fix Version | Status | Retest / Re-review |
| --- | --- | --- | --- | --- | --- | --- | --- |

<!-- coder不能代关独立问题；保留发现、修复、独立复验依据。 -->

## Verification Results

<!-- 不重复用例。记录提交或受测文件清单SHA-256、环境、逐用例结果摘要和报告位置。
明确被排除的结果记录/tasks勾选；同HEAD不同工作树不是同版本。
失败/未执行保持真实状态。CI同版本同范围证据可复用。
同步specs/长期能力用例后归档，再复核路径与实际集成结果，之后才准备PR。 -->
