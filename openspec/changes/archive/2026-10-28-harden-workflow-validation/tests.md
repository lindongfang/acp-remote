# Tests

来源：harden-workflow-validation。独立tester编码前基线；执行结果另存testing/runs。每负例先确认正例通过，再仅注入一个缺陷。

## Levels

| Level | Applicability | Reason |
| --- | --- | --- |
| L1 | not-applicable | 无Rust/TypeScript类型变更；JS语法及YAML类型由L2/L3替代 |
| L2 | required | 合同、链接与依赖登记 |
| L3 | required | 三处误判的正常及反例 |
| L4 | required | 真实CLI及阶段依赖 |
| L5 | required | 角色及证据规则语义 |
| L6 | not-applicable | 无产品端到端变化；真实工具CLI列L4，不冒充完整AI角色自动编排 |
| LX | not-applicable | 无人类专属操作，Agent不冒充人工 |

## Cases

### WF-HARDEN-001 统一检查

- Level: L2
- Requirement: proposal.md 的 Success Criteria：误判修复及流程补充；design.md Decisions。
- Executor: automation
- Tool: Node内置测试器、真实锁定OpenSpec CLI、Git、独立Agent。
- Preconditions: 固定提交和差异文件SHA-256清单；锁定依赖已安装；合法基线及单缺陷反例。
- Environment: Windows独立临时目录/数据/进程；真实解析器与CLI、模拟规划数据；不接外部服务，不触碰用户资源。
- Steps: 运行JS语法检查、npm run test:workflow及npm run check。
- Expected: 全退出0，记录真实用例数，零用例或结构通过不冒充语义验收。
- Cleanup: 保存输入、输出、退出码及逐ID结果；成功/失败/取消均只清理自身临时资源，CLI超时30秒。
- Schedule: acceptance,pr,on-demand
- Entry: 仓库根运行node --test scripts/tests/openspec-tests.test.mjs及npm run check；L3逐例调用validatePlanDocument/validateTestDocument/validateWorkflowAssets，L4执行真实CLI status/instructions --json；L5独立逐条对照config、schema、role、模板及交付plan记录证据。

### WF-HARDEN-002 精确任务关联

- Level: L3
- Requirement: proposal.md 的 Success Criteria：误判修复及流程补充；design.md Decisions。
- Executor: automation
- Tool: Node内置测试器、真实锁定OpenSpec CLI、Git、独立Agent。
- Preconditions: 固定提交和差异文件SHA-256清单；锁定依赖已安装；合法基线及单缺陷反例。
- Environment: Windows独立临时目录/数据/进程；真实解析器与CLI、模拟规划数据；不接外部服务，不触碰用户资源。
- Steps: WP1合法基线逐一变成WP10/WP1-extra/注释/围栏/标题/普通段落；测试未知ID、空任务及多个声明ID、反引号、中文标点正例。
- Expected: 只有真实checkbox正文完整ID满足关联；每任务至少引用一个声明ID；反例拒绝且正常并列引用通过。
- Cleanup: 保存输入、输出、退出码及逐ID结果；成功/失败/取消均只清理自身临时资源，CLI超时30秒。
- Schedule: acceptance,pr,on-demand
- Entry: 仓库根运行node --test scripts/tests/openspec-tests.test.mjs及npm run check；L3逐例调用validatePlanDocument/validateTestDocument/validateWorkflowAssets，L4执行真实CLI status/instructions --json；L5独立逐条对照config、schema、role、模板及交付plan记录证据。

### WF-HARDEN-003 YAML类型与空值

