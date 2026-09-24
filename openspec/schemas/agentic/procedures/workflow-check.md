# Workflow Structural Check

`openspec-agentic workflow check --change <name> --stage plan|final|archive --json`
始终只读，退出码 0 表示结构 PASS，1 表示不一致，2 表示材料/解析/目标不可用（BLOCKED）。
工作目录是代码仓库，`--planning-root` 指向权威规划根；变更位置仍由引擎解析。

| 阶段 | 输入与完成条件 |
| --- | --- |
| plan | proposal 的 agentic-intent、plan 的 agentic-coverage、design、规范/既有契约、tasks；校验意图字段、源标题、需求/场景覆盖、任务和检查引用、两个门禁标记各唯一且分开。证据可尚不存在。 |
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

机器只比对本地 Git 引用；默认合入本地 refs/heads/*，refs/remotes/* 是缓存，不证明远端实时状态。
用户明确要求远端目标时按计划刷新/查询，
由最终审计核实。合入入口应在验证后使用固定提交和条件更新防竞态；检查本身不获取远端锁。
检查不会验证真实用户批准、角色隔离、断言语义或部署实例身份，这些仍按 acceptance.md 审计。
内容摘要可检测变化，不是签名或防篡改证明。

受控归档入口应先执行 --stage archive，只有退出码为 0 且语义验收仍有效才调用上游 archive。
CI 必须针对指定 --change 检查，不能用“全部变更扫描 PASS”代替。该扩展不修改上游 archive/merge，
直接调用上游命令仍可能绕过；项目需在自身受控入口/分支保护中配置强制调用。

E2E 用语统一：每轮最终完整 E2E 只运行一个聚合入口，分片在入口内部执行；失败修复后可以新开一轮，
仍遵守重试上限。四个顺序步骤分别是执行、汇总和清理、[e2e-owned] 门禁检查、[final-verification] 验收。
“执行一次”从不表示整个变更只能尝试一次；门禁行不承担执行或汇总任务。
