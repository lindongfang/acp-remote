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
alternative_checks: [] # not-applicable 时为非空字符串列表
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

<!-- 以下摘要必须入库，原始runs报告是附加证据，不能只填报告路径或tasks状态。
按config判定证据有效性和复用，不复制用例步骤。未执行保持真实状态。 -->

- 受测版本：<!-- 提交或完整文件清单/删除项及SHA-256；清单必须入库或可由提交重建 -->
- 摘要排除范围：<!-- plan结果、tasks状态及报告；不能排除实现/配置/测试变化 -->
- 执行者与环境：<!-- tester/reviewer、OS、依赖及工具版本 -->
- 验证命令及目录：<!-- 实际执行命令、退出码和检查范围 -->

| Case ID | Executor | Target Version | Status | Evidence / Limitations |
| --- | --- | --- | --- | --- |
| <!-- 用例ID --> | <!-- 独立执行者 --> | <!-- 固定版本 --> | <!-- pass/fail/not-run --> | <!-- 可独立判断的断言结果/限制，报告仅附加 --> |

- 未执行项与已知限制：<!-- 明确none或列出范围及影响 -->
- 问题关闭依据：<!-- 稳定ID、独立复验者和版本；无问题明确none -->
- 新版本证据判断：<!-- 变化、影响范围、重验与复用依据、独立确认者 -->
- 归档后复核：<!-- 移动映射、摘要一致性、受影响检查及独立结论；归档前明确待执行 -->