- Level: L3
- Requirement: proposal.md 的 Success Criteria：误判修复及流程补充；design.md Decisions。
- Executor: automation
- Tool: Node内置测试器、真实锁定OpenSpec CLI、Git、独立Agent。
- Preconditions: 固定提交和差异文件SHA-256清单；锁定依赖已安装；合法基线及单缺陷反例。
- Environment: Windows独立临时目录/数据/进程；真实解析器与CLI、模拟规划数据；不接外部服务，不触碰用户资源。
- Steps: 对reason/basis/alternative_checks逐一测试省略、null/Null/NULL/~、单/双引号空串、空白串、[]/[ ]；测试引号null与合法列表；注入数值、布尔、映射、嵌套列表、重复键、别名、语法错误、多个配置块。
- Expected: not-applicable前两字段非空字符串、后者非空字符串列表；required只接受字段合法类型的空值或省略；引号null非空；错误类型/重复键/别名/不唯一块拒绝，正常两模式通过。
- Cleanup: 保存输入、输出、退出码及逐ID结果；成功/失败/取消均只清理自身临时资源，CLI超时30秒。
- Schedule: acceptance,pr,on-demand
- Entry: 仓库根运行node --test scripts/tests/openspec-tests.test.mjs及npm run check；L3逐例调用validatePlanDocument/validateTestDocument/validateWorkflowAssets，L4执行真实CLI status/instructions --json；L5独立逐条对照config、schema、role、模板及交付plan记录证据。

### WF-HARDEN-004 路径根隔离

- Level: L3
- Requirement: proposal.md 的 Success Criteria：误判修复及流程补充；design.md Decisions。
- Executor: automation
- Tool: Node内置测试器、真实锁定OpenSpec CLI、Git、独立Agent。
- Preconditions: 固定提交和差异文件SHA-256清单；锁定依赖已安装；合法基线及单缺陷反例。
- Environment: Windows独立临时目录/数据/进程；真实解析器与CLI、模拟规划数据；不接外部服务，不触碰用户资源。
- Steps: 磁盘样例分别仅delta、仅长期、同名两者、皆缺；活动tests测试裸specs与显式openspec/specs，suite测试裸路径，目录冒充文件。
- Expected: 活动裸路径只当前delta，显式路径只长期，suite裸路径只长期；不得跨根回退，目录拒绝，正确路径通过。
- Cleanup: 保存输入、输出、退出码及逐ID结果；成功/失败/取消均只清理自身临时资源，CLI超时30秒。
- Schedule: acceptance,pr,on-demand
- Entry: 仓库根运行node --test scripts/tests/openspec-tests.test.mjs及npm run check；L3逐例调用validatePlanDocument/validateTestDocument/validateWorkflowAssets，L4执行真实CLI status/instructions --json；L5独立逐条对照config、schema、role、模板及交付plan记录证据。

### WF-HARDEN-005 规划阶段和真实CLI

- Level: L4
- Requirement: proposal.md 的 Success Criteria：误判修复及流程补充；design.md Decisions。
- Executor: automation
- Tool: Node内置测试器、真实锁定OpenSpec CLI、Git、独立Agent。
- Preconditions: 固定提交和差异文件SHA-256清单；锁定依赖已安装；合法基线及单缺陷反例。
- Environment: Windows独立临时目录/数据/进程；真实解析器与CLI、模拟规划数据；不接外部服务，不触碰用户资源。
- Steps: 递增创建空变更、proposal/design、tests、plan、tasks并逐步校验；另测试design独自出现缺proposal及逐一下游缺前置；真实CLI读取status及instructions tests/plan/apply；用真实check-openspec全入口验证无specs的空/proposal/design前缀通过，坏delta、空specs目录、缺前置、未skip但tests出现且无spec、坏长期规范拒绝。
- Expected: 空变更和合法规划前缀通过，现存产物立即校验；design要求proposal，tests要求proposal/design，plan要求tests，tasks要求proposal/design/tests/plan；缺tests阻断plan/apply，补齐恢复；历史归档不追溯；长期spec始终全量strict，只有无specs且无tests/plan/tasks且非skip_specs的规划前缀暂缓delta strict；已有specs（含空目录）、下游或skip_specs必须逐变更strict。
- Cleanup: 保存输入、输出、退出码及逐ID结果；成功/失败/取消均只清理自身临时资源，CLI超时30秒。
- Schedule: acceptance,pr,on-demand
- Entry: 仓库根运行node --test scripts/tests/openspec-tests.test.mjs及npm run check；L3逐例调用validatePlanDocument/validateTestDocument/validateWorkflowAssets，L4执行真实CLI status/instructions --json；L5独立逐条对照config、schema、role、模板及交付plan记录证据。

### WF-HARDEN-006 首次无plan及场景关联

