# 开发工作流长期回归用例

来源变更：isolated-delivery-workflow；历史首次定义保留在 openspec/changes/archive/2026-10-28-isolated-delivery-workflow/tests.md。该路径是出处说明，不作为当前用例或执行入口。本套件维护当前工作流要求，不回写历史定义。

每次执行选定当前工作流固定快照，记录提交及差异文件SHA-256、OS、Node/npm与依赖；禁止绑定首次交付基线。配置权威是 openspec/config.yaml，职责权威是 openspec/role/{coder,tester,reviewer}.md。用例的实现位于 scripts/tests/，执行报告独立输出 openspec/testing/runs/<run-id>/。角色编排的语义由独立Agent按规范检验，不将CLI样例称为完整AI编排自动化。Schedule只是未来执行选择；第一阶段未接入定时调度，第二阶段以当时确认范围实施，不代表日/周用例已经运行。

## Levels

| Level | Applicability | Reason |
| --- | --- | --- |
| L1 | not-applicable | 本套件针对开发工作流规则与JavaScript检查器，不定义产品Rust/TypeScript类型验收；类型改动由对应能力套件承担，此处以schema与脚本语法替代。 |
| L2 | required | 验证资产、链接、层级、字段与规则结构。 |
| L3 | required | 检查器有自有纯函数逻辑，须覆盖正向和逐项负向。 |
| L4 | required | 验证真实CLI依赖、门禁和归档协作。 |
| L5 | required | 独立Agent验证探索、角色、并行、版本和问题闭环行为。 |
| L6 | required | 真实CLI完整工作流生命周期；无产品页面修改，不要求浏览器产品端到端。 |
| LX | not-applicable | 本套件没有必须人类设备操作或主观体验判断；工作流语义由独立Agent验收，不冒充人工；新增人类专属要求时重新评估。 |

## Cases

### WF-001 完整测试文档与合法边界

- Level: L3
- Requirement: config.yaml 的 Validation Levels and Tools 与 rules.tests；schemas/spec-driven/templates/tests.md固定格式。
- Executor: automation
- Tool: node:test；validateTestDocument
- Preconditions: 准备完整合法文本及LF/CRLF变体、合法not-applicable理由；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 调用纯函数验证完整文本和等价行尾文本。
- Expected: 各返回空错误数组；不因中文字段值或合法行尾误拒绝；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,pr,daily,on-demand
- Entry: node --test scripts/tests/openspec-tests.test.mjs。

### WF-002 字段缺失与非法值逐一拒绝

- Level: L3
- Requirement: config.yaml rules.tests；schemas/spec-driven/templates/tests.md必填字段。
- Executor: automation
- Tool: node:test；validateTestDocument
- Preconditions: 从同一完整合法文本每次仅制造一种缺陷；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 独立删除ID、Level、Requirement、Executor、Tool、Preconditions、Environment、Steps、Expected、Cleanup、Schedule、Entry；逐一置空；注入未知Level、Executor、Schedule。
- Expected: 每种非法输入返回非空错误数组且定位字段或用例；正样例仍通过；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,pr,daily,on-demand
- Entry: node --test scripts/tests/openspec-tests.test.mjs。

### WF-003 重复ID和层级表负向

- Level: L3
- Requirement: config.yaml rules.tests 与 Validation Levels and Tools；测试模板稳定ID和层级约束。
- Executor: automation
- Tool: node:test；validateTestDocument
- Preconditions: 准备单缺陷文本；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 分别注入重复用例ID、非法ID、缺层级行、重复层级行、非法Applicability、空Reason、无Cases或无用例；逐一调用。
- Expected: 每例独立被拒绝；七层恰好各一次的正样例通过；不得借其他错误掩盖；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,pr,daily,on-demand
- Entry: node --test scripts/tests/openspec-tests.test.mjs。

### WF-004 资产完整性及历史豁免

- Level: L3
- Requirement: config.yaml Exploration and Execution 与 Validation Levels and Tools；role三角色必需输入及长期套件约定。
- Executor: automation
- Tool: node:test；validateWorkflowAssets
- Preconditions: 临时目录有最小合法角色、术语、活动proposal/plan/tests与长期suites；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 逐一移除GLOSSARY、三个role中任一个、活动proposal、活动plan、活动tests或损坏长期suite；移除活动proposal的Exploration或Success Criteria节；破坏活动plan的必备节、工作包表或Main E2E一致性；单独加入无tests历史归档；测试相似archive名称普通活动目录。
- Expected: 必需资产和活动用例损坏、活动proposal缺失或缺必备节、活动plan结构损坏返回错误；历史归档不追溯补写；相似名称不能取得历史豁免；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,pr,daily,on-demand
- Entry: node --test scripts/tests/openspec-tests.test.mjs。

