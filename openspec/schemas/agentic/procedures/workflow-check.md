# Workflow Structural Check

计划门同时检查项目 E2E 配置和 `### Main E2E` 决策：`mode` 必须为 `required` 或 `not-applicable`；后者必须填写 `reason`、`basis`、非空 `alternative_checks`，且项目 E2E 启用时必须填写 `downgrade_approval`。配置解析错误使检查 BLOCKED。最终 E2E 门只接受显式 `stage=final` 的执行记录；未标阶段和候选记录均不满足最终门。

`openspec-agentic workflow check --change <name> --stage plan|premerge|final|archive --json`
始终只读，退出码 0 表示结构 PASS，1 表示不一致，2 表示材料/解析/目标不可用（BLOCKED）。
工作目录是代码仓库，`--planning-root` 指向权威规划根；变更位置仍由引擎解析。

`premerge` 在候选提交工作区执行，要求 `HEAD` 为候选提交、本地 `target_ref` 仍指向规划基线，且候选包含该基线。`verification.md` 中的唯一 `agentic-premerge` 块记录候选 Project Verify、独立 review 的报告路径和摘要；`required` 时以 `plan.md` 的 E2E Ownership and Cases 候选行为权威 ID 清单，要求 E2E Execution Waves 当前单元的分片 ID 与之完全一致，再逐 ID 核对带 `--cases` 的最近一条 `stage=candidate` 记录及其计划分片命令。候选命令可以不同于项目配置的最终聚合命令；最终门仍要求聚合命令一致。不同 ID 可由不同分片留证，某 ID 的失败不能被另一 ID 的成功覆盖。`not-applicable` 时逐项核对计划中的 `alternative_checks` 与候选 PASS 报告摘要。任一证据失效均非零退出。示例 CI 模板位于 `ci/github-premerge.yml`；使用方需复制到 `.github/workflows/` 并将该检查设为受保护分支的必需状态。机器只能核对报告结构、版本和摘要，reviewer 身份与真实测试内容仍需平台审批和人工审查。

```agentic-premerge
version: 1
delivery_unit: <计划中的交付单元 ID；required 时必填>
target_ref: refs/heads/main
target_commit: <合入前目标提交 SHA>
candidate_commit: <候选 HEAD SHA>
contract_digest: <workflow check --stage plan 输出的摘要>
verify:
  result: PASS
  candidate_commit: <候选 HEAD SHA>
  evidence: {path: reports/candidate-verify.log, sha256: "sha256:<报告摘要>"}
review:
  result: PASS
  candidate_commit: <候选 HEAD SHA>
  reviewer: <独立检视人 ID>
  author: <代码作者 ID>
  evidence: {path: reports/candidate-review.log, sha256: "sha256:<报告摘要>"}
# mode 为 not-applicable 时，逐项列出计划中的 alternative_checks：
alternative_checks:
  - name: <计划中的检查名>
    result: PASS
    candidate_commit: <候选 HEAD SHA>
    evidence: {path: reports/alternative.log, sha256: "sha256:<报告摘要>"}
```

| 阶段 | 输入与完成条件 |
| --- | --- |
| plan | proposal 的 agentic-intent、plan 的 agentic-coverage、design、规范/既有契约、tasks；校验意图字段、源标题、需求/场景覆盖、任务和检查引用、两个门禁标记各唯一且分开，Main E2E 决策完整。证据可尚不存在。 |
| premerge | plan 的全部要求，加当前候选提交、未移动的目标基线、候选 Verify 与独立 review 报告摘要；required 时核对权威候选 ID 清单、Waves 完整分配及逐 ID 的命令与 PASS，not-applicable 时逐项核对替代检查 PASS。 |
| final | 上述全部，加 verification 的唯一 agentic-assessment；HEAD、计划目标引用和验收提交一致；契约/原始证据摘要一致；只允许最终验收任务待办；只读 e2e check PASS。 |
| archive | final 的全部要求，且包括最终验收在内的所有任务完成；检查发生在归档操作之前。 |

输出 contractDigest 是 proposal、design、plan、tasks、元数据、全部增量规范和覆盖索引引用的契约
内容摘要；任务复选框进度不参与。evidence 返回已存在的计划报告的路径和 SHA-256，供审计后留存。
规范标题/任务描述/计划变化都会使旧契约失效。required E2E 每轮执行记录保存该摘要，旧记录未绑定时需重跑。
规划字段应在正式验证前固定；后续用例/任务调整先做影响分析，不为刷新摘要而跳过复验。

路径相对 changeDir（也可绝对路径），证据应使用独立且可持久读取的版本化报告文件，不引用
verification.md 自身，避免自引用摘要。当前验收块只保留一个，原轮次正文和原始报告持续保留。
检查不写报告、不勾选任务、不运行 E2E；E2E owned 行仍由 e2e check 独占更新。

兼容旧变更：旧 e2e 命令在没有 agentic-coverage 时保留既有行为；新 workflow check 不默默放行，
报告缺失材料。恢复旧变更时依据可核实资料补齐，不编造历史授权或证据。

机器只比对本地 Git 引用；计划中的目标须为已核实的本地主分支 refs/heads/*。
合入入口应在验证后使用固定提交和条件更新防竞态；检查本身不锁定目标分支。
检查不会验证真实用户批准、角色隔离、断言语义或部署实例身份，这些仍按 acceptance.md 审计。
内容摘要可检测变化，不是签名或防篡改证明。

受控归档入口应先执行 --stage archive，只有退出码为 0 且语义验收仍有效才调用上游 archive。
CI 必须针对指定 --change 检查，不能用“全部变更扫描 PASS”代替。该扩展不修改上游 archive/merge，
直接调用上游命令仍可能绕过；项目需在自身受控入口/分支保护中配置强制调用。

E2E 用语统一：每轮最终完整 E2E 只运行一个聚合入口，分片在入口内部执行；失败修复后可以新开一轮，
仍遵守重试上限。四个顺序步骤分别是执行、汇总和清理、[e2e-owned] 门禁检查、[final-verification] 验收。
“执行一次”从不表示整个变更只能尝试一次；门禁行不承担执行或汇总任务。