- Level: L5
- Requirement: proposal.md 的 Success Criteria：误判修复及流程补充；design.md Decisions。
- Executor: agent
- Tool: Node内置测试器、真实锁定OpenSpec CLI、Git、独立Agent。
- Preconditions: 固定提交和差异文件SHA-256清单；锁定依赖已安装；合法基线及单缺陷反例。
- Environment: Windows独立临时目录/数据/进程；真实解析器与CLI、模拟规划数据；不接外部服务，不触碰用户资源。
- Steps: 独立核对config/schema/role/模板；无plan时据proposal/specs/design编tests，plan生成后复核安排；核对文件、Requirement、Scenario及逐项覆盖责任。
- Expected: 首次无plan不阻塞基线，开工仍需完整产物与独立检视；文件存在不代表场景存在或覆盖，reviewer逐项复核；本次不扩大自动覆盖算法。
- Cleanup: 保存输入、输出、退出码及逐ID结果；成功/失败/取消均只清理自身临时资源，CLI超时30秒。
- Schedule: acceptance,pr,on-demand
- Entry: 仓库根运行node --test scripts/tests/openspec-tests.test.mjs及npm run check；L3逐例调用validatePlanDocument/validateTestDocument/validateWorkflowAssets，L4执行真实CLI status/instructions --json；L5独立逐条对照config、schema、role、模板及交付plan记录证据。

### WF-HARDEN-007 持久摘要

- Level: L5
- Requirement: proposal.md 的 Success Criteria：误判修复及流程补充；design.md Decisions。
- Executor: agent
- Tool: Node内置测试器、真实锁定OpenSpec CLI、Git、独立Agent。
- Preconditions: 固定提交和差异文件SHA-256清单；锁定依赖已安装；合法基线及单缺陷反例。
- Environment: Windows独立临时目录/数据/进程；真实解析器与CLI、模拟规划数据；不接外部服务，不触碰用户资源。
- Steps: 模拟原始runs报告不可访问，核对模板及实际plan的受测版本、执行者、逐ID状态、未执行项、问题关闭、限制及归档复核；缺项记问题。
- Expected: 入库摘要可独立判断通过与缺口；原始报告链接、tasks勾选、自评不能替代摘要；结果不混入用例定义。
- Cleanup: 保存输入、输出、退出码及逐ID结果；成功/失败/取消均只清理自身临时资源，CLI超时30秒。
- Schedule: acceptance,pr,on-demand
- Entry: 仓库根运行node --test scripts/tests/openspec-tests.test.mjs及npm run check；L3逐例调用validatePlanDocument/validateTestDocument/validateWorkflowAssets，L4执行真实CLI status/instructions --json；L5独立逐条对照config、schema、role、模板及交付plan记录证据。

### WF-HARDEN-008 变更证据失效

- Level: L5
- Requirement: proposal.md 的 Success Criteria：误判修复及流程补充；design.md Decisions。
- Executor: agent
- Tool: Node内置测试器、真实锁定OpenSpec CLI、Git、独立Agent。
- Preconditions: 固定提交和差异文件SHA-256清单；锁定依赖已安装；合法基线及单缺陷反例。
- Environment: Windows独立临时目录/数据/进程；真实解析器与CLI、模拟规划数据；不接外部服务，不触碰用户资源。
- Steps: S1通过后S2修改实现/合同/需求/预期；另测纯路径移动、纯结果记录变化；核对影响分析、受影响重验/复查及快照。
- Expected: 同HEAD不同工作树不等同；实质变化需独立判断证据复用并重验受影响项；纯移动需映射及内容一致证据；结果记录明确分离且不掩盖被验资产变化。
- Cleanup: 保存输入、输出、退出码及逐ID结果；成功/失败/取消均只清理自身临时资源，CLI超时30秒。
- Schedule: acceptance,pr,on-demand
- Entry: 仓库根运行node --test scripts/tests/openspec-tests.test.mjs及npm run check；L3逐例调用validatePlanDocument/validateTestDocument/validateWorkflowAssets，L4执行真实CLI status/instructions --json；L5独立逐条对照config、schema、role、模板及交付plan记录证据。