### WF-005 真实门禁与当前文档链接

- Level: L2
- Requirement: config.yaml Standard Commands、rules.tests、operations.archive；当前合同检查入口。
- Executor: automation
- Tool: Node；npm；Git
- Preconditions: 固定快照依赖已安装；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 执行脚本语法检查、单元测试、check:openspec、check:docs和npm run check；保存各退出码。
- Expected: 全部为0；当前ADR链接可解析；结构通过不被记录为语义验收通过；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,pr,daily,on-demand
- Entry: node --check scripts/check-openspec.mjs；node --test scripts/tests/openspec-tests.test.mjs；npm run check。

### WF-006 需求分支与术语维护

- Level: L5
- Requirement: config.yaml Exploration and Execution、rules.proposal；role/reviewer.md方案与用例检视。
- Executor: agent
- Tool: Git；文本检索；独立Agent
- Preconditions: 当前受测版本确认要求及临时skip_specs样例proposal可读取；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 逐项映射原需求至决定、选项理由及场景；检查每轮探索等待用户回答后才推进依赖该回答的分支，结束时与用户确认共同理解；注入用户沉默、尚未回答便推进、遗漏错误/取消分支或未决关键决定的示例；核对选定样例的GLOSSARY维护记录。
- Expected: 遗漏分支、关键未决项、把沉默当接受或跳过最终共同理解确认均产生阻断问题ID；依赖用户回答的工作等待真实回答；术语新增、修订或检查无需变化均有记录；不凭全覆盖口号通过；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,weekly,on-demand
- Entry: 独立读取当前临时样例proposal/design/plan/tests及GLOSSARY，逐项对照当前config规则与role职责。

### WF-008 CLI产物依赖与缺tests阻断

- Level: L4
- Requirement: config.yaml rules.tests、rules.plan、operations.apply；schemas/spec-driven/schema.yaml产物依赖。
- Executor: agent
- Tool: 仓库锁定OpenSpec CLI；Node
- Preconditions: 临时副本使用新schema；遥测和更新检查关闭；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 建立最小skip_specs变更，按proposal/specs/design/tests/plan/tasks顺序查询并生成规划，其中specs按skip_specs显式跳过；读取status及instructions tests/plan/apply JSON；缺tests时查询plan和apply，再补全重查。
- Expected: tests是真实必产物且在plan之前；缺tests时plan与apply均被阻断；补齐后可恢复；运行时下发模板与规则一致；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,weekly,on-demand
- Entry: 分别执行 node node_modules/@fission-ai/openspec/bin/openspec.js status --change <临时变更> --json、node node_modules/@fission-ai/openspec/bin/openspec.js instructions tests --change <临时变更> --json、node node_modules/@fission-ai/openspec/bin/openspec.js instructions apply --change <临时变更> --json。

### WF-009 主动并行与独立输入

- Level: L5
- Requirement: config.yaml Exploration and Execution、rules.plan、operations.apply；role/coder.md、tester.md、reviewer.md独立性。
- Executor: agent
- Tool: Git worktree；角色任务记录；独立Agent
- Preconditions: 存在两个互不重叠工作包和一个有依赖工作包的计划；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 检查是否主动评估并行；确认可并行包在不同工作区、固定基线和写范围；重叠或依赖串行；查验tester/reviewer无coder对话继承；查验编码前方案和用例检视。
- Expected: 可并行工作实际隔离；无并行写同工作区；独立角色自行调查且自评不替代独立结论；缺前置检视不可开始编码；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,weekly,on-demand
- Entry: 检查plan包表、派发记录和git worktree list；对照role职责。

### WF-010 同一版本证据与问题闭环

- Level: L5
- Requirement: config.yaml Exploration and Execution、operations.apply、operations.archive；三角色交付与问题闭环。
- Executor: agent
- Tool: Git；快照文件hash；独立报告
- Preconditions: 准备S1/S2版本及问题F-001示例；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 核验tester/reviewer目标；确认固定快照覆盖规则、脚本、模板和用例；模拟同HEAD不同工作树及S1/S2不一致；核对原始独立报告、plan结果与tasks勾选分别留证；排除结果记录时记录明确路径范围及理由；对F-001记录发现、修复、复验与关闭；检查修复后的受影响用例重验；归档路径变化单独复核。
- Expected: 不一致版本不可结束；证据含提交及未提交差异hash或不可变快照标识，覆盖规则、脚本、模板和用例；原始报告不可由plan自评或tasks勾选替代；排除结果记录范围明确且不排除被验资产；归档路径变化有单独复核证据；每个阻断问题同ID闭环；未执行不写通过；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,weekly,on-demand
- Entry: 对照独立报告目标版本及问题表，复算快照标识。

### WF-011 长期回归与用例结果分离

- Level: L5
- Requirement: config.yaml Validation Levels and Tools 与 rules.tests；role/coder.md验收边界及role/tester.md闭环与回归。
- Executor: agent
- Tool: 文件对照；独立Agent
- Preconditions: 当前临时样例tests和长期能力suite可读；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 核验每例字段、层级适用性和能力归属；确认coder维护单元测试、tester维护验收预期；注入coder为了消除失败擅改验收预期的反例；再模拟tester确认需求确已变化，先更新specs并经过复核再变更验收用例；确认稳定回归进入能力suite；模拟执行失败并检查结果记录位置。
- Expected: coder可以维护单元测试但擅改验收预期会被阻断；tester确认需求变更后先维护specs并经复核，再更新验收预期，不以消除失败为理由放宽要求；结果单独记录ID、版本和证据；历史tests保留；失败或未执行不勾完成；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,weekly,on-demand
- Entry: 读取tests、openspec/testing/suites及执行记录并逐ID核对。

### WF-012 归档保留与PR前顺序

- Level: L4
- Requirement: config.yaml operations.archive 与 Validation Levels and Tools；role/reviewer.md归档后复核。
- Executor: agent
- Tool: Git；文件hash；锁定OpenSpec CLI
- Preconditions: 临时副本中合法变更已完成验证；保存归档前文件hash；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 同步适用规范及长期suite；归档临时变更；比较tests及历史文件hash；对归档结果再跑门禁；核验实际PR准备时序。
- Expected: tests原样随变更归档；suite长期保留；既有历史不改写；归档后集成结果验证通过且在PR前；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,weekly,on-demand
- Entry: node node_modules/@fission-ai/openspec/bin/openspec.js archive <临时变更> --yes；npm run check。

### WF-013 工作流完整生命周期

- Level: L6
- Requirement: config.yaml Exploration and Execution、Validation Levels and Tools、operations.apply/archive；三角色全过程职责。
- Executor: agent
- Tool: 锁定OpenSpec CLI；Git；Node；独立Agent
- Preconditions: 固定快照独立临时副本；不创建真实PR；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 走需求分支探索、术语维护、规划、独立方案及用例检视、隔离实施、同快照测试和检视、问题闭环、长期suite更新、归档、归档后验证全链路。
- Expected: 所有阶段输入输出及固定版本证据可追溯；缺任一阶段不可宣称完成；真实CLI行为与规则一致；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,weekly,on-demand
- Entry: 在当前受测版本的临时副本创建独立skip_specs样例，执行WF-006及WF-008至WF-012；真实CLI控制产物状态与归档，独立Agent验证角色交接规范和问题闭环，不宣称CLI自动安排角色。

### WF-014 浏览器策略与调度阶段边界

- Level: L2
- Requirement: config.yaml Validation Levels and Tools 的浏览器工具分工与阶段性调度约定。
- Executor: agent
- Tool: Git diff；rg；独立Agent
- Preconditions: 最终规则与CI配置可读；负向样例每次只注入一个缺陷，先确认合法基线通过。
- Environment: 固定快照的独立临时工作区；Node满足package.json engines、锁定依赖已安装；记录OS、Node/npm、提交及差异hash；不修改主工作区。
- Steps: 检索浏览器分工及CI配置改动；按当时用户确认的阶段和范围核对调度：首次工作流阶段没有新增定时CI，第二阶段允许经确认后接入；检查仍残留的默认串行、自评代独立检视等冲突规则。
- Expected: AI操作浏览器优先ego-browser；稳定脚本回归优先脚本；调度改动符合当时确认阶段与范围，不能把第二阶段获准的定时CI误判为回归；当前规则无相互冲突；拒绝必须是非空错误数组、非零退出码或明确blocked状态，警告后成功不算拒绝。
- Cleanup: 保存输入、输出和退出码到独立执行记录；仅清理本例拥有的临时副本及资源，不改主工作区或既有归档。
- Schedule: acceptance,weekly,on-demand
- Entry: git diff -- .github openspec/config.yaml openspec/schemas openspec/role AGENTS.md。


